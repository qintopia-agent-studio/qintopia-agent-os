# QiWe Image Send Adapter Worker

Status: implemented locally; CI and review pending; production disabled

Updated: 2026-07-14

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

`run-qiwe-image-send-worker` supports `--once`, `--work-item-id`, `--apply`, and
`--dry-run`.

- Dry-run reads one eligible AgentOS work item and emits a sanitized preview. It does
  not claim, write Postgres, or open a network connection.
- A default build rejects apply as `staging_adapter_not_compiled` before reading adapter
  configuration, connecting to Postgres, claiming work, or opening a socket.
- A staging-only build additionally requires the non-default `qiwe-staging-adapter`
  Cargo feature, `QINTOPIA_QIWE_IMAGE_SEND_ENABLED=1`, a valid reviewed adapter config,
  and `QINTOPIA_QIWE_IMAGE_SEND_WEBHOOK_READY=1`.
- Apply claims exactly one work item, sends one asynchronous URL-upload request, and
  persists only the request id hash through `qiwe_image_send_attempts`.
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
  one `/msg/sendImage` request with memory-only file credentials.
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

- A crash before upload acceptance is persisted may leave an expired processing claim
  but no active attempt. The next upload-worker claim transaction must safely requeue
  that stale unrecorded claim.
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
- Disposable PostgreSQL integration proving upload acceptance, duplicate callback, exact
  one send, terminal state, stale claim, missing callback recovery, and no raw
  credentials or identifiers in state/events/reports.
- Full Rust suite, Clippy with warnings denied, repository pre-commit, CI contracts,
  secret scan, and diff check.

## Production Boundary

This change must not install a service or timer, modify production runtime
configuration, write Feishu, or contact a real QiWe endpoint. Production artifact and
server-source builders use default Cargo features and are checked to exclude
`qiwe-staging-adapter`; their manifest records `cargo_features: []`. Setting runtime
environment variables cannot add the missing executable path. Rollback keeps default
builds, `QINTOPIA_QIWE_IMAGE_SEND_ENABLED=0`, and both commands unscheduled.
Owner-approved staging callback evidence and an isolated test group remain required
before a staging-feature build may be used, and production enablement remains a later
separate decision.
