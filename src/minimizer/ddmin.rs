use crate::db::error::FailureSignature;
use crate::generator::{DatabaseState, RowData, SqlValue, TableData};
use crate::minimizer::TestFn;
use crate::schema::{DataType, DatabaseSchema};
use std::collections::HashSet;

pub async fn reduce_rows<'a>(
    schema: &DatabaseSchema,
    initial_state: &DatabaseState,
    target_signature: Option<&FailureSignature>,
    test_fn: &TestFn<'a>,
) -> DatabaseState {
    let mut current_state = initial_state.clone();
    let mut table_order = schema.topological_order().unwrap_or_else(|_| {
        schema
            .tables
            .iter()
            .map(|table| table.name.clone())
            .collect()
    });
    table_order.reverse();

    for table_name in table_order {
        let Some(rows) = current_state
            .tables
            .get(&table_name)
            .map(|table| table.rows.clone())
        else {
            continue;
        };

        if rows.is_empty() {
            continue;
        }

        let empty_candidate = replace_table_rows(&current_state, &table_name, Vec::new());
        if is_valid_candidate(schema, &empty_candidate)
            && preserves_failure(test_fn, &empty_candidate, target_signature).await
        {
            current_state = empty_candidate;
            continue;
        }

        current_state = ddmin_table(
            schema,
            &current_state,
            &table_name,
            target_signature,
            test_fn,
        )
        .await;

        let mut index = 0;
        while index
            < current_state
                .tables
                .get(&table_name)
                .map(|table| table.rows.len())
                .unwrap_or(0)
        {
            let mut rows = current_state.tables[&table_name].rows.clone();
            rows.remove(index);
            let candidate = replace_table_rows(&current_state, &table_name, rows);
            if is_valid_candidate(schema, &candidate)
                && preserves_failure(test_fn, &candidate, target_signature).await
            {
                current_state = candidate;
            } else {
                index += 1;
            }
        }
    }

    current_state
}

async fn ddmin_table<'a>(
    schema: &DatabaseSchema,
    initial_state: &DatabaseState,
    table_name: &str,
    target_signature: Option<&FailureSignature>,
    test_fn: &TestFn<'a>,
) -> DatabaseState {
    let mut current_state = initial_state.clone();
    let mut granularity = 2usize;

    while let Some(rows) = current_state
        .tables
        .get(table_name)
        .map(|table| table.rows.clone())
    {
        if rows.len() < 2 {
            break;
        }

        let current_granularity = granularity.min(rows.len()).max(2);
        let chunks = partition_rows(rows.len(), current_granularity);
        let mut changed = false;

        for chunk in &chunks {
            let candidates = [
                chunk
                    .iter()
                    .map(|index| rows[*index].clone())
                    .collect::<Vec<_>>(),
                rows.iter()
                    .enumerate()
                    .filter(|(index, _)| !chunk.contains(index))
                    .map(|(_, row)| row.clone())
                    .collect::<Vec<_>>(),
            ];

            for candidate_rows in candidates {
                let candidate = replace_table_rows(&current_state, table_name, candidate_rows);
                if is_valid_candidate(schema, &candidate)
                    && preserves_failure(test_fn, &candidate, target_signature).await
                {
                    current_state = candidate;
                    granularity = current_granularity.saturating_sub(1).max(2);
                    changed = true;
                    break;
                }
            }
            if changed {
                break;
            }
        }

        if changed {
            continue;
        }

        if current_granularity >= rows.len() {
            break;
        }
        granularity = (current_granularity * 2).min(rows.len());
    }

    current_state
}

fn partition_rows(row_count: usize, parts: usize) -> Vec<Vec<usize>> {
    let parts = parts.min(row_count).max(1);
    (0..parts)
        .filter_map(|part| {
            let start = part * row_count / parts;
            let end = (part + 1) * row_count / parts;
            (start < end).then(|| (start..end).collect())
        })
        .collect()
}

