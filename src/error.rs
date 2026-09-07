use std::fmt;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostgresErrorDetail {
    pub sqlstate: String,
    pub message: String,
    pub detail: Option<String>,
    pub table: Option<String>,
    pub column: Option<String>,
    pub constraint: Option<String>,
}

impl fmt::Display for PostgresErrorDetail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.sqlstate, self.message)?;
        if let Some(constraint) = &self.constraint {
            write!(f, " (constraint: {})", constraint)?;
        }
        if let Some(table) = &self.table {
            write!(f, " (table: {})", table)?;
        }
        if let Some(detail) = &self.detail {
            write!(f, " Detail: {}", detail)?;
        }
        Ok(())
    }
}

#[derive(Error, Debug)]
pub enum FaultlineError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("YAML serialization error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Database connection error: {0}")]
    DbConnection(String),

    #[error("Database query error: {0}")]
    DbQuery(String),

    #[error("PostgreSQL execution error: {0}")]
    PostgresExecution(Box<PostgresErrorDetail>),

    #[error("Safety violation: {0}")]
    SafetyViolation(String),

    #[error("Schema error: {0}")]
    Schema(String),

    #[error("Migration execution error (exit {status}): {stderr}")]
    MigrationFailed {
        status: i32,
        stderr: String,
        stdout: String,
    },

    #[error("Migration timeout after {0} seconds")]
    MigrationTimeout(u64),

    #[error("Data generation error: {0}")]
    Generation(String),

    #[error("Minimization error: {0}")]
    Minimization(String),

    #[error("Counterexample not found at path: {0}")]
    CounterexampleNotFound(PathBuf),

    #[error("Semantic invariant violated: {0}")]
    InvariantViolation(String),

    #[error("Session error: {0}")]
    Session(String),

    #[error("{0}")]
    General(String),
}

pub type Result<T> = std::result::Result<T, FaultlineError>;
