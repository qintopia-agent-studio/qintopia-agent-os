use std::io::{self, BufRead, Write};

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    config::Cli,
    context_tools::{self, ContextConfig},
    db,
    message_search::SearchConfig,
};

const PROTOCOL_VERSION: &str = "2025-06-18";

pub(crate) async fn run(cli: &Cli) -> Result<()> {
    let search = SearchConfig::from_cli(cli)?;
    let config = ContextConfig::from_cli(cli, search);
    let pool = db::connect(
        &config.search.database_url,
        config.search.db_max_connections,
    )
    .await?;
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line.context("read context MCP stdin")?;
        if line.trim().is_empty() {
            continue;
        }
        let response = handle_message(&pool, &config, &line).await;
        if let Some(response) = response {
            writeln!(stdout, "{response}").context("write context MCP stdout")?;
            stdout.flush().context("flush context MCP stdout")?;
        }
    }
    Ok(())
}

async fn handle_message(
    pool: &sqlx::postgres::PgPool,
    config: &ContextConfig,
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
        None => return None,
    };

    let result = match message.method.as_str() {
        "initialize" => Ok(json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {"tools": {}},
            "serverInfo": {
                "name": "qintopia-context-mcp",
                "version": env!("CARGO_PKG_VERSION")
            }
        })),
        "tools/list" => Ok(json!({"tools": context_tools::tool_definitions()})),
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
    config: &ContextConfig,
    params: Option<Value>,
) -> std::result::Result<Value, RpcError> {
    let params = params.unwrap_or_else(|| json!({}));
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let caller = caller_from_arguments(&arguments);
    let result = context_tools::call_tool(pool, config, name, arguments)
        .await
        .map_err(|error| RpcError {
            code: -32000,
            message: error.to_string(),
        })?;
    audit_tool_call(name, &caller, &result);
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

fn audit_tool_call(name: &str, caller: &str, result: &Value) {
    let source_count = result
        .get("sources")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let confidence = result
        .get("confidence")
        .and_then(Value::as_str)
        .unwrap_or("");
    let success = result
        .get("success")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    eprintln!(
        "qintopia_context_mcp_audit tool={} caller={} success={} source_count={} confidence={}",
        sanitize_audit_value(name),
        sanitize_audit_value(caller),
        success,
        source_count,
        sanitize_audit_value(confidence)
    );
}

fn caller_from_arguments(arguments: &Value) -> String {
    arguments
        .get("caller")
        .or_else(|| arguments.get("caller_profile"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .chars()
        .take(80)
        .collect()
}

fn sanitize_audit_value(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
        .take(120)
        .collect()
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

#[cfg(test)]
mod tests {
    use super::{caller_from_arguments, sanitize_audit_value};
    use serde_json::json;

    #[test]
    fn audit_value_keeps_only_safe_name_characters() {
        assert_eq!(
            sanitize_audit_value("erhua; secret=abc 中文"),
            "erhuasecretabc"
        );
        assert_eq!(
            sanitize_audit_value("mcp_qintopia-context.lookup"),
            "mcp_qintopia-context.lookup"
        );
    }

    #[test]
    fn caller_from_arguments_accepts_caller_profile() {
        assert_eq!(
            caller_from_arguments(&json!({"caller_profile": "erhua"})),
            "erhua"
        );
        assert_eq!(
            caller_from_arguments(&json!({"caller": "wenyuange", "caller_profile": "erhua"})),
            "wenyuange"
        );
    }
}
