use crate::db::client::PostgresEnvironment;
use crate::db::error::{FailureClass, FailureSignature};
use crate::error::Result;
use crate::generator::DatabaseState;
use crate::schema::DatabaseSchema;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CounterexampleManifest {
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
    pub environment: Option<PostgresEnvironment>,
    pub failure_class: FailureClass,
    #[serde(default)]
    pub failure_signature: Option<FailureSignature>,
    pub error_message: String,
    pub rows_count: usize,
    pub state_fingerprint: String,
}

pub struct CounterexampleArtifact;

impl CounterexampleArtifact {
    pub fn export_bundle(
        dest_dir: &Path,
        manifest: &CounterexampleManifest,
        schema: &DatabaseSchema,
        minimal_state: &DatabaseState,
        schema_ddl: &str,
        migration_up_sql: Option<&str>,
    ) -> Result<PathBuf> {
        if migration_up_sql.is_none() {
            return Err(crate::error::FaultlineError::Config(
                "Cannot export a standalone bundle without migration_up.sql".to_string(),
            ));
        }

        let bundle_dir = dest_dir.join(&manifest.id);
        fs::create_dir_all(&bundle_dir)?;

        // 1. manifest.json
        let manifest_json = serde_json::to_string_pretty(manifest)?;
        fs::write(bundle_dir.join("manifest.json"), manifest_json)?;

        // 2. schema.sql
        fs::write(bundle_dir.join("schema.sql"), schema_ddl)?;

        // 3. seed.sql
        let seed_sql = minimal_state.to_insert_sql(schema)?;
        fs::write(bundle_dir.join("seed.sql"), &seed_sql)?;

        // 4. migration_up.sql
        if let Some(up_sql) = migration_up_sql {
            fs::write(bundle_dir.join("migration_up.sql"), up_sql)?;
        }

        // 5. reproduce.sh
        let expected_sqlstate = manifest
            .failure_signature
            .as_ref()
            .and_then(|signature| signature.sqlstate.as_deref())
            .unwrap_or("");
        let expected_message = manifest
            .failure_signature
            .as_ref()
            .map(|signature| signature.normalized_message.as_str())
            .unwrap_or("");
        let expected_sqlstate = shell_escape_single_quoted(expected_sqlstate);
        let expected_message = shell_escape_single_quoted(expected_message);
        let script = format!(
            r#"#!/usr/bin/env bash
set -e

echo "=== FAULTLINE COUNTEREXAMPLE REPRODUCER ==="
echo "Counterexample ID: {}"
echo "Expected Failure:  {}"
echo ""

DB_NAME="faultline_reproduce_{}"
export DATABASE_URL="${{DATABASE_URL:-postgres://postgres@localhost:5432}}/${{DB_NAME}}"
EXPECTED_SQLSTATE='{}'
EXPECTED_MESSAGE='{}'

cleanup() {{
    if ! psql -d postgres -c "DROP DATABASE IF EXISTS ${{DB_NAME}} WITH (FORCE);" > /dev/null 2>&1; then
        echo "ERROR: failed to clean up reproduction database ${{DB_NAME}}" >&2
        exit 1
    fi
}}
trap cleanup EXIT

echo "Creating reproduction database: ${{DB_NAME}}"
psql -d postgres -c "DROP DATABASE IF EXISTS ${{DB_NAME}} WITH (FORCE);" > /dev/null 2>&1 || true
psql -d postgres -c "CREATE DATABASE ${{DB_NAME}};"

echo "Applying baseline schema..."
psql -d "${{DATABASE_URL}}" -f schema.sql

echo "Inserting minimal reproducing state..."
psql -d "${{DATABASE_URL}}" -f seed.sql

echo "Executing migration..."
set +e
psql -v ON_ERROR_STOP=1 -d "${{DATABASE_URL}}" -f migration_up.sql 2>migration.stderr
MIGRATION_STATUS=$?
set -e

if [ "${{MIGRATION_STATUS}}" -eq 0 ]; then
    echo "ERROR: migration succeeded; expected {}" >&2
    exit 1
fi

if [ -n "${{EXPECTED_SQLSTATE}}" ] && ! grep -Fq "${{EXPECTED_SQLSTATE}}" migration.stderr; then
    echo "ERROR: migration failed with an unexpected SQLSTATE" >&2
    cat migration.stderr >&2
    exit 1
fi

NORMALIZED_ERROR=$(tr '\\n\\t' '  ' < migration.stderr | tr -s ' ' | tr '[:upper:]' '[:lower:]')
if [ -n "${{EXPECTED_MESSAGE}}" ] && [[ "${{NORMALIZED_ERROR}}" != *"${{EXPECTED_MESSAGE}}"* ]]; then
    echo "ERROR: migration failed with an unexpected error message" >&2
    cat migration.stderr >&2
    exit 1
fi

echo "Reproduction verified: expected failure reproduced."
"#,
            manifest.id,
            manifest.failure_class.display_name(),
            manifest.id,
            expected_sqlstate,
            expected_message,
            manifest.failure_class.display_name()
        );
        fs::write(bundle_dir.join("reproduce.sh"), script)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let script_path = bundle_dir.join("reproduce.sh");
            let mut permissions = fs::metadata(&script_path)?.permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(script_path, permissions)?;
        }

