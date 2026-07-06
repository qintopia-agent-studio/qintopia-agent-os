# MCP: Context Server

`mcp/context-server` is the package contract for the context MCP surface currently
implemented in `qintopia-message-sidecar`.

## Current Source

- Local source: `../qintopia-message-sidecar/src/context_mcp_server.rs`
- Supporting modules: `src/context_tools.rs`, `src/knowledge.rs`,
  `src/member_profile.rs`, `src/message_search.rs`
- Operations doc: `../qintopia-message-sidecar/docs/operations/context-mcp.md`
- Adopted reference: `eda2652f21999e4f32699463413372accbd3b76e`

## Responsibility

The context server prepares safe answer context for Agent profiles such as Wenyuange and
Erhua. It must distinguish static knowledge, discussion evidence, member-safe context,
and live operational questions.

Agent-facing Postgres context behavior is owned by `skills/postgres-context`. Start
there before changing member-safe context, Erhua answer context, or trainer-memory
writes in `runtime/sidecar/src/context_tools.rs`.

## Boundaries

- It may read Postgres-backed context and evidence.
- It must not answer live availability or booking state from stale documents.
- It must not expose private raw member data or unrestricted chat logs.
- It must not mutate Hermes profiles or send external messages.

## Validation

Run source tests plus context acceptance smoke before adopting changes.

```bash
pnpm skills:postgres-context:check
pnpm test:sidecar
```
