use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSession {
    pub session_id: String,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub seed: u64,
    pub total_experiments: usize,
    pub unique_states_tested: usize,
    pub counterexamples_found: usize,
    pub best_counterexample_id: Option<String>,
    pub is_completed: bool,
}

impl TestSession {
    pub fn new(session_id: String, seed: u64) -> Self {
        let now = Utc::now();
        Self {
            session_id,
            started_at: now,
            updated_at: now,
            seed,
            total_experiments: 0,
            unique_states_tested: 0,
            counterexamples_found: 0,
            best_counterexample_id: None,
            is_completed: false,
        }
    }
}
