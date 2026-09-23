use chrono::Utc;
use faultline::config::FaultlineConfig;
use faultline::db::client::PgClient;
use faultline::db::error::{FailureClass, FailureSignature};
use faultline::db::isolation::IsolatedDatabase;
use faultline::generator::{DatabaseState, RowData, SqlValue, TableData};
use faultline::migration::sql::SqlMigrationRunner;
use faultline::migration::MigrationRunner;
use faultline::report::CounterexampleReplayer;
use faultline::schema::introspection::SchemaInspector;
use faultline::search::{SearchBudget, SearchEngine};
use faultline::semantic::{RoundTripTester, SemanticChecker};
use faultline::storage::{
    CounterexampleArtifact, CounterexampleManifest, MigrationSources, StorageManager,
};
use faultline::strategy::StrategyScheduler;
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

const TEST_DB_URL: &str = "postgres://postgres:password@127.0.0.1:54329/faultline_fixture_a";
const ROUNDTRIP_DB_URL: &str = "postgres://postgres:password@127.0.0.1:54329/faultline_fixture_d";

#[tokio::test]
async fn test_end_to_end_case_collision_scenario() {
    let client = PgClient::connect(TEST_DB_URL)
        .await
        .expect("PostgreSQL fixture on port 54329 is required");

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

    let engine = SearchEngine::with_migration_sources(
        &config,
        &schema,
        &runner,
        &scheduler,
        &storage,
        TEST_DB_URL.to_string(),
        MigrationSources {
            up_sql: Some(up_sql),
            ..MigrationSources::default()
        },
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

    let replay = CounterexampleReplayer::replay(
        &storage.counterexamples_dir().join(best.id),
        TEST_DB_URL,
        2,
        false,
    )
    .await
    .expect("Generated replay bundle must reproduce its failure");
    assert_eq!(replay.successful_reproductions, 2);
    assert!(replay.is_deterministic);
}

#[tokio::test]
async fn generated_schema_keeps_independent_unique_and_expression_indexes() {
    let source = PgClient::connect(ROUNDTRIP_DB_URL)
        .await
        .expect("PostgreSQL roundtrip fixture on port 54329 is required");
    let schema = SchemaInspector::introspect(&source, None)
        .await
        .expect("roundtrip fixture schema must be introspectable");
    let notes = schema.get_table("notes").expect("fixture has notes table");
    assert!(notes
        .indexes
        .iter()
        .any(|index| index.name == "notes_body_exact_unique" && index.is_unique));
    assert!(notes.indexes.iter().any(|index| {
        index.name == "notes_body_lower_lookup"
            && index
                .definition
                .as_deref()
                .unwrap_or_default()
                .contains("lower(body)")
    }));

    let isolated = IsolatedDatabase::create(ROUNDTRIP_DB_URL, false)
        .await
        .expect("an isolated clone database must be available");
    let clone = PgClient::connect(&isolated.db_url)
        .await
        .expect("isolated database must be reachable");
    clone
        .batch_execute(&schema.generate_create_ddl())
        .await
        .expect("introspected schema DDL must execute");
    let cloned_schema = SchemaInspector::introspect(&clone, None)
        .await
        .expect("cloned schema must be introspectable");
    let cloned_notes = cloned_schema
        .get_table("notes")
        .expect("clone has notes table");
    assert!(cloned_notes
        .indexes
        .iter()
        .any(|index| index.name == "notes_body_exact_unique" && index.is_unique));
    assert!(cloned_notes.indexes.iter().any(|index| {
        index.name == "notes_body_lower_lookup"
            && index
                .definition
                .as_deref()
                .unwrap_or_default()
                .contains("lower(body)")
    }));
    isolated
        .destroy()
        .await
        .expect("clone database must be cleaned up");
}

#[tokio::test]
async fn roundtrip_check_runs_up_once_and_restores_data() {
    let (isolated, client, schema) = setup_notes_test_database().await;
    let before =
        faultline::semantic::state::CapturedState::capture(&client, &schema, &Default::default())
            .await
            .expect("baseline state must be captured");
    let migrations = tempfile::tempdir().unwrap();
    let up_path = migrations.path().join("roundtrip_up.sql");
    let down_path = migrations.path().join("roundtrip_down.sql");
    std::fs::write(
        &up_path,
        "ALTER TABLE notes ADD COLUMN migration_marker BOOLEAN NOT NULL DEFAULT TRUE;",
    )
    .unwrap();
    std::fs::write(
        &down_path,
        "ALTER TABLE notes DROP COLUMN migration_marker;",
    )
    .unwrap();
    let reversible = SqlMigrationRunner::new(Some(up_path), Some(down_path));
    assert!(
        reversible
            .run_up(&client, &isolated.db_url)
            .await
            .unwrap()
            .success
    );
    assert!(
        RoundTripTester::test_roundtrip(&client, &isolated.db_url, &reversible, &schema, &before,)
            .await
            .unwrap()
            .is_none(),
        "roundtrip checking must not run UP a second time"
    );
    isolated
        .destroy()
        .await
        .expect("roundtrip database must be cleaned up");
}

#[tokio::test]
async fn text_to_null_semantic_loss_is_exported_and_replayed() {
    let (isolated, client, schema) = setup_notes_test_database().await;
    let loss_sql = "UPDATE notes SET body = NULL;";
    let migrations = tempfile::tempdir().unwrap();
    let loss_path = migrations.path().join("semantic_loss.sql");
    std::fs::write(&loss_path, loss_sql).unwrap();
    let loss_runner = SqlMigrationRunner::new(Some(loss_path), None);
    let semantic_before =
        faultline::semantic::state::CapturedState::capture(&client, &schema, &Default::default())
            .await
            .unwrap();
    assert!(
        loss_runner
            .run_up(&client, &isolated.db_url)
            .await
            .unwrap()
            .success
    );
    assert!(
        SemanticChecker::check_semantic_loss(
            &client,
            &schema,
            &semantic_before,
            &Default::default(),
        )
        .await
        .unwrap()
        .is_some(),
        "setting a preserved text value to NULL must be detected"
    );

    let state = sample_notes_state();
    let bundle_root = tempfile::tempdir().unwrap();
    let schema_ddl = schema.generate_create_ddl();
    let manifest =
        counterexample_manifest("semantic_text_to_null", FailureClass::SemanticLoss, &state);
    let bundle = CounterexampleArtifact::export_bundle_with_sources(
        bundle_root.path(),
        &manifest,
        &schema,
        &state,
        &schema_ddl,
        &MigrationSources {
            up_sql: Some(loss_sql.to_string()),
            ..MigrationSources::default()
        },
    )
    .unwrap();
    let replay = CounterexampleReplayer::replay(&bundle, ROUNDTRIP_DB_URL, 1, false)
        .await
        .expect("exported semantic-loss bundle must replay");
    assert_eq!(replay.successful_reproductions, 1);
    #[cfg(unix)]
    assert_reproduction_script_succeeds(&bundle);
    isolated
        .destroy()
        .await
        .expect("semantic-loss database must be cleaned up");
}

#[tokio::test]
async fn irreversible_migration_bundle_is_exported_and_replayed() {
    let (isolated, _client, schema) = setup_notes_test_database().await;
    let state = sample_notes_state();
    let bundle_root = tempfile::tempdir().unwrap();
    let schema_ddl = schema.generate_create_ddl();
    let manifest = counterexample_manifest(
        "roundtrip_text_loss",
        FailureClass::IrreversibleMigration,
        &state,
    );
    let bundle = CounterexampleArtifact::export_bundle_with_sources(
        bundle_root.path(),
        &manifest,
        &schema,
        &state,
        &schema_ddl,
        &MigrationSources {
            up_sql: Some("UPDATE notes SET body = NULL;".to_string()),
            down_sql: Some("SELECT 1;".to_string()),
            ..MigrationSources::default()
        },
    )
    .unwrap();
    let replay = CounterexampleReplayer::replay(&bundle, ROUNDTRIP_DB_URL, 1, false)
        .await
        .expect("exported irreversible-migration bundle must replay");
    assert_eq!(replay.successful_reproductions, 1);
    #[cfg(unix)]
    assert_reproduction_script_succeeds(&bundle);
    isolated
        .destroy()
        .await
        .expect("irreversible database must be cleaned up");
}

async fn setup_notes_test_database() -> (
    IsolatedDatabase,
    PgClient,
    faultline::schema::DatabaseSchema,
) {
    let schema = {
        let source = PgClient::connect(ROUNDTRIP_DB_URL)
            .await
            .expect("PostgreSQL roundtrip fixture on port 54329 is required");
        SchemaInspector::introspect(&source, None)
            .await
            .expect("roundtrip fixture schema must be introspectable")
    };
    let isolated = IsolatedDatabase::create(ROUNDTRIP_DB_URL, false)
        .await
        .expect("an isolated roundtrip database must be available");
    let client = PgClient::connect(&isolated.db_url)
        .await
        .expect("isolated database must be reachable");
    client
        .batch_execute(&schema.generate_create_ddl())
        .await
        .expect("roundtrip schema DDL must execute");
    client
        .batch_execute("INSERT INTO notes (id, body) VALUES (1, 'keep this text');")
        .await
        .expect("fixture must contain a text value to preserve");
    (isolated, client, schema)
}

#[cfg(unix)]
fn assert_reproduction_script_succeeds(bundle: &Path) {
    let output = std::process::Command::new("bash")
        .arg(bundle.join("reproduce.sh"))
        .env("DATABASE_URL", ROUNDTRIP_DB_URL)
        .output()
        .expect("bash and PostgreSQL client must be available in CI");
    assert!(
        output.status.success(),
        "exported reproduction script failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn sample_notes_state() -> DatabaseState {
    let mut state = DatabaseState::new();
    state.tables.insert(
        "notes".to_string(),
        TableData {
            table_name: "notes".to_string(),
            rows: vec![RowData {
                values: HashMap::from([
                    ("id".to_string(), SqlValue::Integer(1)),
                    (
                        "body".to_string(),
                        SqlValue::Text("keep this text".to_string()),
                    ),
                ]),
            }],
        },
    );
    state
}

fn counterexample_manifest(
    id: &str,
    failure_class: FailureClass,
    state: &DatabaseState,
) -> CounterexampleManifest {
    CounterexampleManifest {
        id: id.to_string(),
        session_id: "fixture-session".to_string(),
        timestamp: Utc::now(),
        seed: 1,
        experiment_seed: 1,
        strategy: "fixture".to_string(),
        schema_fingerprint: String::new(),
        migration_fingerprint: String::new(),
        faultline_version: env!("CARGO_PKG_VERSION").to_string(),
        environment: None,
        failure_signature: Some(FailureSignature::new(
            failure_class,
            None,
            failure_class.display_name(),
        )),
        invariants: Default::default(),
        error_message: "fixture data changed".to_string(),
        rows_count: state.total_rows(),
        state_fingerprint: state.fingerprint(),
        failure_class,
    }
}
