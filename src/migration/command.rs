use crate::db::client::PgClient;
use crate::error::{FaultlineError, Result};
use crate::migration::fingerprint::compute_string_fingerprint;
use crate::migration::{MigrationResult, MigrationRunner};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::process::Command;

pub struct CommandMigrationRunner {
    pub up_command: Option<String>,
    pub down_command: Option<String>,
    pub timeout: Duration,
}

impl CommandMigrationRunner {
    pub fn new(
        up_command: Option<String>,
        down_command: Option<String>,
        timeout_secs: u64,
    ) -> Self {
        Self {
            up_command,
            down_command,
            timeout: Duration::from_secs(timeout_secs),
        }
    }

    async fn execute_command(&self, cmd_str: &str, db_url: &str) -> Result<MigrationResult> {
        let start = Instant::now();

        let mut cmd = Command::new("bash");
        cmd.arg("-c")
            .arg(cmd_str)
            .env("DATABASE_URL", db_url)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let child = cmd.spawn().map_err(|e| {
            FaultlineError::General(format!("Failed to spawn migration command: {}", e))
        })?;

        let timeout_result = tokio::time::timeout(self.timeout, child.wait_with_output()).await;

        match timeout_result {
            Ok(Ok(output)) => {
                let duration = start.elapsed();
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                if output.status.success() {
                    Ok(MigrationResult::success(duration, stdout))
                } else {
                    Ok(MigrationResult::failure_command(
                        duration,
                        output.status.code(),
                        stdout,
                        stderr,
                    ))
                }
            }
            Ok(Err(e)) => Err(FaultlineError::General(format!(
                "Command execution error: {}",
                e
            ))),
            Err(_) => Err(FaultlineError::MigrationTimeout(self.timeout.as_secs())),
        }
    }
}

#[async_trait::async_trait]
impl MigrationRunner for CommandMigrationRunner {
    async fn run_up(&self, _client: &PgClient, db_url: &str) -> Result<MigrationResult> {
        if let Some(cmd) = &self.up_command {
            self.execute_command(cmd, db_url).await
        } else {
            Err(FaultlineError::Config(
                "No up command configured".to_string(),
            ))
        }
    }

    async fn run_down(&self, _client: &PgClient, db_url: &str) -> Result<MigrationResult> {
        if let Some(cmd) = &self.down_command {
            self.execute_command(cmd, db_url).await
        } else {
            Err(FaultlineError::Config(
                "No down command configured".to_string(),
            ))
        }
    }

    fn fingerprint(&self) -> Result<String> {
        let mut combined = String::new();
        if let Some(up) = &self.up_command {
            combined.push_str(up);
        }
        if let Some(down) = &self.down_command {
            combined.push_str(down);
        }
        Ok(compute_string_fingerprint(&combined))
    }
}
