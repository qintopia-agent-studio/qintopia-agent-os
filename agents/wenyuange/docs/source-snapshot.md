# Wenyuange Source Snapshot

Snapshot date: 2026-07-03
Mode: read-only inventory

## Observed Runtime

- Profile path: `/home/ubuntu/.hermes/profiles/wenyuange`
- User service: `hermes-gateway-wenyuange.service`
- Key non-secret file names observed: `SOUL.md`, `config.yaml`, `profile.yaml`,
  `channel_directory.json`
- Related docs already adopted under `mcp/context-server` and `mcp/message-store`

## Adopted As

This package records the Agent boundary only. It excludes runtime memories, auth files,
sessions, request dumps, caches, locks, and database files.

## Source Inputs

- `../qintopia-agent-os/docs/agent-os/wenyuange-knowledge-research-capability.md`
- Context MCP and message-store MCP docs
- Server read-only profile inventory
