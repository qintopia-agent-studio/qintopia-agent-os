use std::io::{self, BufRead, Write};

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    config::Cli,
    db,
    message_search::{self, request_from_json, SearchConfig, TOOL_NAME},
};

const PROTOCOL_VERSION: &str = "2025-06-18";

pub(crate) async fn run(cli: &Cli) -> Result<()> {
    let config = SearchConfig::from_cli(cli)?;
    let pool = db::connect(&config.database_url, config.db_max_connections).await?;
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line.context("read MCP stdin")?;
        if line.trim().is_empty() {
            continue;
        }
        let response = handle_message(&pool, &config, &line).await;
        if let Some(response) = response {
            writeln!(stdout, "{response}").context("write MCP stdout")?;
            stdout.flush().context("flush MCP stdout")?;
        }
    }
    Ok(())
}

async fn handle_message(
    pool: &sqlx::postgres::PgPool,
    config: &SearchConfig,
    line: &str,
) -> Option<String> {
    let message = match serde_json::from_str::<RpcMessage>(line) {
        Ok(message) => message,
        Err(error) => {
            return Some(
                json!({
                    "jsonrpc": "2.0",
                    "id": Value::Null,
                    "error": {"code": -32700, "message": format!("Parse error: {error}")}
                })
                .to_string(),
            )
        }
    };

    let id = match message.id.clone() {
        Some(id) => id,
        None => {
            if message.method == "notifications/initialized" {
                return None;
            }
            return None;
        }
    };

    let result = match message.method.as_str() {
        "initialize" => Ok(json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "qintopia-message-store-mcp",
                "version": env!("CARGO_PKG_VERSION")
            }
        })),
        "tools/list" => Ok(json!({
            "tools": [
                {
                    "name": TOOL_NAME,
                    "description": "Search Qintopia QiWe message store evidence with controlled semantic, keyword, and recent-message retrieval.",
                    "inputSchema": message_search::tool_input_schema()
                }
            ]
        })),
        "tools/call" => call_tool(pool, config, message.params).await,
        _ => Err(RpcError {
            code: -32601,
            message: format!("Method not found: {}", message.method),
        }),
    };

    Some(match result {
        Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string(),
        Err(error) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {"code": error.code, "message": error.message}
        })
        .to_string(),
    })
}

async fn call_tool(
    pool: &sqlx::postgres::PgPool,
    config: &SearchConfig,
    params: Option<Value>,
) -> std::result::Result<Value, RpcError> {
    let params = params.unwrap_or_else(|| json!({}));
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if name != TOOL_NAME {
        return Err(RpcError {
            code: -32602,
            message: format!("Unknown tool: {name}"),
        });
    }
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let request = request_from_json(arguments).map_err(|error| RpcError {
        code: -32602,
        message: error.to_string(),
    })?;
    let result = message_search::search_messages(pool, config, request)
        .await
        .map_err(|error| RpcError {
            code: -32000,
            message: error.to_string(),
        })?;
    let text = serde_json::to_string_pretty(&result).map_err(|error| RpcError {
        code: -32000,
        message: error.to_string(),
    })?;
    Ok(json!({
        "content": [
            {
                "type": "text",
                "text": text
            }
        ],
        "isError": false
    }))
}

#[derive(Debug, Deserialize)]
struct RpcMessage {
    #[allow(dead_code)]
    jsonrpc: Option<String>,
    id: Option<Value>,
    method: String,
    params: Option<Value>,
}

#[derive(Debug)]
struct RpcError {
    code: i32,
    message: String,
}
