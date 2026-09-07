use crate::db::client::PgClient;
use crate::error::Result;
use crate::generator::DatabaseState;
use crate::migration::MigrationRunner;
use crate::schema::DatabaseSchema;

pub struct RoundTripTester;

impl RoundTripTester {
    pub async fn test_roundtrip(
        client: &PgClient,
        db_url: &str,
        runner: &dyn MigrationRunner,
        schema: &DatabaseSchema,
        before_state: &DatabaseState,
    ) -> Result<Option<String>> {
        // Step 1: UP
        let up_res = runner.run_up(client, db_url).await?;
        if !up_res.success {
            return Ok(Some(format!(
                "Up migration failed during round-trip: {:?}",
                up_res.error_message
            )));
        }

        // Step 2: DOWN
        let down_res = runner.run_down(client, db_url).await?;
        if !down_res.success {
            return Ok(Some(format!(
                "Down migration failed during round-trip: {:?}",
                down_res.error_message
            )));
        }

        // Step 3: Compare data after DOWN with before_state
        for (table_name, table_data) in &before_state.tables {
            let table = match schema.get_table(table_name) {
                Some(t) => t,
                None => continue,
            };

            let count_query = format!("SELECT COUNT(*) FROM \"{}\"", table_name);
            let rows = match client.query(&count_query, &[]).await {
                Ok(r) => r,
                Err(e) => {
                    return Ok(Some(format!(
                        "Failed to query table '{}' after rollback: {}",
                        table_name, e
                    )))
                }
            };
            let count: i64 = rows.first().map(|r| r.get(0)).unwrap_or(0);
            if count as usize != table_data.rows.len() {
                return Ok(Some(format!(
                    "Table '{}' row count mismatch after rollback: expected {}, found {}",
                    table_name,
                    table_data.rows.len(),
                    count
                )));
            }

            // Compare column values
            for col in &table.columns {
                let query_vals = format!("SELECT \"{}\" FROM \"{}\"", col.name, table.name);
                if let Ok(after_rows) = client.query(&query_vals, &[]).await {
                    for (idx, row) in after_rows.iter().enumerate() {
                        if idx < table_data.rows.len() {
                            if let Some(before_val) = table_data.rows[idx].values.get(&col.name) {
                                let before_lit = before_val.to_sql_literal();
                                // Try reading after value
                                let after_str: Option<String> = row
                                    .try_get::<_, i32>(0)
                                    .map(|i| i.to_string())
                                    .or_else(|_| row.try_get::<_, i64>(0).map(|i| i.to_string()))
                                    .or_else(|_| {
                                        row.try_get::<_, f64>(0).map(|f| format!("{:.2}", f))
                                    })
                                    .or_else(|_| row.try_get::<_, String>(0))
                                    .ok();

                                if let Some(val_str) = after_str {
                                    let before_clean = before_lit.trim_matches('\'');
                                    let after_clean = val_str.trim_matches('\'');

                                    // If before had fractional (e.g. 19.99) and after doesn't match (e.g. 19.00 or 19)
                                    if before_clean != after_clean {
                                        return Ok(Some(format!(
                                            "Table '{}', column '{}' data lost after rollback: before='{}', after UP+DOWN='{}'",
                                            table.name, col.name, before_clean, after_clean
                                        )));
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