pub async fn shrink_values<'a>(
    schema: &DatabaseSchema,
    initial_state: &DatabaseState,
    target_signature: Option<&FailureSignature>,
    test_fn: &TestFn<'a>,
) -> DatabaseState {
    let mut current_state = initial_state.clone();

    for table in &schema.tables {
        let Some(row_count) = current_state.tables.get(&table.name).map(|t| t.rows.len()) else {
            continue;
        };

        for row_idx in 0..row_count {
            for column in &table.columns {
                let Some(original) = current_state
                    .tables
                    .get(&table.name)
                    .and_then(|data| data.rows.get(row_idx))
                    .and_then(|row| row.values.get(&column.name))
                    .cloned()
                else {
                    continue;
                };

                for candidate_value in value_candidates(column, &original) {
                    let mut candidate_state = current_state.clone();
                    let Some(row) = candidate_state
                        .tables
                        .get_mut(&table.name)
                        .and_then(|data| data.rows.get_mut(row_idx))
                    else {
                        break;
                    };
                    row.values.insert(column.name.clone(), candidate_value);

                    if is_valid_candidate(schema, &candidate_state)
                        && preserves_failure(test_fn, &candidate_state, target_signature).await
                    {
                        current_state = candidate_state;
                        break;
                    }
                }
            }
        }
    }

    current_state
}

fn value_candidates(column: &crate::schema::Column, value: &SqlValue) -> Vec<SqlValue> {
    let mut candidates = Vec::new();

    if column.is_nullable && !matches!(value, SqlValue::Null) {
        candidates.push(SqlValue::Null);
    }

    match (value, &column.data_type) {
        (SqlValue::SmallInt(current), DataType::SmallInt) => {
            candidates.extend([0, -1, 1, i16::MIN, i16::MAX].map(SqlValue::SmallInt));
            candidates.retain(|candidate| candidate != &SqlValue::SmallInt(*current));
        }
        (SqlValue::Integer(current), DataType::Integer) => {
            candidates.extend([0, -1, 1, i32::MIN, i32::MAX].map(SqlValue::Integer));
            candidates.retain(|candidate| candidate != &SqlValue::Integer(*current));
        }
        (SqlValue::BigInt(current), DataType::BigInt) => {
            candidates.extend([0, -1, 1, i64::MIN, i64::MAX].map(SqlValue::BigInt));
            candidates.retain(|candidate| candidate != &SqlValue::BigInt(*current));
        }
        (SqlValue::Float(current), DataType::Real | DataType::DoublePrecision) => {
            candidates.extend([0.0, -1.0, 1.0].map(SqlValue::Float));
            candidates.retain(|candidate| candidate != &SqlValue::Float(*current));
        }
        (SqlValue::Numeric(current), DataType::Numeric { .. }) => {
            candidates.extend(
                ["0", "-1", "1", "0.1"]
                    .into_iter()
                    .map(|candidate| SqlValue::Numeric(candidate.to_string())),
            );
            candidates.retain(|candidate| candidate != &SqlValue::Numeric(current.clone()));
        }
        (SqlValue::Text(current), DataType::Text | DataType::Varchar(_) | DataType::Char(_)) => {
            if let Some(at) = current.find('@') {
                if at > 1 {
                    candidates.push(SqlValue::Text(format!(
                        "{}{}",
                        current.chars().next().unwrap_or_default(),
                        &current[at..]
                    )));
                }
            }
            candidates.push(SqlValue::Text(current.trim().to_string()));
            candidates.push(SqlValue::Text(current.to_ascii_lowercase()));
            candidates.push(SqlValue::Text(current.to_ascii_uppercase()));
            candidates.push(SqlValue::Text(current.chars().take(1).collect()));
            candidates.push(SqlValue::Text(
                current.chars().take(current.chars().count() / 2).collect(),
            ));
            candidates.retain(|candidate| candidate != value);
        }
        (SqlValue::Date(current), DataType::Date) => {
            candidates
                .extend(["1970-01-01", "2000-01-01"].map(|date| SqlValue::Date(date.to_string())));
            candidates.retain(|candidate| candidate != &SqlValue::Date(current.clone()));
        }
        (SqlValue::Timestamp(current), DataType::Timestamp | DataType::TimestampTz) => {
            candidates.extend(
                ["1970-01-01 00:00:00", "2000-01-01 00:00:00"]
                    .map(|date| SqlValue::Timestamp(date.to_string())),
            );
            candidates.retain(|candidate| candidate != &SqlValue::Timestamp(current.clone()));
        }
        _ => {}
    }

    candidates.dedup();
    candidates
}

