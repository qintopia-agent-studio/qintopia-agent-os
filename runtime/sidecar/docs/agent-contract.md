# Sidecar engineering contract

These are retained operating constraints, not a new permission grant. Read the sections
relevant to the change before editing. Original conditions and exceptions remain
binding; summaries in AGENTS.md do not replace them.

Backtick paths in root-source rules are repository-relative; paths in Sidecar-source
rules retain their original `runtime/sidecar/` base. New navigation links are relative
to this document.

[Migration inventory](../../../docs/plans/active/agents-guidance/README.md) records the
baseline and source locations.

<a id="root-044"></a>

## Commands — root-044

<!-- preserved-rule: root-044 -->

- Full Rust sidecar tests and `cargo llvm-cov` use loopback fake servers; run them
  outside Codex's restricted sandbox with `RUST_MIN_STACK=33554432` when the sandbox
  reports `bind fake server: Operation not permitted`.

<!-- /preserved-rule: root-044 -->

<a id="root-128"></a>

## Commands — root-128

<!-- preserved-rule: root-128 -->

- Sidecar dependency vulnerability audit:
  `cd runtime/sidecar && cargo deny check advisories bans sources`. A full
  `cargo deny check` currently fails license checks because the repository has no
  `deny.toml` license policy; do not treat that as unresolved RustSec advisories.
  `cargo audit` scans all lockfile entries and may report `rsa` through `sqlx-mysql`
  even when `cargo tree --target all -i rsa` shows it is unreachable from the current
  Postgres-only feature set; record that as a lockfile/audit-tooling follow-up, not as a
  runtime exposure, unless the dependency tree proves it is reachable.

Use `rg` and `rg --files` for search.

<!-- /preserved-rule: root-128 -->

<a id="root-129"></a>

## Core Rules — root-129

<!-- preserved-rule: root-129 -->

- Organize by Agent OS capability, not by programming language.

<!-- /preserved-rule: root-129 -->

<a id="root-130"></a>

## Core Rules — root-130

<!-- preserved-rule: root-130 -->

- Rust, Python, TypeScript, shell, and SQL are implementation details inside a package.

<!-- /preserved-rule: root-130 -->

<a id="root-131"></a>

## Core Rules — root-131

<!-- preserved-rule: root-131 -->

- Do not create top-level `python/`, `rust/`, `typescript/`, or similar language
  buckets.

<!-- /preserved-rule: root-131 -->

<a id="root-268"></a>

## Core Rules — root-268

<!-- preserved-rule: root-268 -->

- `operations-work-item-status` must resolve nested work items to the top-level workflow
  root and report every descendant while preserving each direct `parent_work_item_id`.
  `operations-workflow-sync` may persist that recursive AgentOS summary, but neither
  command may execute workers, schedule a general DAG, call external adapters, or send.

<!-- /preserved-rule: root-268 -->

<a id="sidecar-002"></a>

## Commands — sidecar-002

<!-- preserved-rule: sidecar-002 -->

- Format: `cargo fmt`

<!-- /preserved-rule: sidecar-002 -->

<a id="sidecar-003"></a>

## Commands — sidecar-003

<!-- preserved-rule: sidecar-003 -->

- Check: `cargo check`

<!-- /preserved-rule: sidecar-003 -->

<a id="sidecar-004"></a>

## Commands — sidecar-004

<!-- preserved-rule: sidecar-004 -->

- Test: `RUST_MIN_STACK=33554432 cargo test`

<!-- /preserved-rule: sidecar-004 -->

<a id="sidecar-005"></a>

## Commands — sidecar-005

<!-- preserved-rule: sidecar-005 -->

- Local readiness: `cargo run -- check`

<!-- /preserved-rule: sidecar-005 -->

<a id="sidecar-006"></a>

## Commands — sidecar-006

<!-- preserved-rule: sidecar-006 -->

- Run consumer: `cargo run -- run`

<!-- /preserved-rule: sidecar-006 -->

<a id="sidecar-013"></a>

## Commands — sidecar-013

<!-- preserved-rule: sidecar-013 -->

- Test: `pnpm test:sidecar`

<!-- /preserved-rule: sidecar-013 -->

<a id="sidecar-014"></a>

## Commands — sidecar-014

<!-- preserved-rule: sidecar-014 -->

- Full check: `pnpm check`

<!-- /preserved-rule: sidecar-014 -->

<a id="sidecar-018"></a>

## Rules — sidecar-018

<!-- preserved-rule: sidecar-018 -->

- Keep compatibility with the supported Rust toolchain: `rustc/cargo 1.96.0`.

<!-- /preserved-rule: sidecar-018 -->

<a id="sidecar-019"></a>

## Rules — sidecar-019

<!-- preserved-rule: sidecar-019 -->

