# Project Instructions

## Map

- Human setup and usage: `README.md`
- Source snapshot: `docs/source-snapshot.md`
- Database migrations: `../postgres/migrations/`
- Versioned data design docs: `../postgres/docs/data-design/`
- Server deployment scripts: `../../deploy/sidecar/scripts/`
- Current cutover runbook: `../../docs/operations/m9-server-cutover-runbook.md`
- Target server directory plan: `../../docs/operations/server-directory-plan.md`
- Legacy standalone deployment snapshot:
  `../../deploy/sidecar/docs/server-deployment.md`
- Sidecar entrypoint: `src/main.rs`
- NATS consumer loop: `src/consumer.rs`
- Postgres persistence: `src/db.rs`
- Event protocol parsing: `src/event.rs`

## Commands

- Format: `cargo fmt`
- Check: `cargo check`
- Test: `cargo test`
- Local readiness: `cargo run -- check`
- Run consumer: `cargo run -- run`

From the monorepo root, prefer:

- Test: `pnpm test:sidecar`
- Full check: `pnpm check`

## Rules

- Keep the sidecar independent from 二花's reply path; NATS, sidecar, or Postgres
  failures must not be able to block Hermes webhook ACK or replies.
- Keep compatibility with the server Rust toolchain: `rustc/cargo 1.75.0`.
- Manage this project through the monorepo root git repository.
- Treat `deploy/sidecar/docs/server-deployment.md` as historical rollback evidence, not
  the current deployment path.
- Do not commit database credentials or server-only env files.
- Use runtime SQLx queries, not compile-time `query!` macros, so builds do not require
  database access.
- Migrations must be idempotent and safe to run on sidecar startup.
- Every database schema migration must have a matching versioned design note under
  `../postgres/docs/data-design/` and must record itself in
  `qintopia_agent_os.schema_change_log` when that table exists.
- v1 only captures raw/normalized messages and creates pending processing jobs;
  embedding and graph extraction must remain separate workers.
- Do not adopt files from the server Huabaosi shadow branch until owner review
  explicitly approves them.
