use crate::config::InvariantsConfig;
use crate::db::client::PgClient;
use crate::error::Result;
use crate::schema::DatabaseSchema;
use std::collections::BTreeMap;

/// Canonical row snapshots produced by PostgreSQL itself. Keeping the JSONB
/// text avoids converting exact PostgreSQL numerics through an f64.
pub struct CapturedState {
    tables: BTreeMap<String, TableSnapshot>,
}

struct TableSnapshot {
    all_columns: String,
    preserved_columns: String,
}

impl CapturedState {
    pub async fn capture(
        client: &PgClient,
        schema: &DatabaseSchema,
        invariants: &InvariantsConfig,
    ) -> Result<Self> {
        let mut tables = BTreeMap::new();
        for table in &schema.tables {
            let name = quote_identifier(&table.name);
            let preserved: Vec<&str> = table
                .columns
                .iter()
                .filter(|column| {
                    let invariant_name = format!("{}.{}", table.name, column.name);
                    !invariants.ignore.contains(&invariant_name)
                        && (invariants.preserve.is_empty()
                            || invariants.preserve.contains(&invariant_name))
                })
                .map(|column| column.name.as_str())
                .collect();
            let excluded: Vec<&str> = table
                .columns
                .iter()
                .map(|column| column.name.as_str())
                .filter(|column| !preserved.contains(column))
                .collect();
            let excluded_array = sql_text_array(&excluded);
            let query = format!(
                "SELECT \
                    COALESCE(jsonb_agg(row_value ORDER BY row_value::text), '[]'::jsonb)::text, \
                    COALESCE(jsonb_agg(row_value - {excluded_array} ORDER BY (row_value - {excluded_array})::text), '[]'::jsonb)::text \
                 FROM (SELECT to_jsonb(t) AS row_value FROM {name} t) captured"
            );
            let rows = client.query(&query, &[]).await?;
            let row = rows
                .first()
                .expect("aggregate query returns one row even for an empty table");
            tables.insert(
                table.name.clone(),
                TableSnapshot {
                    all_columns: row.get(0),
                    preserved_columns: row.get(1),
                },
            );
        }
        Ok(Self { tables })
    }

    pub fn first_difference(
        &self,
        after: &Self,
        schema: &DatabaseSchema,
        invariants: Option<&InvariantsConfig>,
    ) -> Option<String> {
        for table in &schema.tables {
            let Some(before_rows) = self.tables.get(&table.name) else {
                return Some(format!(
                    "Table '{}' was not captured before migration",
                    table.name
                ));
            };
            let Some(after_rows) = after.tables.get(&table.name) else {
                return Some(format!("Table '{}' is missing after migration", table.name));
            };
            let (before, after) = if invariants.is_some() {
                (
                    &before_rows.preserved_columns,
                    &after_rows.preserved_columns,
                )
            } else {
                (&before_rows.all_columns, &after_rows.all_columns)
            };
            if before != after {
                return Some(format!(
                    "Table '{}' row count or preserved column values changed",
                    table.name
                ));
            }
        }
        None
    }
}

fn quote_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn sql_text_array(values: &[&str]) -> String {
    let values = values
        .iter()
        .map(|value| format!("'{}'", value.replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(", ");
    format!("ARRAY[{values}]::text[]")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_text_becoming_null_without_depending_on_row_order() {
        // The canonical values are captured as text from PostgreSQL JSONB so
        // comparison does not depend on Rust's floating point representation.
        let before = CapturedState {
            tables: BTreeMap::from([(
                "people".to_string(),
                TableSnapshot {
                    all_columns: r#"[{"email":"a@example.com"},{"email":"b@example.com"}]"#
                        .to_string(),
                    preserved_columns: r#"[{"email":"a@example.com"},{"email":"b@example.com"}]"#
                        .to_string(),
                },
            )]),
        };
        let reordered = CapturedState {
            tables: BTreeMap::from([(
                "people".to_string(),
                TableSnapshot {
                    all_columns: r#"[{"email":"a@example.com"},{"email":"b@example.com"}]"#
                        .to_string(),
                    preserved_columns: r#"[{"email":"a@example.com"},{"email":"b@example.com"}]"#
                        .to_string(),
                },
            )]),
        };
        let loss = CapturedState {
            tables: BTreeMap::from([(
                "people".to_string(),
                TableSnapshot {
                    all_columns: r#"[{"email":null},{"email":"b@example.com"}]"#.to_string(),
                    preserved_columns: r#"[{"email":null},{"email":"b@example.com"}]"#.to_string(),
                },
            )]),
        };
        let schema = DatabaseSchema {
            tables: vec![crate::schema::Table {
                name: "people".to_string(),
                schema_name: "public".to_string(),
                columns: vec![crate::schema::Column {
                    name: "email".to_string(),
                    data_type: crate::schema::DataType::Text,
                    is_nullable: true,
                    default_value: None,
                    is_identity: false,
                    is_generated: false,
                }],
                primary_key: None,
                foreign_keys: vec![],
                unique_constraints: vec![],
                check_constraints: vec![],
                indexes: vec![],
            }],
            enums: vec![],
        };

        assert!(before.first_difference(&reordered, &schema, None).is_none());
        assert!(before.first_difference(&loss, &schema, None).is_some());
    }

    #[test]
    fn preserves_exact_numeric_text_from_postgresql_snapshots() {
        let before = CapturedState {
            tables: BTreeMap::from([(
                "amounts".to_string(),
                TableSnapshot {
                    all_columns: r#"[{"amount":12345678901234567890.123456789}]"#.to_string(),
                    preserved_columns: r#"[{"amount":12345678901234567890.123456789}]"#.to_string(),
                },
            )]),
        };
        let after = CapturedState {
            tables: BTreeMap::from([(
                "amounts".to_string(),
                TableSnapshot {
                    all_columns: r#"[{"amount":12345678901234567890.123456788}]"#.to_string(),
                    preserved_columns: r#"[{"amount":12345678901234567890.123456788}]"#.to_string(),
                },
            )]),
        };
        assert_ne!(
            before.tables["amounts"].all_columns,
            after.tables["amounts"].all_columns
        );
    }
}
