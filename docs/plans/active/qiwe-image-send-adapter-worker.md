# QiWe Image Send Adapter Worker

Status: worker and state machine deployed in the default-feature `v0.2.9` artifact;
guarded staging smoke in progress; production disabled

Updated: 2026-07-15

## Goal

Connect the reviewed QiWe request builders and Postgres state machine through a guarded
Rust worker without enabling production sending:

```text
approved generated_image + send-ready work item
  -> run-qiwe-image-send-worker --once --apply
  -> POST /cloud/cdnUploadByUrlAsync
  -> persist requestId hash as awaiting_callback
  -> bounded dedicated callback input
  -> commit sending
  -> POST /msg/sendImage once
  -> sent | failed | ambiguous
```

This is the first staging-only executable adapter path. Default and production builds
exclude the live adapter at compile time; it is not production scheduling or approval to
contact QiWe.

## Commands

`qiwe-image-send-staging-preflight` validates the staging-only compile feature, exact
owner phrase, enablement, webhook readiness, API/media/group allowlists, and the exact
staging database URL hash from the owner-reviewed one-shot command. It does not connect
to Postgres, read a callback, or open a network connection.

`run-qiwe-image-send-worker` supports `--once`, `--work-item-id`, `--apply`, and
`--dry-run`.

- Dry-run reads one eligible AgentOS work item and emits a sanitized preview. It does
  not claim, write Postgres, or open a network connection. Preview and apply share the
  exact target-group, media-host, JPEG hash, MD5, byte-size, and filename validation, so
  missing policy and ineligible candidates fail closed instead of being reported.
- A default build rejects apply as `live_adapter_not_compiled` before reading adapter
  configuration, connecting to Postgres, claiming work, or opening a socket. Callback
  apply also rejects before reading stdin, so a build without a reviewed live adapter
  does not ingest callback credentials for an unavailable capability.
- A staging-only build additionally requires the non-default `qiwe-staging-adapter`
  Cargo feature, `QINTOPIA_QIWE_IMAGE_SEND_ENABLED=1`, a valid reviewed adapter config,
  `QINTOPIA_QIWE_IMAGE_SEND_WEBHOOK_READY=1`, and the exact one-shot owner approval
  phrase `QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL=approved-staging-qiwe-image-send`.
  Missing approval fails before adapter configuration. Upload validates the remaining
  configuration before connecting to Postgres; callback apply validates it before
  reading stdin.
- Apply claims exactly one work item and creates one `uploading` attempt before opening
  a socket. A successful asynchronous URL-upload response updates that same attempt and
  persists only the request id hash.
- An upload transport or provider failure records a sanitized internal failure and must
  not leave the work item permanently processing.

`process-qiwe-image-send-callback` accepts callback JSON only from bounded stdin. Raw
callback JSON must never be passed as a CLI argument, environment variable, log field,
report field, NATS payload, or persisted work-item field.

- Dry-run validates one `cmd=20000` callback shape and emits only fixed presence flags.
- A default build validates the bounded callback shape but rejects apply before Postgres
  correlation or network access. A staging-only build requires the same compile-time
  feature, explicit enablement, and reviewed adapter config.
- Apply correlates by raw request id in memory, commits `sending`, then builds and sends
  one `/msg/sendImage` request with memory-only file credentials. Before committing the
  send gate, callback filename, canonical MD5, and byte size must exactly match the
  approved final JPEG identity snapshotted at upload acceptance.
- A failure proven to occur before the request leaves the process records `failed`.
  After the send gate, every non-success HTTP or business response records `ambiguous`
  unless a separately reviewed failure-code allowlist can prove no send occurred;
  explicit success records `sent`.
- Duplicate or expired callbacks never reopen the send gate.

## HTTP Boundary

Both Huabaosi and QiWe adapters must use one shared bounded Rust HTTP client. The client
must validate header names and values before connecting, require HTTPS in production,
cap response headers and bodies while reading, bound chunked decoding, set read/write
timeouts, and never include response bodies or endpoint values in errors or reports.

