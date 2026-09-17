use crate::db::error::classify_postgres_error;
use crate::error::{FaultlineError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tokio_postgres::{Client, NoTls, Row};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PostgresEnvironment {
    pub server_version: String,
    pub settings: BTreeMap<String, String>,
    pub extensions: Vec<String>,
}

pub struct PgClient {
    client: Client,
    url: String,
}

impl PgClient {
    pub async fn connect(url: &str) -> Result<Self> {
        let (client, connection) = tokio_postgres::connect(url, NoTls).await.map_err(|e| {
            FaultlineError::DbConnection(format!("Failed to connect to {}: {}", url, e))
        })?;

        // Spawn connection task to keep connection alive
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                tracing::debug!("Postgres connection error: {}", e);
            }
        });

        Ok(Self {
            client,
            url: url.to_string(),
        })
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub async fn execute(
        &self,
        query: &str,
        params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
    ) -> Result<u64> {
        self.client
            .execute(query, params)
            .await
            .map_err(|e| classify_postgres_error(&e))
    }

    pub async fn query(
        &self,
        query: &str,
        params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
    ) -> Result<Vec<Row>> {
        self.client
            .query(query, params)
            .await
            .map_err(|e| classify_postgres_error(&e))
    }

    pub async fn batch_execute(&self, query: &str) -> Result<()> {
        self.client
            .batch_execute(query)
            .await
            .map_err(|e| classify_postgres_error(&e))
    }

    pub async fn get_server_version(&self) -> Result<String> {
        let rows = self.query("SHOW server_version", &[]).await?;
        if let Some(row) = rows.first() {
            let version: String = row.get(0);
            Ok(version)
        } else {
            Ok("unknown".to_string())
        }
    }

    pub async fn get_current_database(&self) -> Result<String> {
        let rows = self.query("SELECT current_database()", &[]).await?;
        if let Some(row) = rows.first() {
            let db: String = row.get(0);
            Ok(db)
        } else {
            Ok("unknown".to_string())
        }
    }

    pub async fn get_environment(&self) -> Result<PostgresEnvironment> {
        let server_version = self.get_server_version().await?;
        let setting_rows = self.query("SHOW ALL", &[]).await?;
        let settings = setting_rows
            .into_iter()
            .map(|row| Ok((row.get::<_, String>(0), row.get::<_, String>(1))))
            .collect::<Result<BTreeMap<_, _>>>()?;
        let extension_rows = self
            .query(
                "SELECT extname, extversion FROM pg_extension ORDER BY extname",
                &[],
            )
            .await?;
        let extensions = extension_rows
            .into_iter()
            .map(|row| format!("{}={}", row.get::<_, String>(0), row.get::<_, String>(1)))
            .collect();

        Ok(PostgresEnvironment {
            server_version,
            settings,
            extensions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn connection_failures_are_classified_separately() {
        let result = PgClient::connect("postgres://127.0.0.1:1/faultline_test").await;
        assert!(matches!(result, Err(FaultlineError::DbConnection(_))));
    }
}
