use crate::error::{FaultlineError, PostgresErrorDetail};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailureClass {
    UniqueViolation,
    NotNullViolation,
    ForeignKeyViolation,
    CheckViolation,
    NumericOutOfRange,
    InvalidTextRepresentation,
    TypeCastError,
    UndefinedColumn,
    UndefinedTable,
    SemanticLoss,
    IrreversibleMigration,
    CommandFailure,
    Timeout,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FailureSignature {
    pub failure_class: FailureClass,
    pub sqlstate: Option<String>,
    pub normalized_message: String,
}

impl FailureSignature {
    pub fn new(
        failure_class: FailureClass,
        sqlstate: Option<String>,
        message: impl AsRef<str>,
    ) -> Self {
        Self {
            failure_class,
            sqlstate,
            normalized_message: normalize_failure_message(message.as_ref()),
        }
    }

    pub fn matches(&self, other: &Self) -> bool {
        self == other
    }
}

pub fn normalize_failure_message(message: &str) -> String {
    message
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

impl FailureClass {
    pub const DATA_EXCEPTION_LIKE: FailureClass = FailureClass::TypeCastError;

    pub fn from_sqlstate(code: &str) -> Self {
        match code {
            "23505" => FailureClass::UniqueViolation,
            "23502" => FailureClass::NotNullViolation,
            "23503" => FailureClass::ForeignKeyViolation,
            "23514" => FailureClass::CheckViolation,
            "22003" => FailureClass::NumericOutOfRange,
            "22P02" => FailureClass::InvalidTextRepresentation,
            "42703" => FailureClass::UndefinedColumn,
            "42P01" => FailureClass::UndefinedTable,
            "42804" | "42846" => FailureClass::TypeCastError,
            _ => {
                if code.starts_with("23") {
                    FailureClass::CheckViolation
                } else if code.starts_with("22") {
                    FailureClass::DATA_EXCEPTION_LIKE
                } else {
                    FailureClass::Unknown
                }
            }
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            FailureClass::UniqueViolation => "UNIQUE VIOLATION",
            FailureClass::NotNullViolation => "NOT NULL VIOLATION",
            FailureClass::ForeignKeyViolation => "FOREIGN KEY VIOLATION",
            FailureClass::CheckViolation => "CHECK CONSTRAINT VIOLATION",
            FailureClass::NumericOutOfRange => "NUMERIC OUT OF RANGE",
            FailureClass::InvalidTextRepresentation => "INVALID TEXT REPRESENTATION",
            FailureClass::TypeCastError => "TYPE CAST ERROR",
            FailureClass::UndefinedColumn => "UNDEFINED COLUMN",
            FailureClass::UndefinedTable => "UNDEFINED TABLE",
            FailureClass::SemanticLoss => "SEMANTIC DATA LOSS",
            FailureClass::IrreversibleMigration => "IRREVERSIBLE MIGRATION",
            FailureClass::CommandFailure => "MIGRATION COMMAND FAILURE",
            FailureClass::Timeout => "TIMEOUT",
            FailureClass::Unknown => "UNKNOWN ERROR",
        }
    }
}

pub fn classify_postgres_error(err: &tokio_postgres::Error) -> FaultlineError {
    if let Some(db_err) = err.as_db_error() {
        let sqlstate = db_err.code().code().to_string();
        let message = db_err.message().to_string();
        let detail = db_err.detail().map(|s| s.to_string());
        let table = db_err.table().map(|s| s.to_string());
        let column = db_err.column().map(|s| s.to_string());
        let constraint = db_err.constraint().map(|s| s.to_string());

        FaultlineError::PostgresExecution(Box::new(PostgresErrorDetail {
            sqlstate,
            message,
            detail,
            table,
            column,
            constraint,
        }))
    } else {
        FaultlineError::DbQuery(err.to_string())
    }
}

pub fn signature_from_faultline_error(err: &FaultlineError) -> Option<FailureSignature> {
    match err {
        FaultlineError::PostgresExecution(detail) => Some(FailureSignature::new(
            FailureClass::from_sqlstate(&detail.sqlstate),
            Some(detail.sqlstate.clone()),
            &detail.message,
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_signature_normalizes_whitespace_and_case() {
        let first = FailureSignature::new(
            FailureClass::UniqueViolation,
            Some("23505".to_string()),
            "duplicate   KEY\nvalue",
        );
        let second = FailureSignature::new(
            FailureClass::UniqueViolation,
            Some("23505".to_string()),
            "Duplicate key value",
        );

        assert!(first.matches(&second));
    }

    #[test]
    fn failure_signature_keeps_distinct_sqlstates_separate() {
        let unique = FailureSignature::new(
            FailureClass::UniqueViolation,
            Some("23505".to_string()),
            "constraint failed",
        );
        let not_null = FailureSignature::new(
            FailureClass::NotNullViolation,
            Some("23502".to_string()),
            "constraint failed",
        );

        assert!(!unique.matches(&not_null));
    }
}
