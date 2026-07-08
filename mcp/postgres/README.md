# MCP Adapter: Postgres

`mcp/postgres` defines the Postgres MCP adapter contract for controlled Agent OS
context, memory, event, and operations data access.

## Responsibility

- Expose schema allowlists rather than unrestricted SQL access.
- Separate read-only context lookup from audited write capabilities.
- Keep QiWe speaker resolution deterministic: safe context reads may use exact
  chat-scoped identities or the pre-materialized QiWe platform identity with
  `chat_id=''`, but must not choose an arbitrary or newest cross-chat identity row.
- Require idempotency keys and audit fields for writes.
- Keep secrets, connection strings, passwords, snapshots, and live database dumps
  outside git.

## Related Capability Package

Start Postgres-backed Agent context changes from `skills/postgres-context/README.md`.
That package owns the Agent-facing contract for:

- `qintopia_member_context_lookup`;
- `qintopia_answer_context_prepare`;
- `qintopia_erhua_training_note_submit`.

This MCP package owns adapter and schema-access rules. It must not become unrestricted
SQL access for Agents.

## Production Boundary

- Postgres is the Agent OS fact source.
- Read operations may be exposed through safe MCP tools.
- Writes require explicit package documentation, audit fields, idempotency, and smoke
  coverage before production use.

## Validation

```bash
pnpm mcp:adapters:check
pnpm skills:postgres-context:check
```
