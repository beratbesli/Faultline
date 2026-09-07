use serde::{Deserialize, Serialize};
pub mod experiment;
pub mod session;

pub use experiment::ExperimentRecord;
pub use session::TestSession;

use crate::config::FaultlineConfig;
use crate::db::client::PgClient;
use crate::db::error::FailureClass;
use crate::db::isolation::IsolatedDatabase;
use crate::error::Result;
use crate::generator::seed::GenerationSeed;
use crate::generator::DatabaseState;
use crate::migration::MigrationRunner;
use crate::minimizer::Minimizer;
use crate::schema::DatabaseSchema;
use crate::semantic::{RoundTripTester, SemanticChecker};
use crate::storage::{CounterexampleArtifact, CounterexampleManifest, StorageManager};
use crate::strategy::StrategyScheduler;
use chrono::Utc;
use std::collections::HashSet;
use std::time::{Duration, Instant};
use uuid::Uuid;

pub struct SearchBudget {
    pub experiments: usize,
    pub seed: u64,
    pub time_limit: Option<Duration>,
    pub max_rows_per_table: usize,
    pub no_minimize: bool,
    pub allow_non_isolated: bool,
    pub roundtrip: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchSummary {
    pub session_id: String,
    pub experiments_executed: usize,
    pub unique_states_tested: usize,
    pub counterexamples_found: usize,
    pub best_counterexample: Option<CounterexampleManifest>,
    pub minimal_state: Option<DatabaseState>,
    pub duration_ms: u64,
}

pub struct SearchEngine<'a> {
    pub config: &'a FaultlineConfig,
    pub schema: &'a DatabaseSchema,
    pub runner: &'a (dyn MigrationRunner + 'a),
    pub scheduler: &'a StrategyScheduler,
    pub storage: &'a StorageManager,
    pub base_db_url: String,
    pub migration_up_sql: Option<String>,
}

