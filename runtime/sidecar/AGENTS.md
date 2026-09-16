# Sidecar instructions

Follow the [root instructions](../../AGENTS.md) and this directory's
[README](README.md), including its operating rules. Source paths in this file are
relative to `runtime/sidecar/` unless explicitly linked elsewhere.

## Map

- Consumer and events: `src/consumer.rs`, `src/event.rs`.
- Persistence: `src/db.rs`; CLI: `src/main.rs`.
- Database migrations and design: [Postgres](../postgres/).
- Capability-specific constraints: the owning package README, found through
  [change routing](../../docs/engineering/change-routing-index.md).
- QiWe transport and send gates: [QiWe](../../skills/qiwe/README.md).
- Artifact profiles, installation and rollback:
  [deploy runner](../../deploy/runner/README.md).

Read the relevant package before changing cross-component behavior. The original
standalone deployment snapshot is historical evidence; use the monorepo Git root.

## Development and validation

Use Rust 1.96.0. Use runtime SQLx queries rather than compile-time query macros so
compilation does not require database access. Keep migrations idempotent and accompany
schema changes with the required versioned data-design note and schema log.

Run from this directory:

```bash
cargo fmt --check
cargo check
RUST_MIN_STACK=33554432 cargo test
```

The larger stack is test-only. Fake transports require loopback socket binding; a denied
bind is an environment failure. From the repository root, use `pnpm check:pr:auto` and
the configured disposable PostgreSQL tier when applicable. Preserve feature-gated test
helpers and warning-denied default/all-feature Clippy coverage.

## Review boundaries

Register capabilities in both registries and smokes. Preserve trusted destination
resolution, row/attempt identity, atomic audit behavior, external-effect ordering and
ambiguous-outcome handling as specified by the owning capability.

Production artifact features, runtime flags, owner approvals and allowlists are separate
gates. All-feature test builds are not production artifacts. Do not weaken apply gates,
transport restrictions or privacy controls to make tests pass. Report fixture results
separately from real production acceptance.
