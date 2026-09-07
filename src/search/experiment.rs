use crate::db::error::FailureClass;
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
    pub strategy: String,
    pub state_fingerprint: String,
    pub rows_tested: usize,
    pub migration_result: MigrationResult,
    pub failure_class: Option<FailureClass>,
    pub counterexample_found: bool,
    pub duration_ms: u64,
    pub state: DatabaseState,
}
