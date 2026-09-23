use crate::config::InvariantsConfig;
use crate::db::client::PgClient;
use crate::error::Result;
use crate::migration::MigrationRunner;
use crate::schema::DatabaseSchema;
use crate::semantic::state::CapturedState;

pub struct RoundTripTester;

impl RoundTripTester {
    /// UP has already run. Apply DOWN once and compare with the captured pre-UP state.
    pub async fn test_roundtrip(
        client: &PgClient,
        db_url: &str,
        runner: &dyn MigrationRunner,
        schema: &DatabaseSchema,
        before: &CapturedState,
    ) -> Result<Option<String>> {
        let down = match runner.run_down(client, db_url).await {
            Ok(down) => down,
            Err(error) => return Ok(Some(format!("Down migration could not run: {error}"))),
        };
        if !down.success {
            return Ok(Some(format!(
                "Down migration failed during round-trip: {:?}",
                down.error_message
            )));
        }
        let after = match CapturedState::capture(client, schema, &InvariantsConfig::default()).await
        {
            Ok(after) => after,
            Err(error) => return Ok(Some(format!("Cannot read rolled-back data: {error}"))),
        };
        Ok(before.first_difference(&after, schema, None))
    }
}
