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
            Err(_) => Err(FaultlineError::MigrationTimeout(
                self.timeout.as_secs().max(1),
            )),
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

    fn replay_command(&self) -> Option<String> {
        self.up_command.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::error::FailureClass;

    #[tokio::test]
    async fn timeout_is_reported_as_a_migration_timeout() {
        let runner = CommandMigrationRunner {
            up_command: None,
            down_command: None,
            timeout: Duration::from_millis(20),
        };

        let result = runner
            .execute_command("sleep 1", "postgres://localhost/test")
            .await;
        assert!(matches!(result, Err(FaultlineError::MigrationTimeout(1))));
    }

    #[tokio::test]
    async fn command_failures_preserve_sqlstate_signature() {
        let runner = CommandMigrationRunner {
            up_command: None,
            down_command: None,
            timeout: Duration::from_secs(1),
        };

        let result = runner
            .execute_command(
                "printf 'ERROR: duplicate key value violates unique constraint (SQLSTATE 23505)\\n' >&2; exit 3",
                "postgres://localhost/test",
            )
            .await
            .unwrap();

        assert_eq!(result.exit_code, Some(3));
        assert_eq!(result.failure_class, Some(FailureClass::UniqueViolation));
        assert_eq!(result.sqlstate.as_deref(), Some("23505"));
        assert!(result.failure_signature.is_some());
    }

    #[tokio::test]
    async fn crashed_migration_process_is_a_command_failure() {
        let runner = CommandMigrationRunner {
            up_command: None,
            down_command: None,
            timeout: Duration::from_secs(1),
        };

        let result = runner
            .execute_command("kill -KILL $$", "postgres://localhost/test")
            .await
            .unwrap();

        assert!(!result.success);
        assert_eq!(result.failure_class, Some(FailureClass::CommandFailure));
        assert_eq!(result.exit_code, None);
    }
}
