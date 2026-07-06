# MCP Adapter: Postgres

`mcp/postgres` defines the Postgres MCP adapter contract for controlled Agent OS
context, memory, event, and operations data access.

## Responsibility

- Expose schema allowlists rather than unrestricted SQL access.
- Separate read-only context lookup from audited write capabilities.
- Require idempotency keys and audit fields for writes.
- Keep secrets, connection strings, passwords, snapshots, and live database dumps
  outside git.

## Production Boundary

- Postgres is the Agent OS fact source.
- Read operations may be exposed through safe MCP tools.
- Writes require explicit package documentation, audit fields, idempotency, and smoke
  coverage before production use.

## Validation

```bash
pnpm mcp:adapters:check
```
