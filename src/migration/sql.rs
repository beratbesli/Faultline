use crate::db::client::PgClient;
use crate::error::{FaultlineError, Result};
use crate::migration::fingerprint::compute_string_fingerprint;
use crate::migration::{MigrationResult, MigrationRunner};
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

pub struct SqlMigrationRunner {
    pub up_path: Option<PathBuf>,
    pub down_path: Option<PathBuf>,
}

impl SqlMigrationRunner {
    pub fn new(up_path: Option<PathBuf>, down_path: Option<PathBuf>) -> Self {
        Self { up_path, down_path }
    }

    async fn execute_file(&self, client: &PgClient, path: &PathBuf) -> Result<MigrationResult> {
        if !path.exists() {
            return Err(FaultlineError::Config(format!(
                "Migration SQL file does not exist: {}",
                path.display()
            )));
        }

        let sql = fs::read_to_string(path)?;
        let start = Instant::now();

        match client.batch_execute(&sql).await {
            Ok(_) => {
                let duration = start.elapsed();
                Ok(MigrationResult::success(
                    duration,
                    format!("Executed SQL from {}", path.display()),
                ))
            }
            Err(FaultlineError::PostgresExecution(detail)) => {
                let duration = start.elapsed();
                Ok(MigrationResult::failure_sql(
                    duration,
                    detail.sqlstate.clone(),
                    detail.message.clone(),
                    detail.detail.clone(),
                ))
            }
            Err(e) => Err(e),
        }
    }
}

#[async_trait::async_trait]
impl MigrationRunner for SqlMigrationRunner {
    async fn run_up(&self, client: &PgClient, _db_url: &str) -> Result<MigrationResult> {
        if let Some(path) = &self.up_path {
            self.execute_file(client, path).await
        } else {
            Err(FaultlineError::Config(
                "No up migration SQL file specified".to_string(),
            ))
        }
    }

    async fn run_down(&self, client: &PgClient, _db_url: &str) -> Result<MigrationResult> {
        if let Some(path) = &self.down_path {
            self.execute_file(client, path).await
        } else {
            Err(FaultlineError::Config(
                "No down migration SQL file specified".to_string(),
            ))
        }
    }

    fn fingerprint(&self) -> Result<String> {
        let mut combined = String::new();
        if let Some(up) = &self.up_path {
            if up.exists() {
                combined.push_str(&fs::read_to_string(up)?);
            }
        }
        if let Some(down) = &self.down_path {
            if down.exists() {
                combined.push_str(&fs::read_to_string(down)?);
            }
        }
        Ok(compute_string_fingerprint(&combined))
    }
}
