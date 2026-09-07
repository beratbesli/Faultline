use crate::config::InvariantsConfig;
use crate::db::client::PgClient;
use crate::error::{FaultlineError, Result};
use crate::generator::DatabaseState;
use crate::schema::DatabaseSchema;

pub struct SemanticChecker;

impl SemanticChecker {
    pub async fn check_semantic_loss(
        client: &PgClient,
        schema: &DatabaseSchema,
        before_state: &DatabaseState,
        invariants: &InvariantsConfig,
    ) -> Result<Option<String>> {
        for table in &schema.tables {
            let before_table = match before_state.tables.get(&table.name) {
                Some(t) => t,
                None => continue,
            };

            // Query current row count in table
            let count_query = format!("SELECT COUNT(*) FROM \"{}\"", table.name);
            let rows = client.query(&count_query, &[]).await?;
            let after_count: i64 = rows.first().map(|r| r.get(0)).unwrap_or(0);

            if after_count as usize != before_table.rows.len() {
                return Ok(Some(format!(
                    "Table '{}' row count changed from {} to {}",
                    table.name,
                    before_table.rows.len(),
                    after_count
                )));
            }

            // Check preserved columns
            for col in &table.columns {
                let full_col_name = format!("{}.{}", table.name, col.name);
                if invariants.ignore.contains(&full_col_name) {
                    continue;
                }

                let must_preserve =
                    invariants.preserve.contains(&full_col_name) || invariants.preserve.is_empty();
                if must_preserve {
                    // Check if numeric column had fractional data truncated or rounded
                    let query_vals = format!("SELECT \"{}\" FROM \"{}\"", col.name, table.name);
                    if let Ok(after_rows) = client.query(&query_vals, &[]).await {
                        for (idx, row) in after_rows.iter().enumerate() {
                            if idx < before_table.rows.len() {
                                if let Some(before_val) =
                                    before_table.rows[idx].values.get(&col.name)
                                {
                                    let before_lit = before_val.to_sql_literal();
                                    // If before was '19.99' and now it's '19' or integer
                                    if before_lit.contains('.') {
                                        // Attempt reading as integer or string
                                        let after_str: Result<String> = row
                                            .try_get::<_, i32>(0)
                                            .map(|i| i.to_string())
                                            .or_else(|_| {
                                                row.try_get::<_, i64>(0).map(|i| i.to_string())
                                            })
                                            .or_else(|_| row.try_get::<_, String>(0))
                                            .map_err(|e| FaultlineError::DbQuery(e.to_string()));

                                        if let Ok(val_str) = after_str {
                                            if !val_str.contains('.') && val_str != "NULL" {
                                                return Ok(Some(format!(
                                                    "Column '{}' lost precision: before='{}', after='{}'",
                                                    full_col_name, before_lit, val_str
                                                )));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(None)
    }
}
