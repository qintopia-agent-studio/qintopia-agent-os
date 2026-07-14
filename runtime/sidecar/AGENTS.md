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
- Test: `RUST_MIN_STACK=33554432 cargo test`
- Local readiness: `cargo run -- check`
- Run consumer: `cargo run -- run`
- Huabaosi WeCom shadow capture fixture tests: `cargo test huabaosi_wecom_shadow`
- Huabaosi WeCom policy preview fixture tests: `cargo test huabaosi_wecom_policy`
- Huabaosi WeCom canary gateway fixture tests: `cargo test huabaosi_wecom_canary`

From the monorepo root, prefer:

- Test: `pnpm test:sidecar`
- Full check: `pnpm check`

## Rules

- Keep the sidecar independent from 二花's reply path; NATS, sidecar, or Postgres
  failures must not be able to block Hermes webhook ACK or replies.
- Keep compatibility with the supported Rust toolchain: `rustc/cargo 1.96.0`.
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
- The complete sidecar suite needs a 32 MiB test-thread stack. `pnpm test:sidecar` and
  CI set `RUST_MIN_STACK=33554432`; this is test-only and must not be copied into the
  production sidecar service environment.
- The complete suite includes fake provider/media tests that bind ephemeral loopback
  sockets. In restricted coding sandboxes, run the same `cargo test` command with
  loopback-bind permission; `PermissionDenied` from `TcpListener::bind` is an
  environment failure and must be confirmed by an unsandboxed rerun, not hidden by
  skipping tests.
- v1 only captures raw/normalized messages and creates pending processing jobs;
  embedding and graph extraction must remain separate workers.
- Sanitize QiWe asynchronous `cmd=20000` callback credentials before raw-event
  persistence. Dead letters may keep only payload length and digest; malformed payloads
  must not become a bypass that stores callback credentials or raw private text. Only
  preserve callback event/message ids matching `qiwe-callback:<64 hex SHA-256>`; hash
  the complete id again when a prefixed value has any other suffix.
- QiWe image-send state transitions must lock both the work item and attempt, recheck
  the same unexpired claim plus approved artifact/target/final-confirmation facts, and
  store only canonical hashes. The `sending` transition is the at-most-once boundary;
  crashes or transport uncertainty after it require `ambiguous` human reconciliation,
  never an automatic retry with callback credentials. A non-2xx or non-success business
  response after the request may have been sent is also ambiguous without a reviewed
  no-send failure-code allowlist. Treat QiWe target group ids as opaque and
  case-sensitive, and match their allowlist exactly. An ambiguous send audit must use
  `external_send_executed=null` and outcome `unknown`, never a definite false. Late
  callbacks must atomically expire the awaiting attempt and requeue the same work item
  before returning. After the send gate commits, terminal writes must still require the
  exact attempt and claim token but must not fail only because its short TTL elapsed.
  Before selecting new work, the claim transaction must expire and requeue a stale
  `awaiting_callback` attempt even when no callback ever arrives; never apply that
  timeout retry path to `sending`.
- Persist an `uploading` attempt in the same transaction that claims the work item,
  before any external socket can open. Expired `uploading` attempts and legacy claims
  with no attempt row are unknown external outcomes: terminalize them as `ambiguous`
  with automatic retry disabled. Worker previews must reuse the exact apply-side group
  and media-host allowlists.
- The QiWe upload worker and callback processor remain unscheduled. Their live helpers
  compile only with the non-default `qiwe-staging-adapter` feature; default/production
  apply must return `staging_adapter_not_compiled` before Postgres or network access,
  and callback apply must do so before reading stdin. Runtime env flags are not a
  substitute for this compile gate. Callback JSON is accepted from bounded stdin only,
  never CLI arguments or environment variables. File credentials may open the send gate
  only when callback filename, canonical MD5, and byte size exactly match the approved
  final JPEG identity snapshotted at upload. Callback credentials, request ids, media
  URLs, target groups, tokens, device ids, response bodies, and provider message ids
  must not appear in reports or logs; sensitive in-memory buffers must be zeroized on
  drop.
- A staging-feature callback apply must validate explicit enablement, API/media/group
  allowlists, and webhook readiness before reading stdin. Upload apply must validate the
  same adapter configuration before connecting to Postgres.
- A staging-feature QiWe apply must require
  `QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL=approved-staging-qiwe-image-send` before
  adapter configuration, stdin, Postgres, or network access. The Cargo feature, enable
  flag, secrets, and allowlists do not substitute for this owner-reviewed one-shot gate.
- CI must run warning-denied Clippy once with no default features and once with all
  features. The all-feature build type-checks staging code but cannot stand in for the
  production feature set.
- QiWe upload dry-run must use the same exact group/media allowlists and approved JPEG
  identity validator as apply. It may skip locks and writes, but not policy checks.
- External adapter modules must use `bounded_http`; do not add another raw socket HTTP
  implementation. Test-only loopback HTTP is allowed, while production clients require
  HTTPS and the reviewed endpoint/host allowlists.
- Do not adopt files from the server Huabaosi shadow branch until owner review
  explicitly approves them.
- `huabaosi-wecom-shadow-capture` is a preview-only migration command. It may read one
  event from bounded stdin and emit only sanitized hashes, byte counts, field presence,
  classification, and fixed guardrails. It must not gain an apply mode, connect to
  Postgres or external services, send WeCom/QiWe messages, generate or upload media,
  write Feishu, create artifacts, or print raw ids, user text, media URLs, filenames,
  tokens, or callback credentials.
- `huabaosi-wecom-policy-preview` is a preview-only migration command. It may read one
  event from bounded stdin and emit only sanitized policy classifications, fixed
  fallback copy, and hash-based idempotency metadata. It must not gain an apply mode,
  connect to Postgres or external services, send WeCom/QiWe messages, generate or upload
  media, write Feishu, create artifacts, or print raw ids, user text, media URLs,
  filenames, tokens, or callback credentials. Internal-process suppression must use
  narrow full-template matches with negative fixture coverage for ordinary user text
  containing terms such as `plain text`.
- `huabaosi-wecom-canary-preflight` is a local configuration preflight only. It must not
  read stdin, open network or database connections, source env files, or emit
  endpoint/token/id values. `huabaosi-wecom-canary-gateway --apply` is staging-only,
  requires the non-default `huabaosi-wecom-canary-gateway` Cargo feature plus explicit
  enablement, approval phrase, HTTPS endpoint, token, and exact Bot/chat/user
  allowlists, and must remain unscheduled. Default builds must fail closed before stdin,
  network, database, or send access. It must not change production routing, run image
  generation, upload media, write Feishu/Postgres, or send outside the allowlist.