- Manage this project through the monorepo root git repository.

<!-- /preserved-rule: sidecar-019 -->

<a id="sidecar-021"></a>

## Rules — sidecar-021

<!-- preserved-rule: sidecar-021 -->

- Do not commit database credentials or server-only env files.

<!-- /preserved-rule: sidecar-021 -->

<a id="sidecar-022"></a>

## Rules — sidecar-022

<!-- preserved-rule: sidecar-022 -->

- Use runtime SQLx queries, not compile-time `query!` macros, so builds do not require
  database access.

<!-- /preserved-rule: sidecar-022 -->

<a id="sidecar-023"></a>

## Rules — sidecar-023

<!-- preserved-rule: sidecar-023 -->

- Migrations must be idempotent and safe to run on sidecar startup.

<!-- /preserved-rule: sidecar-023 -->

<a id="sidecar-024"></a>

## Rules — sidecar-024

<!-- preserved-rule: sidecar-024 -->

- Every database schema migration must have a matching versioned design note under
  `../postgres/docs/data-design/` and must record itself in
  `qintopia_agent_os.schema_change_log` when that table exists.

<!-- /preserved-rule: sidecar-024 -->

<a id="sidecar-027"></a>

## Rules — sidecar-027

<!-- preserved-rule: sidecar-027 -->

- Work-item creation events must not mirror full request metadata. If a capability needs
  metadata in `work_item_events.data`, add an explicit allowlist that emits only stable,
  non-secret audit fields; `work_items.metadata` may retain broader internal context
  after normal sensitivity validation.

<!-- /preserved-rule: sidecar-027 -->

<a id="sidecar-029"></a>

## Rules — sidecar-029

<!-- preserved-rule: sidecar-029 -->

- The complete suite includes fake provider/media tests that bind ephemeral loopback
  sockets. In restricted coding sandboxes, run the same `cargo test` command with
  loopback-bind permission; `PermissionDenied` from `TcpListener::bind` is an
  environment failure and must be confirmed by an unsandboxed rerun, not hidden by
  skipping tests.

<!-- /preserved-rule: sidecar-029 -->

<a id="sidecar-030"></a>

## Rules — sidecar-030

<!-- preserved-rule: sidecar-030 -->

- Test helpers used only by a non-default adapter feature must carry the same feature
  gate on their imports, types, and implementations; default-feature Clippy compiles
  test targets and rejects otherwise-unused helpers.

<!-- /preserved-rule: sidecar-030 -->

<a id="sidecar-039"></a>

## Rules — sidecar-039

<!-- preserved-rule: sidecar-039 -->

- The disposable operations smoke may enter the live retry path only with both the
  Huabaosi and PostgreSQL integration features, its explicit apply-smoke flag, exact
  literal-loopback `qintopia_test` URL hash, and literal-loopback-only provider/media
  configuration.

<!-- /preserved-rule: sidecar-039 -->

<a id="sidecar-041"></a>

## Rules — sidecar-041

<!-- preserved-rule: sidecar-041 -->

- v1 only captures raw/normalized messages and creates pending processing jobs;
  embedding and graph extraction must remain separate workers.

<!-- /preserved-rule: sidecar-041 -->

<a id="sidecar-043"></a>

## Rules — sidecar-043

<!-- preserved-rule: sidecar-043 -->

- Callback credential-shape reports may expose only one fixed reviewed schema id and an
  additional-field count. Reject canonical and alias spellings that appear together;
  never report request ids, credential values, filenames, MD5 values, unknown field
  names, or unknown values.

<!-- /preserved-rule: sidecar-043 -->

<a id="sidecar-046"></a>

## Rules — sidecar-046

<!-- preserved-rule: sidecar-046 -->

- Persist an `uploading` attempt in the same transaction that claims the work item,
  before any external socket can open. Expired `uploading` attempts and legacy claims
  with no attempt row are unknown external outcomes: terminalize them as `ambiguous`
  with automatic retry disabled. Worker previews must reuse the exact apply-side group
  and media-host allowlists.

<!-- /preserved-rule: sidecar-046 -->

<a id="sidecar-053"></a>

## Rules — sidecar-053

<!-- preserved-rule: sidecar-053 -->

- `space_agent_turn_result` artifacts are inert data. A future consumer must derive
  destinations and capability arguments from the exact work item's trusted `space_id`
  and the live capability registry; never interpret result property names or values as a
  room, target, URL, HTTP request, executable input, or tool invocation.

<!-- /preserved-rule: sidecar-053 -->

<a id="sidecar-054"></a>

## Rules — sidecar-054

<!-- preserved-rule: sidecar-054 -->

- Do not adopt files from the server Huabaosi shadow branch until owner review
  explicitly approves them.

<!-- /preserved-rule: sidecar-054 -->