impl<'a> SearchEngine<'a> {
    pub fn new(
        config: &'a FaultlineConfig,
        schema: &'a DatabaseSchema,
        runner: &'a (dyn MigrationRunner + 'a),
        scheduler: &'a StrategyScheduler,
        storage: &'a StorageManager,
        base_db_url: String,
        migration_up_sql: Option<String>,
    ) -> Self {
        Self {
            config,
            schema,
            runner,
            scheduler,
            storage,
            base_db_url,
            migration_up_sql,
        }
    }

    pub async fn run_search(&self, budget: SearchBudget) -> Result<SearchSummary> {
        let session_id = Uuid::new_v4().simple().to_string();
        let mut session = TestSession::new(session_id.clone(), budget.seed);
        let start_time = Instant::now();

        let root_seed = GenerationSeed::new(budget.seed);
        let mut tested_fingerprints: HashSet<String> = HashSet::new();

        let mut best_manifest: Option<CounterexampleManifest> = None;
        let mut best_minimal_state: Option<DatabaseState> = None;

        let schema_ddl = self.schema.generate_create_ddl();

        for exp_idx in 0..budget.experiments {
            if let Some(limit) = budget.time_limit {
                if start_time.elapsed() >= limit {
                    tracing::info!("Search time limit reached ({}s)", limit.as_secs());
                    break;
                }
            }

            let mut sub_rng = root_seed.derive_subseed(exp_idx as u64);
            let strategy = self.scheduler.get_strategy(exp_idx);

            // Generate candidate state
            let candidate_state = strategy.generate_candidate(
                self.schema,
                budget.max_rows_per_table,
                &mut sub_rng,
            )?;

            let fingerprint = candidate_state.fingerprint();
            if !tested_fingerprints.insert(fingerprint.clone()) {
                continue;
            }

            session.unique_states_tested += 1;
            session.total_experiments += 1;

            let exp_id = format!("{}_{}", session_id, exp_idx);
            let exp_start = Instant::now();

            // Run migration trial on isolated database
            let trial_result = self
                .execute_trial(
                    &candidate_state,
                    &schema_ddl,
                    budget.allow_non_isolated,
                    budget.roundtrip,
                )
                .await;

            let (migration_res, failure_class) = match trial_result {
                Ok((res, fail)) => (res, fail),
                Err(e) => {
                    tracing::warn!("Trial error: {}", e);
                    continue;
                }
            };

            let counterexample_found = failure_class.is_some();

            let record = ExperimentRecord {
                id: exp_id.clone(),
                session_id: session_id.clone(),
                timestamp: Utc::now(),
                seed: budget.seed,
                strategy: strategy.name().to_string(),
                state_fingerprint: fingerprint.clone(),
                rows_tested: candidate_state.total_rows(),
                migration_result: migration_res.clone(),
                failure_class,
                counterexample_found,
                duration_ms: exp_start.elapsed().as_millis() as u64,
                state: candidate_state.clone(),
            };

            // Save experiment JSON
            let exp_file = self
                .storage
                .experiments_dir()
                .join(format!("{}.json", exp_id));
            let _ = self.storage.save_json(&exp_file, &record);

            if let Some(f_class) = failure_class {
                session.counterexamples_found += 1;
                tracing::info!(
                    "Counterexample discovered! Strategy: {}, Failure: {}",
                    strategy.name(),
                    f_class.display_name()
                );

                // Minimize counterexample
                let minimal_state = if budget.no_minimize {
                    candidate_state.clone()
                } else {
                    tracing::info!(
                        "Minimizing counterexample (from {} rows)...",
                        candidate_state.total_rows()
                    );
                    let runner = self.runner;
                    let schema = self.schema;
                    let schema_ddl_ref = &schema_ddl;
                    let base_url = &self.base_db_url;
                    let allow_non_iso = budget.allow_non_isolated;

                    Minimizer::minimize(
                        self.schema,
                        &candidate_state,
                        Box::new(move |candidate| {
                            let ddl = schema_ddl_ref.clone();
                            let b_url = base_url.clone();
                            let cand = candidate.clone();

                            Box::pin(async move {
                                if let Ok(isolated) =
                                    IsolatedDatabase::create(&b_url, allow_non_iso).await
                                {
                                    if let Ok(client) = PgClient::connect(&isolated.db_url).await {
                                        if client.batch_execute(&ddl).await.is_ok() {
                                            if let Ok(inserts) = cand.to_insert_sql(schema) {
                                                if client.batch_execute(&inserts).await.is_ok() {
                                                    if let Ok(m_res) = runner
                                                        .run_up(&client, &isolated.db_url)
                                                        .await
                                                    {
                                                        let reproduces = !m_res.success;
                                                        let _ = isolated.destroy().await;
                                                        return reproduces;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    let _ = isolated.destroy().await;
                                }
                                false
                            })
                        }),
                    )
                    .await
                };

                let counterexample_id = format!("cx_{}", Uuid::new_v4().simple());
                let manifest = CounterexampleManifest {
                    id: counterexample_id.clone(),
                    session_id: session_id.clone(),
                    timestamp: Utc::now(),
                    seed: budget.seed,
                    strategy: strategy.name().to_string(),
                    failure_class: f_class,
                    error_message: migration_res
                        .error_message
                        .unwrap_or_else(|| "Unknown failure".to_string()),
                    rows_count: minimal_state.total_rows(),
                    state_fingerprint: minimal_state.fingerprint(),
                };

                // Export counterexample bundle
                let _ = CounterexampleArtifact::export_bundle(
                    &self.storage.counterexamples_dir(),
                    &manifest,
                    self.schema,
                    &minimal_state,
                    &schema_ddl,
                    self.migration_up_sql.as_deref(),
                );

                session.best_counterexample_id = Some(counterexample_id);
                best_manifest = Some(manifest);
                best_minimal_state = Some(minimal_state);

                // By default stop on first verified counterexample
                break;
            }
        }

        session.is_completed = true;
        session.updated_at = Utc::now();

        // Save session
        let session_file = self
            .storage
            .sessions_dir()
            .join(format!("{}.json", session_id));
        let _ = self.storage.save_json(&session_file, &session);

        Ok(SearchSummary {
            session_id,
            experiments_executed: session.total_experiments,
            unique_states_tested: session.unique_states_tested,
            counterexamples_found: session.counterexamples_found,
            best_counterexample: best_manifest,
            minimal_state: best_minimal_state,
            duration_ms: start_time.elapsed().as_millis() as u64,
        })
    }

    async fn execute_trial(
        &self,
        candidate_state: &DatabaseState,
        schema_ddl: &str,
        allow_non_isolated: bool,
        roundtrip: bool,
    ) -> Result<(crate::migration::MigrationResult, Option<FailureClass>)> {
        let isolated = IsolatedDatabase::create(&self.base_db_url, allow_non_isolated).await?;
        let client = PgClient::connect(&isolated.db_url).await?;

        // Apply baseline schema
        client.batch_execute(schema_ddl).await?;

        // Insert candidate data
        let insert_sql = candidate_state.to_insert_sql(self.schema)?;
        if !insert_sql.trim().is_empty() {
            client.batch_execute(&insert_sql).await?;
        }

        // Run migration UP
        let migration_res = self.runner.run_up(&client, &isolated.db_url).await?;

        let mut failure_class = None;

        if !migration_res.success {
            failure_class = migration_res.failure_class.or(Some(FailureClass::Unknown));
        } else {
            // Check semantic loss if migration technically succeeded
            if self.config.checks.semantic_loss {
                let semantic_loss = SemanticChecker::check_semantic_loss(
                    &client,
                    self.schema,
                    candidate_state,
                    &self.config.invariants,
                )
                .await?;

                if let Some(msg) = semantic_loss {
                    failure_class = Some(FailureClass::SemanticLoss);
                    let _ = isolated.destroy().await;
                    let mut modified_res = migration_res;
                    modified_res.success = false;
                    modified_res.error_message = Some(msg);
                    return Ok((modified_res, failure_class));
                }
            }

            // Check roundtrip if enabled
            if roundtrip || self.config.checks.roundtrip {
                let roundtrip_loss = RoundTripTester::test_roundtrip(
                    &client,
                    &isolated.db_url,
                    self.runner,
                    self.schema,
                    candidate_state,
                )
                .await?;

                if let Some(msg) = roundtrip_loss {
                    failure_class = Some(FailureClass::IrreversibleMigration);
                    let _ = isolated.destroy().await;
                    let mut modified_res = migration_res;
                    modified_res.success = false;
                    modified_res.error_message = Some(msg);
                    return Ok((modified_res, failure_class));
                }
            }
        }

        let _ = isolated.destroy().await;
        Ok((migration_res, failure_class))
    }
}
