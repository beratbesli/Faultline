use faultline::config::FaultlineConfig;
use faultline::db::client::PgClient;
use faultline::migration::sql::SqlMigrationRunner;
use faultline::schema::introspection::SchemaInspector;
use faultline::search::{SearchBudget, SearchEngine};
use faultline::storage::StorageManager;
use faultline::strategy::StrategyScheduler;
use std::path::Path;
use std::time::Duration;

const TEST_DB_URL: &str = "postgres://postgres@127.0.0.1:54329/faultline_fixture_a";

#[tokio::test]
async fn test_end_to_end_case_collision_scenario() {
    // Check if test db is available
    let client = match PgClient::connect(TEST_DB_URL).await {
        Ok(c) => c,
        Err(_) => {
            eprintln!("Skipping integration test: PostgreSQL on 54329 not available");
            return;
        }
    };

    let config_path = Path::new("fixtures/fixture_a_case_collision/faultline.yaml");
    let config =
        FaultlineConfig::load_from_file(config_path).expect("Failed to load fixture A config");

    let schema = SchemaInspector::introspect(&client, None)
        .await
        .expect("Failed to introspect schema");
    assert!(!schema.tables.is_empty(), "Schema must have tables");

    let runner = SqlMigrationRunner::new(
        config.migration.up_sql.clone(),
        config.migration.down_sql.clone(),
    );

    let scheduler = StrategyScheduler::new(&["collision".to_string()], None).unwrap();
    let temp_storage_dir = tempfile::tempdir().unwrap();
    let storage = StorageManager::new(temp_storage_dir.path());
    storage.init().unwrap();

    let up_sql = std::fs::read_to_string(config.migration.up_sql.as_ref().unwrap()).unwrap();

    let engine = SearchEngine::new(
        &config,
        &schema,
        &runner,
        &scheduler,
        &storage,
        TEST_DB_URL.to_string(),
        Some(up_sql),
    );

    let budget = SearchBudget {
        experiments: 10,
        seed: 912831,
        time_limit: Some(Duration::from_secs(30)),
        max_rows_per_table: 20,
        no_minimize: false,
        allow_non_isolated: false,
        roundtrip: false,
    };

    let summary = engine
        .run_search(budget)
        .await
        .expect("Search engine failed");
    assert_eq!(
        summary.counterexamples_found, 1,
        "Must find exactly 1 counterexample"
    );

    let best = summary
        .best_counterexample
        .expect("Must have counterexample");
    assert_eq!(best.strategy, "collision");
    assert_eq!(best.rows_count, 2, "Minimal reproducing rows must be 2");

    let min_state = summary.minimal_state.expect("Must have minimal state");
    let users_rows = &min_state.tables["users"].rows;
    assert_eq!(users_rows.len(), 2);
}
