use crate::db::client::PgClient;
use crate::db::isolation::IsolatedDatabase;
use crate::error::{FaultlineError, Result};
use crate::storage::CounterexampleManifest;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayResult {
    pub counterexample_id: String,
    pub total_attempts: usize,
    pub successful_reproductions: usize,
    pub is_deterministic: bool,
    pub error_details: Vec<String>,
}

pub struct CounterexampleReplayer;

impl CounterexampleReplayer {
    pub async fn replay(
        counterexample_dir: &Path,
        base_db_url: &str,
        repeat_count: usize,
        allow_non_isolated: bool,
    ) -> Result<ReplayResult> {
        let manifest_path = counterexample_dir.join("manifest.json");
        let schema_path = counterexample_dir.join("schema.sql");
        let seed_path = counterexample_dir.join("seed.sql");
        let migration_path = counterexample_dir.join("migration_up.sql");

        if !manifest_path.exists() {
            return Err(FaultlineError::CounterexampleNotFound(
                counterexample_dir.to_path_buf(),
            ));
        }

        let manifest_content = fs::read_to_string(&manifest_path)?;
        let manifest: CounterexampleManifest = serde_json::from_str(&manifest_content)?;

        let schema_sql = if schema_path.exists() {
            fs::read_to_string(&schema_path)?
        } else {
            String::new()
        };

        let seed_sql = if seed_path.exists() {
            fs::read_to_string(&seed_path)?
        } else {
            String::new()
        };

        let migration_sql = if migration_path.exists() {
            fs::read_to_string(&migration_path)?
        } else {
            return Err(FaultlineError::Config(
                "Cannot replay: migration_up.sql not found in counterexample bundle".to_string(),
            ));
        };

        let mut successes = 0;
        let mut errors = Vec::new();

        for _ in 0..repeat_count {
            let isolated = IsolatedDatabase::create(base_db_url, allow_non_isolated).await?;
            let client = match PgClient::connect(&isolated.db_url).await {
                Ok(c) => c,
                Err(e) => {
                    let _ = isolated.destroy().await;
                    return Err(e);
                }
            };

            // Setup schema and seed
            if !schema_sql.is_empty() {
                client.batch_execute(&schema_sql).await?;
            }
            if !seed_sql.is_empty() {
                client.batch_execute(&seed_sql).await?;
            }

            // Run migration
            match client.batch_execute(&migration_sql).await {
                Ok(_) => {
                    // Migration succeeded (did not reproduce expected failure)
                }
                Err(e) => {
                    // Migration failed! Check if it matches
                    successes += 1;
                    if errors.is_empty() {
                        errors.push(e.to_string());
                    }
                }
            }

            let _ = isolated.destroy().await;
        }

        Ok(ReplayResult {
            counterexample_id: manifest.id,
            total_attempts: repeat_count,
            successful_reproductions: successes,
            is_deterministic: successes == repeat_count,
            error_details: errors,
        })
    }
}
