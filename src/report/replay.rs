use crate::db::client::PgClient;
use crate::db::error::{FailureClass, FailureSignature};
use crate::db::isolation::IsolatedDatabase;
use crate::error::{FaultlineError, Result};
use crate::migration::command::CommandMigrationRunner;
use crate::migration::sql::SqlMigrationRunner;
use crate::migration::MigrationRunner;
use crate::schema::introspection::SchemaInspector;
use crate::semantic::state::CapturedState;
use crate::semantic::{RoundTripTester, SemanticChecker};
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
        eprintln!("replay: begin");
        let manifest_path = counterexample_dir.join("manifest.json");
        let schema_path = counterexample_dir.join("schema.sql");
        let seed_path = counterexample_dir.join("seed.sql");
        let migration_path = counterexample_dir.join("migration_up.sql");
        let migration_command_path = counterexample_dir.join("migration_up.command");
        let migration_down_path = counterexample_dir.join("migration_down.sql");
        let migration_down_command_path = counterexample_dir.join("migration_down.command");

        if !manifest_path.exists() {
            return Err(FaultlineError::CounterexampleNotFound(
                counterexample_dir.to_path_buf(),
            ));
        }

        if repeat_count == 0 {
            return Err(FaultlineError::Config(
                "Replay repeat count must be greater than zero".to_string(),
            ));
        }

        let manifest_content = fs::read_to_string(&manifest_path)?;
        let manifest: CounterexampleManifest = serde_json::from_str(&manifest_content)?;
        eprintln!("replay: manifest loaded");

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

        let migration_command = if migration_command_path.exists() {
            Some(fs::read_to_string(&migration_command_path)?)
        } else {
            None
        };

        if !migration_path.exists() && migration_command.is_none() {
            return Err(FaultlineError::Config(
                "Cannot replay: migration_up.sql or migration_up.command not found in counterexample bundle"
                    .to_string(),
            ));
        }

        let runner: Box<dyn MigrationRunner> = if migration_path.exists() {
            Box::new(SqlMigrationRunner::new(
                Some(migration_path),
                migration_down_path.exists().then_some(migration_down_path),
            ))
        } else {
            let down_command = migration_down_command_path
                .exists()
                .then(|| fs::read_to_string(migration_down_command_path))
                .transpose()?;
            Box::new(CommandMigrationRunner::new(
                migration_command,
                down_command,
                30,
            ))
        };

        let mut successes = 0;
        let mut errors = Vec::new();

        for _ in 0..repeat_count {
            eprintln!("replay: creating isolated db");
            let isolated = IsolatedDatabase::create(base_db_url, allow_non_isolated).await?;
            eprintln!("replay: isolated db created");
            let attempt = async {
                let client = PgClient::connect(&isolated.db_url).await?;
                eprintln!("replay: connected");

                if let Some(expected_environment) = &manifest.environment {
                    let actual_environment = client.get_environment().await?;
                    if &actual_environment != expected_environment {
                        return Err(FaultlineError::Config(
                            "Replay environment differs from the recorded PostgreSQL environment"
                                .to_string(),
                        ));
                    }
                }

                if !schema_sql.is_empty() {
                    client.batch_execute(&schema_sql).await.map_err(|e| {
                        FaultlineError::Schema(format!("Replay schema setup failed: {}", e))
                    })?;
                }
                if !seed_sql.is_empty() {
                    client.batch_execute(&seed_sql).await.map_err(|e| {
                        FaultlineError::CandidateData(format!(
                            "Replay candidate data could not be loaded: {}",
                            e
                        ))
                    })?;
                }

                let schema = SchemaInspector::introspect(&client, None).await?;
                eprintln!("replay: schema introspected");
                let before = CapturedState::capture(&client, &schema, &manifest.invariants).await?;
                eprintln!("replay: state captured");

                let result = runner.run_up(&client, &isolated.db_url).await?;
                eprintln!("replay: up run");
                let observed = if !result.success {
                    Some(result.failure_signature.unwrap_or_else(|| {
                        FailureSignature::new(
                            result.failure_class.unwrap_or(FailureClass::Unknown),
                            result.sqlstate,
                            result
                                .error_message
                                .as_deref()
                                .unwrap_or("migration failed"),
                        )
                    }))
                } else if manifest.failure_class == FailureClass::SemanticLoss {
                    SemanticChecker::check_semantic_loss(
                        &client,
                        &schema,
                        &before,
                        &manifest.invariants,
                    )
                    .await?
                    .map(|_| {
                        FailureSignature::new(FailureClass::SemanticLoss, None, "semantic loss")
                    })
                } else if manifest.failure_class == FailureClass::IrreversibleMigration {
                    eprintln!("replay: roundtrip check begin");
                    RoundTripTester::test_roundtrip(
                        &client,
                        &isolated.db_url,
                        runner.as_ref(),
                        &schema,
                        &before,
                    )
                    .await?
                    .map(|_| {
                        FailureSignature::new(
                            FailureClass::IrreversibleMigration,
                            None,
                            "irreversible migration",
                        )
                    })
                } else {
                    None
                };

                Ok::<Option<FailureSignature>, FaultlineError>(observed)
            }
            .await;
            eprintln!("replay: attempt done");

            let cleanup = isolated.destroy().await;
            eprintln!("replay: cleanup done");
            let observed = match (attempt, cleanup) {
                (Ok(observed), Ok(())) => observed,
                (Err(attempt_error), Ok(())) => return Err(attempt_error),
                (Ok(_), Err(cleanup_error)) => {
                    return Err(FaultlineError::Session(format!(
                        "Replay cleanup failed: {}",
                        cleanup_error
                    )))
                }
                (Err(attempt_error), Err(cleanup_error)) => {
                    return Err(FaultlineError::Session(format!(
                        "Replay failed: {}; cleanup also failed: {}",
                        attempt_error, cleanup_error
                    )))
                }
            };

            if let Some(actual) = observed {
                let matches = manifest
                    .failure_signature
                    .as_ref()
                    .map(|expected| actual.matches(expected))
                    .unwrap_or(actual.failure_class == manifest.failure_class);
                if matches {
                    successes += 1;
                } else if errors.is_empty() {
                    errors.push(format!(
                        "failure signature mismatch: expected {:?}, observed {:?}",
                        manifest.failure_signature, actual
                    ));
                }
            } else if errors.is_empty() {
                errors.push(format!(
                    "migration succeeded; expected {}",
                    manifest.failure_class.display_name()
                ));
            }
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
