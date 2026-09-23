use crate::config::InvariantsConfig;
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
    #[serde(default)]
    pub experiment_seed: u64,
    pub strategy: String,
    #[serde(default)]
    pub schema_fingerprint: String,
    #[serde(default)]
    pub migration_fingerprint: String,
    #[serde(default)]
    pub faultline_version: String,
    #[serde(default)]
    pub environment: Option<PostgresEnvironment>,
    pub failure_class: FailureClass,
    #[serde(default)]
    pub failure_signature: Option<FailureSignature>,
    #[serde(default)]
    pub invariants: InvariantsConfig,
    pub error_message: String,
    pub rows_count: usize,
    pub state_fingerprint: String,
}

pub struct CounterexampleArtifact;

#[derive(Debug, Clone, Default)]
pub struct MigrationSources {
    pub up_sql: Option<String>,
    pub up_command: Option<String>,
    pub down_sql: Option<String>,
    pub down_command: Option<String>,
}

impl CounterexampleArtifact {
    pub fn export_bundle(
        dest_dir: &Path,
        manifest: &CounterexampleManifest,
        schema: &DatabaseSchema,
        minimal_state: &DatabaseState,
        schema_ddl: &str,
        migration_up_sql: Option<&str>,
        migration_up_command: Option<&str>,
    ) -> Result<PathBuf> {
        let migrations = MigrationSources {
            up_sql: migration_up_sql.map(str::to_string),
            up_command: migration_up_command.map(str::to_string),
            ..MigrationSources::default()
        };
        Self::export_bundle_with_sources(
            dest_dir,
            manifest,
            schema,
            minimal_state,
            schema_ddl,
            &migrations,
        )
    }

