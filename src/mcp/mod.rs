use crate::config::FaultlineConfig;
use crate::db::client::PgClient;
use crate::error::Result;
use crate::migration::analyzer::MigrationAnalyzer;
use crate::schema::introspection::SchemaInspector;
use crate::storage::StorageManager;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<Value>,
}

pub struct McpServer;

impl McpServer {
    pub async fn run() -> Result<()> {
        let stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();
        let mut reader = BufReader::new(stdin).lines();

        while let Ok(Some(line)) = reader.next_line().await {
            if line.trim().is_empty() {
                continue;
            }

            let request: JsonRpcRequest = match serde_json::from_str(&line) {
                Ok(req) => req,
                Err(e) => {
                    let err_resp = JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: None,
                        result: None,
                        error: Some(json!({
                            "code": -32700,
                            "message": format!("Parse error: {}", e)
                        })),
                    };
                    let resp_str = serde_json::to_string(&err_resp)? + "\n";
                    stdout.write_all(resp_str.as_bytes()).await?;
                    stdout.flush().await?;
                    continue;
                }
            };

            let response = Self::handle_request(request).await;
            let resp_str = serde_json::to_string(&response)? + "\n";
            stdout.write_all(resp_str.as_bytes()).await?;
            stdout.flush().await?;
        }

        Ok(())
    }

    async fn handle_request(req: JsonRpcRequest) -> JsonRpcResponse {
        let result = match req.method.as_str() {
            "initialize" => Ok(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "faultline-mcp",
                    "version": "0.1.0"
                }
            })),
            "tools/list" => Ok(json!({
                "tools": [
                    {
                        "name": "get_project_status",
                        "description": "Get current Faultline project configuration and health status",
                        "inputSchema": {
                            "type": "object",
                            "properties": {}
                        }
                    },
                    {
                        "name": "inspect_schema",
                        "description": "Inspect live PostgreSQL database schema and return discovered tables, columns, and constraints",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "table": { "type": "string", "description": "Optional table name filter" }
                            }
                        }
                    },
                    {
                        "name": "inspect_migration",
                        "description": "Analyze migration SQL for operations and get recommended counterexample strategies",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "sql": { "type": "string", "description": "Migration SQL content" }
                            },
                            "required": ["sql"]
                        }
                    },
                    {
                        "name": "list_strategies",
                        "description": "List all available counterexample search strategies",
                        "inputSchema": {
                            "type": "object",
                            "properties": {}
                        }
                    },
                    {
                        "name": "list_counterexamples",
                        "description": "List all discovered migration counterexamples",
                        "inputSchema": {
                            "type": "object",
                            "properties": {}
                        }
                    }
                ]
            })),
            "tools/call" => {
                let tool_name = req
                    .params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let arguments = req.params.get("arguments").cloned().unwrap_or(json!({}));
                Self::call_tool(tool_name, arguments).await
            }
            _ => Err(format!("Method not found: {}", req.method)),
        };

        match result {
            Ok(val) => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: req.id,
                result: Some(val),
                error: None,
            },
            Err(msg) => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: req.id,
                result: None,
                error: Some(json!({
                    "code": -32603,
                    "message": msg
                })),
            },
        }
    }

    async fn call_tool(name: &str, args: Value) -> std::result::Result<Value, String> {
        let storage = StorageManager::new(".");

        match name {
            "get_project_status" => {
                let (config, path) = FaultlineConfig::discover_and_load(Path::new("."))
                    .map_err(|e| format!("Failed to load config: {}", e))?;
                Ok(json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&json!({
                            "config_path": path.display().to_string(),
                            "project": config.project.name,
                            "database_type": config.database.db_type,
                            "checks": config.checks
                        })).unwrap()
                    }]
                }))
            }
            "inspect_schema" => {
                let (config, _) = FaultlineConfig::discover_and_load(Path::new("."))
                    .map_err(|e| format!("Failed to load config: {}", e))?;
                let db_url = config.get_database_url().map_err(|e| e.to_string())?;
                let client = PgClient::connect(&db_url)
                    .await
                    .map_err(|e| e.to_string())?;
                let table_filter = args.get("table").and_then(|v| v.as_str());
                let schema = SchemaInspector::introspect(&client, table_filter)
                    .await
                    .map_err(|e| e.to_string())?;

                Ok(json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&schema).unwrap()
                    }]
                }))
            }
            "inspect_migration" => {
                let sql = args.get("sql").and_then(|v| v.as_str()).unwrap_or("");
                let hints = MigrationAnalyzer::analyze_sql(sql);
                Ok(json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&hints).unwrap()
                    }]
                }))
            }
            "list_strategies" => {
                let strategies = vec![
                    json!({ "name": "collision", "description": "Targets case-folding and whitespace uniqueness collisions" }),
                    json!({ "name": "precision", "description": "Targets numeric and floating-point precision loss and truncation" }),
                    json!({ "name": "nullability", "description": "Targets schema-valid nulls breaking NOT NULL additions" }),
                    json!({ "name": "boundary", "description": "Tests extreme bounds (min, max, empty strings, overflow limits)" }),
                    json!({ "name": "random", "description": "Property-based random state exploration" }),
                ];
                Ok(json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&strategies).unwrap()
                    }]
                }))
            }
            "list_counterexamples" => {
                let dir = storage.counterexamples_dir();
                let mut list = Vec::new();
                if dir.exists() {
                    if let Ok(entries) = fs::read_dir(dir) {
                        for entry in entries.flatten() {
                            let manifest_path = entry.path().join("manifest.json");
                            if manifest_path.exists() {
                                if let Ok(content) = fs::read_to_string(&manifest_path) {
                                    if let Ok(val) = serde_json::from_str::<Value>(&content) {
                                        list.push(val);
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&list).unwrap()
                    }]
                }))
            }
            _ => Err(format!("Unknown tool: {}", name)),
        }
    }
}
