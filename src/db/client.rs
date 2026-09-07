use crate::db::error::classify_postgres_error;
use crate::error::{FaultlineError, Result};
use tokio_postgres::{Client, NoTls, Row};

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
}
