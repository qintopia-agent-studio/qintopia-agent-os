# Sidecar runtime instructions

<!-- guidance-scope: runtime/sidecar -->

These instructions supplement the [root contract](../../AGENTS.md) for the Rust
consumer, persistence, capability registry and governed workers.

## Map and required reading

- [Package guide](README.md) and [source snapshot](docs/source-snapshot.md).
- [Complete engineering constraints](docs/agent-contract.md).
- [Database migrations](../postgres/migrations/).
- [Versioned data design](../postgres/docs/data-design/).
- [Deployment rules](../../deploy/AGENTS.md).
- [QiWe rules](../../skills/qiwe/AGENTS.md).
- [Change routing](../../docs/engineering/change-routing-index.md).

Start from the changed capability and its owning topic, not a file's language. Read QiWe
or Huabaosi topic contracts when working on those adapters; root and sibling summaries
cannot replace their complete conditions and exceptions.

The consumer, database and event protocol entrypoints remain `src/consumer.rs`,
`src/db.rs` and `src/event.rs`; the CLI enters through `src/main.rs`.

Treat the old standalone deployment snapshot as historical rollback evidence, not the
current installation path. Manage changes through the monorepo Git root.

## Validation commands

From the repository root:

- `pnpm test:sidecar`
- `pnpm check:pr:auto`
- `pnpm check:pr:heavy`

From this directory:

- `cargo fmt --check`
- `cargo check`
- `RUST_MIN_STACK=33554432 cargo test`

Use the supported Rust 1.96.0 toolchain. The complete test suite needs a 32 MiB thread
stack; this is test-only, not a production environment setting.

Fake provider/media tests bind ephemeral loopback sockets. A denied local bind is an
environment failure, not permission to skip tests or treat them as passed.

Test-only helpers must share the feature gates of their callers. Preserve CI's
all-feature fixture coverage and both default/all-feature warning-denied Clippy;
all-features CI builds are not deployable production artifacts.

## Persistence and capabilities

Use runtime SQLx queries rather than compile-time query macros so compilation does not
require database access. Never commit database credentials or server env files.

Migrations must remain idempotent and safe at startup. Each schema migration needs its
versioned data-design note and schema-change-log entry when the log exists.

Register new built-in capabilities in both registry surfaces and capability-list smokes.
Update downstream selectors only when the capability is intended to take part in those
workflows; preserve their explicit authorization boundaries.

State changes must retain row locking, exact claim/attempt identity and atomic audit
behavior. Release all claim fields together where required. Lost leases do not establish
that external effects stayed local or make automatic retry safe.

Audit events use explicit non-secret allowlists. Broader internal metadata is not
permission to mirror complete request payloads into event history or reports.

Treat Agent turn results as inert data. Resolve destinations and capabilities from
trusted work-item/registry context, never from arbitrary result properties.

## Transport and external effects

Ordinary Hermes webhook acknowledgements and replies must not depend on Sidecar, NATS or
Postgres. Preserve the narrowly specified authenticated system-event PubAck exception in
the QiWe channel contract.

Authenticated ingress comes from the actual trusted subject, not publisher JSON. Keep
producer/consumer credentials, subjects and permissions distinct.

External adapters must use the shared bounded HTTP client, approved HTTPS hosts and
reviewed allowlists. Do not introduce another raw socket implementation. Test-only
loopback providers do not relax production transport policy.

Record upload/send attempts before network effects. Preserve terminal ambiguous outcomes
and no-retry boundaries for expired processing, uploading and sending. A missing
response cannot establish that the provider did nothing.

## Artifact and apply boundaries

Main production artifacts retain the reviewed Huabaosi production, guarded Feishu mirror
and default-disabled Xiaoman poster features. The QiWe production adapter is a separate
companion artifact, not another feature in the main runtime.

Only exact reviewed staging/production feature combinations may enter their respective
live paths. Defaults fail closed. A runtime enable flag does not replace a compile gate,
owner approval, database binding or target allowlist.

Apply must enforce the owning capability's checks before database, stdin or external
access as specified in its complete contract. A shell-only check cannot replace
validation in the runtime itself.

Keep approvals tied to exact artifact type/status, content identity and row lock. A
canary review cannot approve an unrelated generated artifact through a different
workflow. Dry-run must preserve the apply-side policy checks it is specified to share.

Observations discover the immutable reviewed binary. Do not fall back to source
`cargo run`, read arbitrary environment values or expose credentials for convenience.

## Evidence and completion

Keep provider bodies, callback credentials, filenames, raw identities and private
messages out of retained evidence. Follow each capability's fixed output schema.

Preview-only migration commands stay preview-only: no database writes, generated
artifacts, external API calls or sends. An apply flag is not a harmless extension.

Do not adopt server shadow branches without explicit owner review. Keep pending
production acceptance and historical restrictions visible through their owning runbooks
and reports; a fixture pass is not live delivery acceptance.
