use crate::db::error::FailureClass;
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
    pub strategy: String,
    pub failure_class: FailureClass,
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
        let script = format!(
            r#"#!/usr/bin/env bash
set -e

echo "=== FAULTLINE COUNTEREXAMPLE REPRODUCER ==="
echo "Counterexample ID: {}"
echo "Expected Failure:  {}"
echo ""

DB_NAME="faultline_reproduce_{}"
export DATABASE_URL="${{DATABASE_URL:-postgres://postgres@localhost:5432}}/${{DB_NAME}}"

echo "Creating reproduction database: ${{DB_NAME}}"
psql -d postgres -c "DROP DATABASE IF EXISTS ${{DB_NAME}} WITH (FORCE);" > /dev/null 2>&1 || true
psql -d postgres -c "CREATE DATABASE ${{DB_NAME}};"

echo "Applying baseline schema..."
psql -d "${{DATABASE_URL}}" -f schema.sql

echo "Inserting minimal reproducing state..."
psql -d "${{DATABASE_URL}}" -f seed.sql

echo "Executing migration..."
if [ -f migration_up.sql ]; then
    psql -d "${{DATABASE_URL}}" -f migration_up.sql
else
    echo "No migration_up.sql file found, please run configured migration command"
fi

echo "Reproduction finished."
"#,
            manifest.id,
            manifest.failure_class.display_name(),
            manifest.id
        );
        fs::write(bundle_dir.join("reproduce.sh"), script)?;

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
