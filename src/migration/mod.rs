pub mod analyzer;
pub mod command;
pub mod fingerprint;
pub mod sql;

use crate::db::client::PgClient;
use crate::db::error::{FailureClass, FailureSignature};
use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationResult {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub error_message: Option<String>,
    pub sqlstate: Option<String>,
    pub failure_class: Option<FailureClass>,
    #[serde(default)]
    pub failure_signature: Option<FailureSignature>,
    pub stdout: String,
    pub stderr: String,
}

impl MigrationResult {
    pub fn success(duration: Duration, stdout: String) -> Self {
        Self {
            success: true,
            exit_code: Some(0),
            duration_ms: duration.as_millis() as u64,
            error_message: None,
            sqlstate: None,
            failure_class: None,
            failure_signature: None,
            stdout,
            stderr: String::new(),
        }
    }

    pub fn failure_sql(
        duration: Duration,
        sqlstate: String,
        message: String,
        detail: Option<String>,
    ) -> Self {
        let failure_class = FailureClass::from_sqlstate(&sqlstate);
        let mut full_msg = format!("[{}] {}", sqlstate, message);
        if let Some(d) = detail {
            full_msg.push_str(&format!(" Detail: {}", d));
        }

        Self {
            success: false,
            exit_code: Some(1),
            duration_ms: duration.as_millis() as u64,
            error_message: Some(full_msg),
            sqlstate: Some(sqlstate.clone()),
            failure_class: Some(failure_class),
            failure_signature: Some(FailureSignature::new(
                failure_class,
                Some(sqlstate.clone()),
                &message,
            )),
            stdout: String::new(),
            stderr: message,
        }
    }

    pub fn failure_command(
        duration: Duration,
        exit_code: Option<i32>,
        stdout: String,
        stderr: String,
    ) -> Self {
        let mut failure_class = FailureClass::CommandFailure;
        let mut sqlstate = None;

        // Try extracting SQLSTATE from stderr if present (e.g. 23505).
        for token in stderr.split(|c: char| !c.is_ascii_alphanumeric()) {
            if token.len() == 5
                && token.chars().take(2).all(|c| c.is_ascii_alphanumeric())
                && token.chars().skip(2).all(|c| c.is_ascii_alphanumeric())
            {
                sqlstate = Some(token.to_string());
                failure_class = FailureClass::from_sqlstate(token);
                break;
            }
        }

        Self {
            success: false,
            exit_code,
            duration_ms: duration.as_millis() as u64,
            error_message: Some(stderr.clone()),
            sqlstate: sqlstate.clone(),
            failure_class: Some(failure_class),
            failure_signature: Some(FailureSignature::new(
                failure_class,
                sqlstate.clone(),
                &stderr,
            )),
            stdout,
            stderr,
        }
    }
}

#[async_trait::async_trait]
pub trait MigrationRunner: Send + Sync {
    async fn run_up(&self, client: &PgClient, db_url: &str) -> Result<MigrationResult>;
    async fn run_down(&self, client: &PgClient, db_url: &str) -> Result<MigrationResult>;
    fn fingerprint(&self) -> Result<String>;
}