async fn preserves_failure<'a>(
    test_fn: &TestFn<'a>,
    candidate: &DatabaseState,
    target_signature: Option<&FailureSignature>,
) -> bool {
    let result = test_fn(candidate).await;
    match target_signature {
        Some(expected) => result
            .as_ref()
            .map(|actual| actual.matches(expected))
            .unwrap_or(false),
        None => result.is_some(),
    }
}

fn replace_table_rows(
    state: &DatabaseState,
    table_name: &str,
    rows: Vec<RowData>,
) -> DatabaseState {
    let mut candidate = state.clone();
    if let Some(table) = candidate.tables.get_mut(table_name) {
        table.rows = rows;
    } else {
        candidate.tables.insert(
            table_name.to_string(),
            TableData {
                table_name: table_name.to_string(),
                rows,
            },
        );
    }
    candidate
}

fn is_valid_candidate(schema: &DatabaseSchema, state: &DatabaseState) -> bool {
    for table in &schema.tables {
        let Some(child_data) = state.tables.get(&table.name) else {
            continue;
        };
        for fk in &table.foreign_keys {
            let Some(parent_data) = state.tables.get(&fk.foreign_table) else {
                return child_data.rows.is_empty();
            };
            let parent_keys: HashSet<Vec<String>> = parent_data
                .rows
                .iter()
                .filter_map(|row| composite_key(row, &fk.foreign_columns))
                .collect();

            for child in &child_data.rows {
                let Some(child_key) = composite_key(child, &fk.columns) else {
                    continue;
                };
                if !parent_keys.contains(&child_key) {
                    return false;
                }
            }
        }
    }
    true
}

