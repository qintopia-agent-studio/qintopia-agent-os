# Xiaoman QiWe Callback Ingress Gap

Date: 2026-07-15

## Current State

The guarded Rust QiWe adapter can submit one asynchronous URL upload and process one
bounded `cmd=20000` callback from stdin. PR #137 added the reviewed two-phase staging
smoke, exact staging database URL hash gate, and stdin-only callback phase. The existing
QiWe webhook sanitizes callback credentials before NATS publication, as required, but no
reviewed component passes the original callback directly from webhook memory to the Rust
processor.

The upload therefore cannot reach `/msg/sendImage` through the reviewed path. Sanitized
NATS or Postgres callback records cannot be reused because the required file credentials
have already been removed.

## Resolution

Add a disabled-by-default bridge inside the existing QiWe Hermes webhook boundary. It
recognizes the async callback shape, keeps the original bounded body in memory, and
starts one explicitly configured staging sidecar executable with fixed
`process-qiwe-image-send-callback --apply` arguments. The callback is written only to
the child stdin. Before that write, the bridge requires the exact owner approval phrase,
canonical approved staging database URL hash, explicit send and webhook readiness, and
an executable staging sidecar path. It discards stderr, bounds and validates stdout
against the existing sanitized Rust report schema, applies a hard timeout, and never
includes raw callback data or subprocess output in logs or HTTP responses. An exact
enable request with invalid staging configuration remains visibly enabled but
configuration-invalid, so callback handling returns HTTP 503 without starting a child.
It cannot silently downgrade to disabled and acknowledge an unprocessed callback.
Callback detection requires the reviewed top-level QiWe success envelope, a bounded
event list, a request id, a `msgData` object, and complete credential-field presence. It
catches excessive JSON decoder depth and never treats an arbitrary nested `cmd=20000`
value as an image callback. The child environment is rebuilt from a fixed 12-name
allowlist containing only the sidecar staging database boundary, QiWe adapter
configuration, owner gates, and exact host/group allowlists. Hermes webhook secrets,
NATS, Feishu, proxy, and unrelated runtime variables are not inherited. Any explicit
enable value outside `0` or `1` becomes configuration-invalid and follows the same HTTP
503 path. The executable trust boundary is fixed to
`/home/ubuntu/qintopia-agent-os-staging-releases/<40-hex-sha>/sidecar/qintopia-message-sidecar`.
The configured release root must match that exact immutable layout, every checked path
component must be owned by root or the gateway effective user and reject group/world
write bits, and no checked component may be a symlink. The owner-approved executable
SHA-256 is verified while building the bridge and immediately before spawn. A
staging-like writable path under `/tmp` therefore cannot receive the callback or child
environment even if its basename is correct and executable.

This PR does not add a second staging smoke. The already-merged
`qiwe-image-send-staging-smoke.sh` remains the only reviewed one-shot operator
entrypoint for explicit upload and callback phases.

## Validation

- Python unit tests cover callback detection, disabled behavior, executable-path
  validation, fixed subprocess arguments, sanitized report validation, timeout, bounded
  stdout, callback-envelope rejection, invalid-enabled HTTP 503 behavior, exact child
  environment allowlisting, and webhook routing without ordinary Agent dispatch.
- The existing staging smoke test uses a fake sidecar and synthetic callbacks. It does
  not contact QiWe, Postgres, Feishu, an image provider, or a media service.
- Existing Rust fake-server and disposable PostgreSQL coverage remain authoritative for
  upload, callback correlation, exact JPEG identity, send idempotency, and ambiguous
  outcomes.

Local results:

- focused callback bridge and webhook routing tests passed `12/12`;
- the complete QiWe package suite passed `171/171`;
- the existing fake-sidecar staging smoke, deploy contracts, deploy preflight, deploy
  runner, anti-drift, secret, runtime, and CI contract checks passed;
- the sidecar default suite passed `368/368`; the all-feature suite passed `365/365`
  with nine guarded PostgreSQL tests skipped by design;
- warning-denied Clippy passed with no default features and with all features; and
- Cargo deny passed advisories, bans, and sources with only the existing duplicate
  dependency warnings.

The `pnpm test:qiwe` attempt stopped because the pnpm version shim could not verify the
registry signature for `pnpm@10.29.2`. No bypass flag was used. The exact fixed
repository-local command behind that script,
`PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tests -v`, was run directly
from `skills/qiwe` and passed `169/169`.

The exact-head `pnpm format:check` attempt stopped on the same signature validation. The
fixed local entrypoint, `./node_modules/.bin/prettier --check .`, was run directly
without a bypass and passed.

## Production Boundary

This change does not compile the live adapter into a production sidecar, enable the
bridge by default, install a service or timer, change nginx, call QiWe, write production
Postgres, generate an image, write Feishu, or send externally. A real smoke still
requires a separately built staging-feature sidecar, isolated group and database,
reviewed secrets and allowlists, the approved database URL hash, and the exact one-shot
approval phrase. Production scheduling remains blocked until that staging evidence and
rollback decision are reviewed in a later PR.