        // 6. README.md
        let readme = format!(
            r#"# Faultline Counterexample: {}

- **Failure Class:** {}
- **Discovery Strategy:** {}
- **Seed:** {}
- **Minimal Rows:** {}
- **Error:**
```text
{}
```

## Reproduction
To reproduce this failure:
```bash
bash reproduce.sh
```
"#,
            manifest.id,
            manifest.failure_class.display_name(),
            manifest.strategy,
            manifest.seed,
            manifest.rows_count,
            manifest.error_message
        );
        fs::write(bundle_dir.join("README.md"), readme)?;

        Ok(bundle_dir)
    }
}

fn shell_escape_single_quoted(value: &str) -> String {
    value.replace('\'', "'\"'\"'")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::error::{FailureClass, FailureSignature};
    use crate::generator::{DatabaseState, RowData, SqlValue, TableData};
    use crate::schema::{Column, DataType, DatabaseSchema, PrimaryKey, Table};
    use std::collections::HashMap;

    #[test]
    fn shell_escape_does_not_break_single_quoted_values() {
        assert_eq!(shell_escape_single_quoted("don't"), "don'\"'\"'t");
    }

    #[test]
    fn export_script_checks_expected_failure() {
        let temp_dir = tempfile::tempdir().unwrap();
        let schema = DatabaseSchema {
            tables: vec![Table {
                name: "users".to_string(),
                schema_name: "public".to_string(),
                columns: vec![Column {
                    name: "id".to_string(),
                    data_type: DataType::Integer,
                    is_nullable: false,
                    default_value: None,
                    is_identity: false,
                    is_generated: false,
                }],
                primary_key: Some(PrimaryKey {
                    name: "users_pkey".to_string(),
                    columns: vec!["id".to_string()],
                }),
                foreign_keys: vec![],
                unique_constraints: vec![],
                check_constraints: vec![],
                indexes: vec![],
            }],
            enums: vec![],
        };
        let state = DatabaseState {
            tables: HashMap::from([(
                "users".to_string(),
                TableData {
                    table_name: "users".to_string(),
                    rows: vec![RowData {
                        values: HashMap::from([("id".to_string(), SqlValue::Integer(1))]),
                    }],
                },
            )]),
        };
        let manifest = CounterexampleManifest {
            id: "cx_test".to_string(),
            session_id: "session".to_string(),
            timestamp: chrono::Utc::now(),
            seed: 1,
            experiment_seed: 1,
            strategy: "collision".to_string(),
            schema_fingerprint: schema.fingerprint(),
            migration_fingerprint: "migration".to_string(),
            faultline_version: env!("CARGO_PKG_VERSION").to_string(),
            environment: None,
            failure_class: FailureClass::UniqueViolation,
            failure_signature: Some(FailureSignature::new(
                FailureClass::UniqueViolation,
                Some("23505".to_string()),
                "duplicate key value",
            )),
            error_message: "duplicate key value".to_string(),
            rows_count: 1,
            state_fingerprint: state.fingerprint(),
        };

        let bundle = CounterexampleArtifact::export_bundle(
            temp_dir.path(),
            &manifest,
            &schema,
            &state,
            &schema.generate_create_ddl(),
            Some("SELECT 1;"),
        )
        .unwrap();
        let script = fs::read_to_string(bundle.join("reproduce.sh")).unwrap();
        assert!(script.contains("EXPECTED_SQLSTATE='23505'"));
        assert!(script.contains("migration succeeded; expected UNIQUE VIOLATION"));
    }
}
