use crate::db::client::PgClient;
use crate::error::{FaultlineError, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use uuid::Uuid;

pub struct SafetyGuard;

impl SafetyGuard {
    pub fn verify_target_safety(db_url: &str, allow_non_isolated: bool) -> Result<()> {
        if allow_non_isolated {
            tracing::warn!("Safety override: running against non-isolated target as requested");
            return Ok(());
        }

        let lower = db_url.to_lowercase();

        // 1. Check for dangerous database names or production patterns
        let prod_keywords = [
            "prod",
            "production",
            "live",
            "primary",
            "master",
            "customer",
            "real",
        ];

        for kw in prod_keywords {
            // Check in url path (database name)
            if let Some(db_part) = lower.split('/').next_back() {
                let db_name = db_part.split('?').next().unwrap_or("");
                if db_name.contains(kw) && !db_name.starts_with("faultline_test") {
                    return Err(FaultlineError::SafetyViolation(format!(
                        "Refusing to run against database '{}' which contains production keyword '{}'. Use --allow-non-isolated if intentional.",
                        db_name, kw
                    )));
                }
            }
        }

        // 2. Check for public cloud hosted databases
        let cloud_hosts = [
            "rds.amazonaws.com",
            "database.azure.com",
            "cloudsql",
            "neon.tech",
            "supabase.co",
            "elephantsql.com",
            "render.com",
            "fly.dev",
        ];

        for host in cloud_hosts {
            if lower.contains(host) {
                return Err(FaultlineError::SafetyViolation(format!(
                    "Refusing to run against remote cloud database host '{}'. Faultline must run against isolated local/disposable environments.",
                    host
                )));
            }
        }

        Ok(())
    }
}

pub struct IsolatedDatabase {
    pub db_name: String,
    pub db_url: String,
    maintenance_url: String,
    cleaned: Arc<AtomicBool>,
}

impl IsolatedDatabase {
    pub async fn create(base_url: &str, allow_non_isolated: bool) -> Result<Self> {
        SafetyGuard::verify_target_safety(base_url, allow_non_isolated)?;

        let test_id = Uuid::new_v4().simple().to_string();
        let db_name = format!("faultline_test_{}", test_id);

        // Derive maintenance URL (connecting to 'postgres' db)
        let (maintenance_url, new_db_url) = derive_db_urls(base_url, &db_name)?;

        // Connect to maintenance DB to create new test DB
        let client = PgClient::connect(&maintenance_url).await?;
        let create_sql = format!("CREATE DATABASE \"{}\"", db_name);
        client.execute(&create_sql, &[]).await?;

        Ok(Self {
            db_name,
            db_url: new_db_url,
            maintenance_url,
            cleaned: Arc::new(AtomicBool::new(false)),
        })
    }

    pub async fn destroy(&self) -> Result<()> {
        if self.cleaned.swap(true, Ordering::SeqCst) {
            return Ok(());
        }

        let client = match PgClient::connect(&self.maintenance_url).await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(
                    "Failed to connect to maintenance db to drop {}: {}",
                    self.db_name,
                    e
                );
                return Ok(());
            }
        };

        // Terminate any remaining connections to this test database and drop it
        let terminate_sql = format!(
            "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '{}' AND pid <> pg_backend_pid()",
            self.db_name
        );
        let _ = client.execute(&terminate_sql, &[]).await;

        let drop_sql = format!("DROP DATABASE IF EXISTS \"{}\" WITH (FORCE)", self.db_name);
        client.execute(&drop_sql, &[]).await?;

        tracing::debug!("Destroyed isolated database: {}", self.db_name);
        Ok(())
    }
}

impl Drop for IsolatedDatabase {
    fn drop(&mut self) {
        if !self.cleaned.load(Ordering::SeqCst) {
            let db_name = self.db_name.clone();
            let maintenance_url = self.maintenance_url.clone();
            let cleaned = self.cleaned.clone();

            // Attempt synchronous or spawned cleanup on drop
            tokio::spawn(async move {
                if !cleaned.swap(true, Ordering::SeqCst) {
                    if let Ok(client) = PgClient::connect(&maintenance_url).await {
                        let _ = client.execute(
                            &format!("SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '{}' AND pid <> pg_backend_pid()", db_name),
                            &[],
                        ).await;
                        let _ = client
                            .execute(
                                &format!("DROP DATABASE IF EXISTS \"{}\" WITH (FORCE)", db_name),
                                &[],
                            )
                            .await;
                    }
                }
            });
        }
    }
}

fn derive_db_urls(base_url: &str, target_db_name: &str) -> Result<(String, String)> {
    let mut parts: Vec<&str> = base_url.split('/').collect();
    if parts.len() < 4 {
        return Err(FaultlineError::Config(format!(
            "Invalid database URL format: {}",
            base_url
        )));
    }

    let last = parts.pop().unwrap();
    let query_param = if let Some(pos) = last.find('?') {
        &last[pos..]
    } else {
        ""
    };

    let base_prefix = parts.join("/");
    let maintenance_url = format!("{}/postgres{}", base_prefix, query_param);
    let target_url = format!("{}/{}{}", base_prefix, target_db_name, query_param);

    Ok((maintenance_url, target_url))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_db_urls() {
        let base = "postgres://postgres:password@localhost:5432/my_app?sslmode=disable";
        let (maint, target) = derive_db_urls(base, "faultline_test_123").unwrap();
        assert_eq!(
            maint,
            "postgres://postgres:password@localhost:5432/postgres?sslmode=disable"
        );
        assert_eq!(
            target,
            "postgres://postgres:password@localhost:5432/faultline_test_123?sslmode=disable"
        );
    }

    #[test]
    fn test_safety_guard_rejects_production() {
        let prod_url = "postgres://user:pass@localhost:5432/production_db";
        assert!(SafetyGuard::verify_target_safety(prod_url, false).is_err());

        // But succeeds with allow_non_isolated
        assert!(SafetyGuard::verify_target_safety(prod_url, true).is_ok());
    }

    #[test]
    fn test_safety_guard_rejects_cloud_hosts() {
        let cloud_url = "postgres://user:pass@my-cluster.rds.amazonaws.com:5432/test";
        assert!(SafetyGuard::verify_target_safety(cloud_url, false).is_err());
    }
}
