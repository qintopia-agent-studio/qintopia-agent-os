# Runtime: Sidecar

`runtime/sidecar` is the Agent OS data and worker service package adopted from the
existing `qintopia-message-sidecar` Rust service.

## Current Source

- Local source: `../qintopia-message-sidecar`
- Adopted reference: `eda2652f21999e4f32699463413372accbd3b76e`
- Server deployment source observed on 2026-07-03: `/home/ubuntu/qintopia-msg-sidecar`
- Server branch observed on 2026-07-03:
  `codex/huabaosi-localization-shadow@b16c247a19ec751c08de75ae2d312f35b765f317`

The local `main` branch is the source for this package contract. The server Huabaosi
shadow branch is a review-pool input until the owner explicitly approves those files as
roadmap.

## Responsibility

The sidecar receives QiWe/Hermes message events from NATS JetStream, persists raw and
normalized records into Postgres, and runs Agent OS background workers. It must stay
independent from the Hermes reply path: sidecar, NATS, Postgres, or embedding failures
must not block webhook ACKs or group replies.

## Package Split

This package owns the service runtime and workers. Related packages are split out so
reviewers can reason about risk:

- `runtime/postgres`: migrations, schema notes, and database runbooks.
- `mcp/context-server`: context and answer-basis MCP surface.
- `mcp/message-store`: message search and evidence lookup MCP surface.
- `workflows/activity-promotion`: Xiaoman, Wenyuange, Huabaosi, and Erhua operations
  control-plane workflow.
- `deploy/sidecar`: systemd, smoke, rollout, and rollback procedures.

## Boundaries

- External sends: no direct group send ownership in this package.
- Database writes: yes. Migrations and workers write Agent OS state.
- Runtime profile: no direct Hermes profile mutation.
- Secrets: uses runtime-only env vars and database URLs; never commit real env files.

## Imported Contents

- Rust crate: `Cargo.toml`, `Cargo.lock`, and `src/`.
- Runtime config templates: `config/agentos/`.
- Replay fixtures: `fixtures/`.
- Safe env template: `.env.example`.
- Source-specific agent rules: `AGENTS.md`.

Migrations are intentionally owned by `runtime/postgres`. The sidecar loads
`../postgres/migrations` by default inside this monorepo. Set
`QINTOPIA_SIDECAR_MIGRATIONS_DIR` to override the path for legacy deployments or local
experiments.

## Validation

Run from the monorepo root:

```bash
pnpm test:sidecar
```

For source-level checks during M5:

```bash
cargo fmt --check --manifest-path runtime/sidecar/Cargo.toml
cargo check --manifest-path runtime/sidecar/Cargo.toml
```

Use smoke scripts under `deploy/sidecar/scripts/` only with the documented environment
and owner approval. Guarded apply smokes can write Postgres state when explicitly
enabled.
