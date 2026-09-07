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
        _schema: &DatabaseSchema,
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
        }

        Ok(None)
    }
}
