# MCP: Message Store

`mcp/message-store` is the package contract for Postgres-backed message and discussion
evidence lookup.

## Current Source

- Local source: `../qintopia-message-sidecar/src/mcp_server.rs`
- Supporting modules: `src/message_search.rs`, `src/evidence.rs`, `src/raw_archive.rs`
- Operations doc: `../qintopia-message-sidecar/docs/operations/message-store-mcp.md`
- Adopted reference: `eda2652f21999e4f32699463413372accbd3b76e`

## Responsibility

This package provides controlled retrieval over captured messages. It should be used for
evidence and discussion-history queries, not as an unrestricted log viewer.

## Boundaries

- Reads Postgres-backed message and evidence data.
- Does not send external messages.
- Does not write workflow state.
- Must avoid exposing private raw payloads beyond approved tool contracts.

## Validation

Run the source test suite and MCP-specific smoke commands before source import or
behavior changes.
