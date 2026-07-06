# Postgres Context Skill

This package defines the future extraction boundary for Postgres-backed Agent context,
safe profile snapshots, and audited training memory operations.

The active implementation currently lives in `runtime/sidecar/src/context_tools.rs` and
the context MCP package. New context behavior should be specified here before it becomes
a shared tool capability.

## Capability

- read-only safe member or context snapshots;
- prepare Erhua reply context with source evidence;
- write only audited Erhua trainer-memory notes through allowlisted trainer IDs;
- record audit and idempotency fields for write-capable operations.

## Validation

```bash
pnpm skills:postgres-context:check
pnpm test:sidecar
pnpm check:light
```
