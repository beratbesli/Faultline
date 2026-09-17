use crate::db::client::PostgresEnvironment;
use crate::db::error::FailureClass;
use crate::db::error::FailureSignature;
use crate::generator::DatabaseState;
use crate::migration::MigrationResult;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentRecord {
    pub id: String,
    pub session_id: String,
    pub timestamp: DateTime<Utc>,
    pub seed: u64,
    pub experiment_seed: u64,
    pub strategy: String,
    pub schema_fingerprint: String,
    pub migration_fingerprint: String,
    pub faultline_version: String,
    #[serde(default)]
    pub environment: PostgresEnvironment,
    pub state_fingerprint: String,
    pub rows_tested: usize,
    pub migration_result: MigrationResult,
    pub failure_class: Option<FailureClass>,
    #[serde(default)]
    pub failure_signature: Option<FailureSignature>,
    pub counterexample_found: bool,
    pub duration_ms: u64,
    pub state: DatabaseState,
}
