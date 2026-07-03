# Sidecar Source Snapshot

Date: 2026-07-03

## Source

- Repository: `../qintopia-message-sidecar`
- Branch: `main`
- Commit: `eda2652f21999e4f32699463413372accbd3b76e`
- Commit subject: `Add AgentOS operations control plane`

## Imported

- `Cargo.toml`
- `Cargo.lock`
- `src/`
- `config/agentos/`
- `fixtures/`
- `.env.example`
- `AGENTS.md`

## Split To Related Packages

- `migrations/` -> `runtime/postgres/migrations/`
- `docs/data-design/` -> `runtime/postgres/docs/data-design/`
- `docs/operations/context-mcp.md` -> `mcp/context-server/docs/`
- `docs/operations/message-store-mcp.md` -> `mcp/message-store/docs/`
- `docs/operations/agentos-operations-control-plane.md` ->
  `workflows/activity-promotion/docs/`
- `docs/operations/server-deployment.md` -> `deploy/sidecar/docs/`
- `scripts/` -> `deploy/sidecar/scripts/`

## Excluded

- `.git/`
- `target/`
- `vendor/`
- real `.env` files
- runtime credentials and server-only state

## Local Patch

`runtime/sidecar/src/db.rs` now resolves migrations from `runtime/postgres/migrations`
by default when running inside the monorepo. Set `QINTOPIA_SIDECAR_MIGRATIONS_DIR` to
override the path.