Test-only clients may use loopback HTTP. Production configuration must continue to
require the reviewed HTTPS QiWe API path and exact host allowlist.

## Recovery And Idempotency

- Every new claim persists `uploading` before external I/O. If its claim expires before
  acceptance is durably correlated, the upload outcome is `ambiguous`, the work item
  fails, and automatic retry is forbidden.
- A legacy expired claim with no attempt row is also terminalized as ambiguous because
  AgentOS cannot prove that older code did not cross the external boundary.
- An `awaiting_callback` attempt with no callback is expired and requeued by the next
  claim scan, as defined by the merged state-machine contract.
- A crash, HTTP failure, provider non-success, or uncertain transport after `sending`
  cannot be retried. A later guarded reconciliation marks the attempt `ambiguous` for
  human review.
- No automatic recovery path may convert `sending` back to queued.

## Safe Reports

Worker reports may include AgentOS work-item UUIDs, fixed action statuses, counts, and
booleans. They must use `safe_for_chat=false` and must not include:

- QiWe token or device id;
- target group id;
- media URI;
- raw request, callback, file, or provider message ids;
- callback credentials;
- response bodies; or
- database URLs.

## Test Plan

- Rust unit tests for default-build compile-gate rejection before database/network
  access, disabled/config-invalid/dry-run behavior, and sanitized reports.
- Warning-denied Clippy runs separately with no default features and with all features,
  proving both the production compile gate and the isolated live path remain valid.
- Local fake QiWe server tests for upload acceptance, callback credentials, successful
  send, non-success HTTP/business ambiguity, oversized response, connection timeout, and
  header injection rejection.
- Disposable PostgreSQL integration proving pre-network attempt persistence, upload
  acceptance on the same attempt, duplicate callback, exact one send, terminal stale
  upload/send behavior, legacy unrecorded-claim terminalization, preview allowlist
  parity, missing callback recovery, callback file identity mismatch rejection, and no
  raw credentials or identifiers in state/events/reports.
- Full Rust suite, Clippy with warnings denied, repository pre-commit, CI contracts,
  secret scan, and diff check.

## Local Validation Evidence

- Default Rust suite: 368 passed with `RUST_MIN_STACK=33554432`.
- All-feature nextest: 364 passed; 8 guarded PostgreSQL tests skipped because no
  disposable `qintopia_test` database was configured.
- Default and all-feature Clippy passed with warnings denied. Default and all-feature
  staging-gate tests each passed 3 tests.
- Rust line coverage was 46.66% overall, up from the 46.53% baseline;
  `qiwe_image_send.rs` reached 85.50% line coverage.
- Fake two-phase staging smoke, deploy contracts, deploy runner contracts, sidecar
  smokes, systemd rendering, pre-commit, format, Markdown, registry, MCP, skills,
  workflow, runtime, CI, policy, secret, and release-model checks passed. The QiWe
  parser suite passed 159 tests; the weather, knowledge-retrieval, and operations-intake
  suites passed 18, 10, and 7 tests.
- `pnpm check` could not start because the repository pnpm shim could not verify the
  `pnpm@10.29.2` registry signature. Per repository policy, `pmOnFail=ignore` was not
  used; the fixed repository-local Node, Python, shell, and Rust entrypoints were run
  directly instead.
- No validation command contacted a real QiWe endpoint, connected to staging or
  production Postgres, wrote Feishu, changed systemd, or modified production runtime.

## Production Boundary

Production image-send observation may inspect the immutable release/current artifact and
confirm send remains disabled, but it must not install a worker service/timer, compile a
QiWe live adapter into the production artifact, install a callback listener, broaden
group allowlists, pass database/QiWe secrets to observation children, or contact QiWe.
Production artifact builders are checked to exclude `qiwe-staging-adapter` and
all-features builds; their manifest records exactly
`cargo_features: [huabaosi-production-adapter, huabaosi-feishu-mirror-adapter]`.
Rollback keeps `QINTOPIA_QIWE_IMAGE_SEND_ENABLED=0` and QiWe image-send production units
absent.
