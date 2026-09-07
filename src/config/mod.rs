use crate::error::{FaultlineError, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectConfig {
    pub name: String,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            name: "faultline-project".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DatabaseConfig {
    #[serde(rename = "type", default = "default_db_type")]
    pub db_type: String,

    #[serde(default)]
    pub url_env: Option<String>,

    #[serde(default)]
    pub url: Option<String>,

    #[serde(default)]
    pub allow_non_isolated: bool,
}

fn default_db_type() -> String {
    "postgres".to_string()
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            db_type: "postgres".to_string(),
            url_env: Some("DATABASE_URL".to_string()),
            url: None,
            allow_non_isolated: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommandConfig {
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MigrationConfig {
    #[serde(default)]
    pub up: Option<CommandConfig>,

    #[serde(default)]
    pub down: Option<CommandConfig>,

    #[serde(default)]
    pub up_sql: Option<PathBuf>,

    #[serde(default)]
    pub down_sql: Option<PathBuf>,

    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

fn default_timeout_secs() -> u64 {
    30
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TestingConfig {
    #[serde(default = "default_experiments")]
    pub experiments: usize,

    #[serde(default)]
    pub seed: Option<u64>,

    #[serde(default)]
    pub time_limit_secs: Option<u64>,

    #[serde(default = "default_max_rows_per_table")]
    pub max_rows_per_table: usize,

    #[serde(default)]
    pub strategies: Vec<String>,
}

fn default_experiments() -> usize {
    500
}

fn default_max_rows_per_table() -> usize {
    100
}

impl Default for TestingConfig {
    fn default() -> Self {
        Self {
            experiments: default_experiments(),
            seed: None,
            time_limit_secs: None,
            max_rows_per_table: default_max_rows_per_table(),
            strategies: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChecksConfig {
    #[serde(default = "default_true")]
    pub migration_success: bool,

    #[serde(default)]
    pub roundtrip: bool,

    #[serde(default = "default_true")]
    pub semantic_loss: bool,
}

fn default_true() -> bool {
    true
}

impl Default for ChecksConfig {
    fn default() -> Self {
        Self {
            migration_success: true,
            roundtrip: false,
            semantic_loss: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct InvariantsConfig {
    #[serde(default)]
    pub preserve: Vec<String>,

    #[serde(default)]
    pub ignore: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FaultlineConfig {
    #[serde(default = "default_version")]
    pub version: u32,

    #[serde(default)]
    pub project: ProjectConfig,

    #[serde(default)]
    pub database: DatabaseConfig,

    #[serde(default)]
    pub migration: MigrationConfig,

    #[serde(default)]
    pub testing: TestingConfig,

    #[serde(default)]
    pub checks: ChecksConfig,

    #[serde(default)]
    pub invariants: InvariantsConfig,
}

fn default_version() -> u32 {
    1
}

impl Default for FaultlineConfig {
    fn default() -> Self {
        Self {
            version: 1,
            project: ProjectConfig::default(),
            database: DatabaseConfig::default(),
            migration: MigrationConfig::default(),
            testing: TestingConfig::default(),
            checks: ChecksConfig::default(),
            invariants: InvariantsConfig::default(),
        }
    }
}

impl FaultlineConfig {
    pub fn load_from_file(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Err(FaultlineError::Config(format!(
                "Configuration file not found: {}",
                path.display()
            )));
        }
        let content = fs::read_to_string(path)?;
        Self::parse_yaml(&content)
    }

    pub fn discover_and_load(dir: &Path) -> Result<(Self, PathBuf)> {
        let candidates = [
            dir.join("faultline.yaml"),
            dir.join("faultline.yml"),
            dir.join(".faultline.yaml"),
            dir.join(".faultline.yml"),
        ];

        for candidate in candidates.iter() {
            if candidate.exists() {
                let config = Self::load_from_file(candidate)?;
                return Ok((config, candidate.clone()));
            }
        }

        Err(FaultlineError::Config(
            "No faultline.yaml configuration found in current workspace. Run `faultline init` to create one.".to_string(),
        ))
    }

    pub fn parse_yaml(content: &str) -> Result<Self> {
        let config: FaultlineConfig = serde_yaml::from_str(content)?;
        config.validate()?;
        Ok(config)
    }

    pub fn to_yaml(&self) -> Result<String> {
        Ok(serde_yaml::to_string(self)?)
    }

    pub fn validate(&self) -> Result<()> {
        if self.database.db_type != "postgres" && self.database.db_type != "postgresql" {
            return Err(FaultlineError::Config(format!(
                "Unsupported database type '{}'. v0.1 supports 'postgres'",
                self.database.db_type
            )));
        }

        if self.migration.up.is_none() && self.migration.up_sql.is_none() {
            return Err(FaultlineError::Config(
                "Migration configuration must specify either 'migration.up.command' or 'migration.up_sql'".to_string(),
            ));
        }

        Ok(())
    }

    pub fn get_database_url(&self) -> Result<String> {
        if let Some(env_var) = &self.database.url_env {
            if let Ok(val) = std::env::var(env_var) {
                if !val.trim().is_empty() {
                    return Ok(val);
                }
            }
        }

        if let Some(url) = &self.database.url {
            if !url.trim().is_empty() {
                return Ok(url.clone());
            }
        }

        Err(FaultlineError::Config(
            "Database URL could not be resolved. Set DATABASE_URL environment variable or configure database.url / database.url_env in faultline.yaml".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_yaml() {
        let yaml = r#"
version: 1
project:
  name: test-project
database:
  type: postgres
  url_env: TEST_DB_URL
migration:
  up_sql: "./migrations/001.sql"
testing:
  experiments: 100
  seed: 42
checks:
  migration_success: true
  roundtrip: false
"#;
        let config = FaultlineConfig::parse_yaml(yaml).expect("Failed to parse YAML");
        assert_eq!(config.project.name, "test-project");
        assert_eq!(config.testing.experiments, 100);
        assert_eq!(config.testing.seed, Some(42));
    }

    #[test]
    fn test_validate_missing_migration() {
        let yaml = r#"
version: 1
project:
  name: test-project
database:
  type: postgres
"#;
        assert!(FaultlineConfig::parse_yaml(yaml).is_err());
    }
}
