use crate::config::InvariantsConfig;
use crate::db::client::PgClient;
use crate::error::Result;
use crate::schema::DatabaseSchema;
use crate::semantic::state::CapturedState;

pub struct SemanticChecker;

impl SemanticChecker {
    pub async fn check_semantic_loss(
        client: &PgClient,
        schema: &DatabaseSchema,
        before: &CapturedState,
        invariants: &InvariantsConfig,
    ) -> Result<Option<String>> {
        let after = match CapturedState::capture(client, schema, invariants).await {
            Ok(after) => after,
            Err(error) => return Ok(Some(format!("Cannot read migrated data: {error}"))),
        };
        Ok(before.first_difference(&after, schema, Some(invariants)))
    }
}