    pub fn export_bundle_with_sources(
        dest_dir: &Path,
        manifest: &CounterexampleManifest,
        schema: &DatabaseSchema,
        minimal_state: &DatabaseState,
        schema_ddl: &str,
        migrations: &MigrationSources,
    ) -> Result<PathBuf> {
        if migrations.up_sql.is_none() && migrations.up_command.is_none() {
            return Err(crate::error::FaultlineError::Config(
                "Cannot export a standalone bundle without migration_up.sql or migration_up.command"
                    .to_string(),
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
        if let Some(up_sql) = &migrations.up_sql {
            fs::write(bundle_dir.join("migration_up.sql"), up_sql)?;
        }
        if let Some(up_command) = &migrations.up_command {
            fs::write(bundle_dir.join("migration_up.command"), up_command)?;
        }
        if let Some(down_sql) = &migrations.down_sql {
            fs::write(bundle_dir.join("migration_down.sql"), down_sql)?;
        }
        if let Some(down_command) = &migrations.down_command {
            fs::write(bundle_dir.join("migration_down.command"), down_command)?;
        }

        if matches!(
            manifest.failure_class,
            FailureClass::SemanticLoss | FailureClass::IrreversibleMigration
        ) {
            fs::write(
                bundle_dir.join("state_check.sql"),
                state_check_sql(schema, manifest),
            )?;
        }

        // 5. reproduce.sh
        let replay_mode = match manifest.failure_class {
            FailureClass::SemanticLoss => "semantic",
            FailureClass::IrreversibleMigration => "roundtrip",
            _ => "sql_error",
        };
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
cd -- "$(dirname -- "${{BASH_SOURCE[0]}}")"

echo "=== FAULTLINE COUNTEREXAMPLE REPRODUCER ==="
echo "Counterexample ID: {}"
echo "Expected Failure:  {}"
echo ""

DB_NAME="faultline_reproduce_{}"
DB_BASE_URL="${{DATABASE_URL:-postgres://postgres@localhost:5432/postgres}}"
if [[ "${{DB_BASE_URL}}" == *\?* ]]; then
    DB_URL_NO_QUERY="${{DB_BASE_URL%%\?*}}"
    DB_URL_QUERY="?${{DB_BASE_URL#*\?}}"
else
    DB_URL_NO_QUERY="${{DB_BASE_URL}}"
    DB_URL_QUERY=""
fi
DB_URL_SCHEME="${{DB_URL_NO_QUERY%%://*}}"
DB_URL_AUTHORITY="${{DB_URL_NO_QUERY#*://}}"
DB_URL_PREFIX="${{DB_URL_SCHEME}}://${{DB_URL_AUTHORITY%%/*}}"
MAINTENANCE_URL="${{DB_URL_PREFIX}}/postgres${{DB_URL_QUERY}}"
export DATABASE_URL="${{DB_URL_PREFIX}}/${{DB_NAME}}${{DB_URL_QUERY}}"
EXPECTED_SQLSTATE='{}'
EXPECTED_MESSAGE='{}'
REPLAY_MODE='{}'

cleanup() {{
    if ! psql -d "${{MAINTENANCE_URL}}" -c "DROP DATABASE IF EXISTS ${{DB_NAME}} WITH (FORCE);" > /dev/null 2>&1; then
        echo "ERROR: failed to clean up reproduction database ${{DB_NAME}}" >&2
        exit 1
    fi
}}
trap cleanup EXIT

echo "Creating reproduction database: ${{DB_NAME}}"
psql -d "${{MAINTENANCE_URL}}" -c "DROP DATABASE IF EXISTS ${{DB_NAME}} WITH (FORCE);" > /dev/null 2>&1 || true
psql -v ON_ERROR_STOP=1 -d "${{MAINTENANCE_URL}}" -c "CREATE DATABASE ${{DB_NAME}};"

echo "Applying baseline schema..."
psql -v ON_ERROR_STOP=1 -d "${{DATABASE_URL}}" -f schema.sql

echo "Inserting minimal reproducing state..."
psql -v ON_ERROR_STOP=1 -d "${{DATABASE_URL}}" -f seed.sql

if [ "${{REPLAY_MODE}}" != "sql_error" ]; then
    BASELINE_STATE=$(psql -v ON_ERROR_STOP=1 -At -d "${{DATABASE_URL}}" -f state_check.sql)
fi

echo "Executing migration..."
set +e
if [ -f migration_up.sql ]; then
    psql -v ON_ERROR_STOP=1 -d "${{DATABASE_URL}}" -f migration_up.sql 2>migration.stderr
elif [ -f migration_up.command ]; then
    bash -c "$(cat migration_up.command)" 2>migration.stderr
else
    echo "ERROR: no migration definition found" >&2
    exit 1
fi
MIGRATION_STATUS=$?
set -e

if [ "${{REPLAY_MODE}}" != "sql_error" ]; then
    if [ "${{MIGRATION_STATUS}}" -ne 0 ]; then
        echo "ERROR: UP migration failed before semantic verification" >&2
        cat migration.stderr >&2
        exit 1
    fi
    if [ "${{REPLAY_MODE}}" = "roundtrip" ]; then
        if [ -f migration_down.sql ]; then
            psql -v ON_ERROR_STOP=1 -d "${{DATABASE_URL}}" -f migration_down.sql 2>down.stderr || {{
                echo "Reproduction verified: DOWN migration failed."
                exit 0
            }}
        elif [ -f migration_down.command ]; then
            bash -c "$(cat migration_down.command)" 2>down.stderr || {{
                echo "Reproduction verified: DOWN migration failed."
                exit 0
            }}
        else
            echo "Reproduction verified: DOWN migration is unavailable."
            exit 0
        fi
    fi
    if AFTER_STATE=$(psql -v ON_ERROR_STOP=1 -At -d "${{DATABASE_URL}}" -f state_check.sql 2>state.stderr); then
        if [ "${{AFTER_STATE}}" = "${{BASELINE_STATE}}" ]; then
            echo "ERROR: preserved data matched the baseline; expected data loss" >&2
            exit 1
        fi
    elif grep -Eiq 'relation .* does not exist|column .* does not exist' state.stderr; then
        echo "Reproduction verified: a preserved table or column disappeared."
        exit 0
    else
        echo "ERROR: unable to inspect migrated data" >&2
        cat state.stderr >&2
        exit 1
    fi
    echo "Reproduction verified: preserved data changed."
    exit 0
fi

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
            replay_mode,
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
        let environment_details = manifest
            .environment
            .as_ref()
            .map(|environment| {
                let timezone = environment
                    .settings
                    .get("TimeZone")
                    .or_else(|| environment.settings.get("timezone"))
                    .map(String::as_str)
                    .unwrap_or("unknown");
                let lc_collate = environment
                    .settings
                    .get("lc_collate")
                    .map(String::as_str)
                    .unwrap_or("unknown");
                format!(
                    "- **PostgreSQL Version:** {}\n- **TimeZone:** {}\n- **Locale:** {}",
                    environment.server_version, timezone, lc_collate
                )
            })
            .unwrap_or_else(|| {
                "- **PostgreSQL Environment:** recorded in manifest.json".to_string()
            });
        let readme = format!(
            r#"# Faultline Counterexample: {}

- **Failure Class:** {}
- **Discovery Strategy:** {}
- **Root Seed:** {}
- **Experiment Seed:** {}
- **Faultline Version:** {}
- **Schema Fingerprint:** {}
- **Migration Fingerprint:** {}
- **Minimal Rows:** {}
{}
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
            manifest.experiment_seed,
            manifest.faultline_version,
            manifest.schema_fingerprint,
            manifest.migration_fingerprint,
            manifest.rows_count,
            environment_details,
            manifest.error_message
        );
        fs::write(bundle_dir.join("README.md"), readme)?;

        Ok(bundle_dir)
    }
}

fn shell_escape_single_quoted(value: &str) -> String {
    value.replace('\'', "'\"'\"'")
}

fn state_check_sql(schema: &DatabaseSchema, manifest: &CounterexampleManifest) -> String {
    let mut sql = String::new();
    for table in &schema.tables {
        let columns = table.columns.iter().filter(|column| {
            if manifest.failure_class != FailureClass::SemanticLoss {
                return true;
            }
            let name = format!("{}.{}", table.name, column.name);
            !manifest.invariants.ignore.contains(&name)
                && (manifest.invariants.preserve.is_empty()
                    || manifest.invariants.preserve.contains(&name))
        });
        let fields = columns
            .flat_map(|column| {
                [
                    sql_string_literal(&column.name),
                    format!("t.{}", sql_identifier(&column.name)),
                ]
            })
            .collect::<Vec<_>>()
            .join(", ");
        sql.push_str(&format!(
            "SELECT {} || COALESCE(jsonb_agg(row_value ORDER BY row_value::text), '[]'::jsonb)::text FROM (SELECT jsonb_build_object({fields}) AS row_value FROM {} t) state;\n",
            sql_string_literal(&format!("{}:", table.name)),
            sql_identifier(&table.name),
        ));
    }
    sql
}

fn sql_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn sql_string_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
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
            invariants: InvariantsConfig::default(),
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
            None,
        )
        .unwrap();
        let script = fs::read_to_string(bundle.join("reproduce.sh")).unwrap();
        assert!(script.contains("EXPECTED_SQLSTATE='23505'"));
        assert!(script.contains("migration succeeded; expected UNIQUE VIOLATION"));
        assert!(script.contains("DB_URL_PREFIX=\"${DB_URL_SCHEME}://${DB_URL_AUTHORITY%%/*}\""));
        assert!(script.contains("MAINTENANCE_URL=\"${DB_URL_PREFIX}/postgres${DB_URL_QUERY}\""));
    }
}
