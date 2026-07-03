# Message Store MCP Tool Spec

Status: draft v1  
Owner surface: WenYuanGe / controlled Agent OS callers  
Implementation target: `qintopia-message-sidecar mcp-message-store`

## Goal

Expose a minimal MCP stdio server that lets authorized Agents search the captured QiWe
message store through a controlled tool. The MCP surface is for message recall and
evidence lookup only. It must not expose arbitrary SQL, database credentials, raw vector
values, or write operations.

This is intentionally not an internal HTTP API. The first target is that an MCP client
can list and call the message search tool.

## Tool

### `qintopia_message_store_search`

Read-only search over:

- `qintopia_messages.messages`
- `qintopia_messages.message_embeddings`

Default behavior is `hybrid`: semantic recall when query embeddings are configured,
keyword recall for query terms, and recent-message fallback.

The tool is intended for WenYuanGe or a later filtered context service. Frontline Agents
should not receive broad raw message search. They should receive filtered, sourced
context from WenYuanGe.

## Input Schema

```json
{
  "type": "object",
  "properties": {
    "query": {
      "type": "string",
      "description": "Natural language query or keywords. Required for semantic and keyword search."
    },
    "search_mode": {
      "type": "string",
      "enum": ["hybrid", "semantic", "keyword", "recent"],
      "description": "Defaults to hybrid."
    },
    "chat_id": {
      "type": "string",
      "description": "Optional QiWe chat/group id filter."
    },
    "sender_id": {
      "type": "string",
      "description": "Optional QiWe sender id filter."
    },
    "chat_type": {
      "type": "string",
      "enum": ["group", "direct"],
      "description": "Optional chat type filter."
    },
    "message_kind": {
      "type": "string",
      "description": "Optional message kind filter, for example text."
    },
    "since": {
      "type": "string",
      "description": "Optional lower timestamp bound. RFC3339 is preferred."
    },
    "until": {
      "type": "string",
      "description": "Optional upper timestamp bound. RFC3339 is preferred."
    },
    "limit": {
      "type": "integer",
      "minimum": 1,
      "maximum": 50,
      "description": "Maximum messages to return. Defaults to 20."
    },
    "caller": {
      "type": "string",
      "description": "Calling profile id. v1 expects wenyuange."
    },
    "purpose": {
      "type": "string",
      "description": "Why this message search is needed. Required."
    }
  },
  "additionalProperties": false
}
```

At least one of `query`, `chat_id`, `sender_id`, `chat_type`, `message_kind`, `since`,
or `until` must be present. `purpose` is required. v1 rejects callers other than
`wenyuange` unless the MCP process is configured with a different allowed caller.

## Output Shape

The MCP tool returns one JSON text content item. The JSON object has:

- `success`: boolean.
- `tool`: `qintopia_message_store_search`.
- `source`: `postgres_qintopia_messages`.
- `read_only`: always true.
- `query`, `query_terms`, `search_mode`.
- `filters`: normalized filters used.
- `retrieval_trace`: semantic/keyword/recent execution status.
- `result_count`.
- `messages`: ranked message evidence.

Each message evidence item includes:

- `id`: internal message UUID.
- `message_id`: platform message id.
- `platform`, `chat_id`, `chat_type`.
- `sender_id`, `sender_name`.
- `message_kind`.
- `text_preview`: truncated message text, not the full raw payload.
- `sent_at`, `received_at`.
- `retrieval_methods`.
- `retrieval_score`.
- `semantic_distance` when semantic search contributed.
- `matched_terms` when keyword search contributed.

## Runtime Configuration

The MCP server reuses the sidecar database and message embedding configuration:

```env
QINTOPIA_SIDECAR_DATABASE_URL=postgres://USER:PASSWORD@127.0.0.1:55432/qintopia
QINTOPIA_EMBEDDING_BASE_URL=https://livecool.net
QINTOPIA_EMBEDDING_API_KEY=replace-with-server-secret
QINTOPIA_MESSAGE_EMBEDDING_ENDPOINT=https://ark.cn-beijing.volces.com/api/plan/v3/embeddings
QINTOPIA_MESSAGE_EMBEDDING_MODEL=doubao-embedding-vision
QINTOPIA_MESSAGE_STORE_MCP_ALLOWED_CALLER=wenyuange
```

If the embedding key or endpoint is not configured, `hybrid` and `semantic` search still
run keyword/recent fallback and report the semantic skip in `retrieval_trace`.

## MCP Methods

The server implements the minimal MCP JSON-RPC methods required for tool use:

- `initialize`
- `tools/list`
- `tools/call`
- `notifications/initialized`

Unknown tools and unsupported methods return JSON-RPC errors. The server reads
newline-delimited JSON-RPC messages from stdin and writes newline-delimited responses to
stdout.

## Guardrails

- No write operations.
- No arbitrary SQL.
- No raw vector output.
- No database URL, API key, or secret output.
- Default caller gate is `wenyuange`.
- Search results return text previews and metadata, not raw event payloads.
- Query execution sets `search_path` to `qintopia_messages, public` so pgvector
  operators installed in `qintopia_messages` resolve consistently.

## Validation

Local validation before deploy:

```bash
rtk cargo fmt --check
rtk cargo check
rtk cargo test
```

Live validation with real server env:

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"smoke","version":"0.1.0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"qintopia_message_store_search","arguments":{"caller":"wenyuange","purpose":"smoke test","query":"wifi 密码","limit":5}}}' \
  | ./target/release/qintopia-message-sidecar mcp-message-store
```
