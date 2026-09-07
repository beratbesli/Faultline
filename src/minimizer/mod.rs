pub mod ddmin;

use crate::generator::DatabaseState;
use crate::schema::DatabaseSchema;
use std::future::Future;
use std::pin::Pin;

pub type TestFn<'a> = Box<
    dyn Fn(&DatabaseState) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> + Send + Sync + 'a,
>;

pub struct Minimizer;

impl Minimizer {
    pub async fn minimize<'a>(
        schema: &DatabaseSchema,
        initial_state: &DatabaseState,
        test_fn: TestFn<'a>,
    ) -> DatabaseState {
        // Step 1: Hierarchical / Dependency-aware row reduction
        let mut current_state = ddmin::reduce_rows(schema, initial_state, &test_fn).await;

        // Step 2: Value-level shrinking
        current_state = ddmin::shrink_values(schema, &current_state, &test_fn).await;

        current_state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generator::{RowData, SqlValue, TableData};
    use crate::schema::{Column, DataType, DatabaseSchema, PrimaryKey, Table};
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_minimizer_reduces_to_minimal_failing_subset() {
        let table = Table {
            name: "users".to_string(),
            schema_name: "public".to_string(),
            columns: vec![
                Column {
                    name: "id".to_string(),
                    data_type: DataType::Integer,
                    is_nullable: false,
                    default_value: None,
                    is_identity: true,
                    is_generated: false,
                },
                Column {
                    name: "email".to_string(),
                    data_type: DataType::Text,
                    is_nullable: false,
                    default_value: None,
                    is_identity: false,
                    is_generated: false,
                },
            ],
            primary_key: Some(PrimaryKey {
                name: "pk_users".to_string(),
                columns: vec!["id".to_string()],
            }),
            foreign_keys: vec![],
            unique_constraints: vec![],
            check_constraints: vec![],
            indexes: vec![],
        };

        let schema = DatabaseSchema {
            tables: vec![table],
            enums: vec![],
        };

        // Create 6 rows, where only rows 2 and 4 have "collision@example.com" and "COLLISION@EXAMPLE.COM"
        let mut rows = Vec::new();
        for i in 0..6 {
            let mut values = HashMap::new();
            values.insert("id".to_string(), SqlValue::Integer(i as i32 + 1));
            let email = match i {
                2 => "collision@example.com".to_string(),
                4 => "COLLISION@EXAMPLE.COM".to_string(),
                _ => format!("normal_{}@example.com", i),
            };
            values.insert("email".to_string(), SqlValue::Text(email));
            rows.push(RowData { values });
        }

        let initial_state = DatabaseState {
            tables: HashMap::from([(
                "users".to_string(),
                TableData {
                    table_name: "users".to_string(),
                    rows,
                },
            )]),
        };

        // Failure predicate: fails if state contains both lowercase and uppercase of "collision@example.com"
        let test_fn = Box::new(|state: &DatabaseState| {
            let mut has_lower = false;
            let mut has_upper = false;
            if let Some(t_data) = state.tables.get("users") {
                for r in &t_data.rows {
                    if let Some(SqlValue::Text(s)) = r.values.get("email") {
                        if s == "collision@example.com" {
                            has_lower = true;
                        }
                        if s == "COLLISION@EXAMPLE.COM" {
                            has_upper = true;
                        }
                    }
                }
            }
            let fail = has_lower && has_upper;
            Box::pin(async move { fail }) as Pin<Box<dyn Future<Output = bool> + Send>>
        });

        let minimized = Minimizer::minimize(&schema, &initial_state, test_fn).await;

        let final_rows = &minimized.tables["users"].rows;
        assert_eq!(
            final_rows.len(),
            2,
            "Minimizer must shrink 6 rows down to exactly the 2 essential rows"
        );
    }
}