fn composite_key(row: &RowData, columns: &[String]) -> Option<Vec<String>> {
    let mut key = Vec::with_capacity(columns.len());
    for column in columns {
        match row.values.get(column) {
            Some(SqlValue::Null) | None => return None,
            Some(value) => key.push(value.to_sql_literal()),
        }
    }
    Some(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Column, ForeignKey, PrimaryKey, Table};
    use std::collections::HashMap;
    use std::pin::Pin;

    fn integer_column(name: &str) -> Column {
        Column {
            name: name.to_string(),
            data_type: DataType::Integer,
            is_nullable: false,
            default_value: None,
            is_identity: false,
            is_generated: false,
        }
    }

    fn signature() -> FailureSignature {
        FailureSignature::new(
            crate::db::error::FailureClass::UniqueViolation,
            Some("23505".to_string()),
            "duplicate key",
        )
    }

    #[tokio::test]
    async fn ddmin_reduces_rows_in_groups() {
        let schema = DatabaseSchema {
            tables: vec![Table {
                name: "events".to_string(),
                schema_name: "public".to_string(),
                columns: vec![integer_column("id")],
                primary_key: Some(PrimaryKey {
                    name: "events_pkey".to_string(),
                    columns: vec!["id".to_string()],
                }),
                foreign_keys: vec![],
                unique_constraints: vec![],
                check_constraints: vec![],
                indexes: vec![],
            }],
            enums: vec![],
        };
        let rows = (1..=16)
            .map(|id| RowData {
                values: HashMap::from([("id".to_string(), SqlValue::Integer(id))]),
            })
            .collect();
        let state = DatabaseState {
            tables: HashMap::from([(
                "events".to_string(),
                TableData {
                    table_name: "events".to_string(),
                    rows,
                },
            )]),
        };
        let test_fn: TestFn<'_> = Box::new(|candidate: &DatabaseState| {
            let reproduces = candidate.tables["events"]
                .rows
                .iter()
                .any(|row| row.values.get("id") == Some(&SqlValue::Integer(3)));
            Box::pin(async move { reproduces.then(signature) })
                as Pin<Box<dyn std::future::Future<Output = Option<FailureSignature>> + Send>>
        });

        let expected = signature();
        let minimized = reduce_rows(&schema, &state, Some(&expected), &test_fn).await;

        assert_eq!(minimized.tables["events"].rows.len(), 1);
        assert_eq!(
            minimized.tables["events"].rows[0].values["id"],
            SqlValue::Integer(3)
        );
    }

    #[tokio::test]
    async fn row_reduction_preserves_foreign_key_validity() {
        let schema = DatabaseSchema {
            tables: vec![
                Table {
                    name: "users".to_string(),
                    schema_name: "public".to_string(),
                    columns: vec![integer_column("id")],
                    primary_key: Some(PrimaryKey {
                        name: "users_pkey".to_string(),
                        columns: vec!["id".to_string()],
                    }),
                    foreign_keys: vec![],
                    unique_constraints: vec![],
                    check_constraints: vec![],
                    indexes: vec![],
                },
                Table {
                    name: "orders".to_string(),
                    schema_name: "public".to_string(),
                    columns: vec![integer_column("id"), integer_column("user_id")],
                    primary_key: Some(PrimaryKey {
                        name: "orders_pkey".to_string(),
                        columns: vec!["id".to_string()],
                    }),
                    foreign_keys: vec![ForeignKey {
                        name: "orders_user_fk".to_string(),
                        columns: vec!["user_id".to_string()],
                        foreign_table: "users".to_string(),
                        foreign_columns: vec!["id".to_string()],
                        on_delete: "NO ACTION".to_string(),
                        on_update: "NO ACTION".to_string(),
                    }],
                    unique_constraints: vec![],
                    check_constraints: vec![],
                    indexes: vec![],
                },
            ],
            enums: vec![],
        };
        let state = DatabaseState {
            tables: HashMap::from([
                (
                    "users".to_string(),
                    TableData {
                        table_name: "users".to_string(),
                        rows: vec![
                            RowData {
                                values: HashMap::from([("id".to_string(), SqlValue::Integer(1))]),
                            },
                            RowData {
                                values: HashMap::from([("id".to_string(), SqlValue::Integer(2))]),
                            },
                        ],
                    },
                ),
                (
                    "orders".to_string(),
                    TableData {
                        table_name: "orders".to_string(),
                        rows: vec![
                            RowData {
                                values: HashMap::from([
                                    ("id".to_string(), SqlValue::Integer(1)),
                                    ("user_id".to_string(), SqlValue::Integer(1)),
                                ]),
                            },
                            RowData {
                                values: HashMap::from([
                                    ("id".to_string(), SqlValue::Integer(2)),
                                    ("user_id".to_string(), SqlValue::Integer(2)),
                                ]),
                            },
                        ],
                    },
                ),
            ]),
        };
        let test_fn: TestFn<'_> = Box::new(|candidate: &DatabaseState| {
            let reproduces = candidate.tables["orders"].rows.len() == 2;
            Box::pin(async move { reproduces.then(signature) })
                as Pin<Box<dyn std::future::Future<Output = Option<FailureSignature>> + Send>>
        });

        let minimized = reduce_rows(&schema, &state, None, &test_fn).await;
        assert_eq!(minimized.tables["orders"].rows.len(), 2);
        assert_eq!(minimized.tables["users"].rows.len(), 2);
    }
}
