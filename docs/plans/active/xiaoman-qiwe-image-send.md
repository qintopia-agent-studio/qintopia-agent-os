# Xiaoman QiWe Image Send

Updated: 2026-07-18

## Goal

Complete the final external boundary after an approved Xiaoman `generated_image` and an
internally recorded group-message send-ready decision. The adapter must upload the
reviewed image through the documented QiWe media protocol, send it to one allowlisted
group, and record a durable sanitized result without treating QiWe as the system source
of truth.

## Official Protocol Evidence

The QiWe Open Platform documentation reviewed on 2026-07-14 defines:

- [asynchronous URL upload](https://doc.qiweapi.com/api-347221662):
  `/cloud/cdnUploadByUrlAsync` accepts `guid`, `filename`, `fileUrl`, and `fileType=1`,
  then returns a `requestId`;
- [Webhook callback structure](https://doc.qiweapi.com/doc-7331304): asynchronous API
  results use `cmd=20000` and correlate through the same `requestId`; and
- [send image](https://doc.qiweapi.com/api-344613915): `/msg/sendImage` requires
  `fileAesKey`, `fileId`, `fileMd5`, `fileSize`, `filename`, and the target `toId`.

The synchronous local and URL upload APIs return file credentials directly, but both are
marked for deprecation. New AgentOS work must not make either deprecated API the
production foundation.

The official non-deprecated SDK temporary-storage upload uses
`POST /qiwe/api/qw/doFileApi` with multipart `method=/cloud/cloudUpload`, `guid`, and
`file`, then returns `data.cloudUrl`. Feishu-backed delivery may use that URL only in
memory after exact-host allowlisting and same-byte readback; it remains an input to the
non-deprecated asynchronous URL upload rather than a stable AgentOS artifact URI.

## Final Artifact Format Decision

The official send-image page names JPG as the supported image format and asynchronous
upload uses `fileType=1` for JPG. The provider source remains a fully decoded 1024x1024
PNG, but Huabaosi deterministically composites alpha over white and encodes a quality-92
JPEG before media upload. Only the exact read-back JPEG bytes become the pending
`generated_image` reviewed by humans and referenced by QiWe. The artifact records the
source PNG hash, final JPEG hash, and fixed transform identity; renaming PNG bytes to
`.jpg` remains forbidden.

This resolves the code-level format gap. It does not approve external generation or
sending: owner-approved staging must still prove the provider, isolated media storage,
JPEG upload/readback, and QiWe callback behavior together.

The async callback documentation shows correlation but its public example contains only
`cloudUrl`. The send step remains blocked unless a staging callback proves that complete
file credentials are returned. Missing or ambiguous credentials must never fall back to
the deprecated synchronous API.

## State Machine

```text
approved generated_image + internal send-ready
  -> validate immutable HTTPS URI or exact Feishu primary-storage URI
  -> for Feishu: authenticated readback -> temporary upload -> same-byte URL readback
  -> validate target allowlist, final confirmation, and format
  -> POST /cloud/cdnUploadByUrlAsync
  -> persist sanitized requestId correlation and wait
  -> ingest exactly one cmd=20000 callback with matching requestId
  -> validate complete file credentials without exposing them
  -> POST /msg/sendImage once
  -> persist send_executed or sanitized terminal failure
```

Postgres remains the fact source. Callback delivery, retries, and worker restarts must
be idempotent. A stale work-item claim, duplicate callback, changed artifact, changed
target, or missing final confirmation must stop before sending.

## Implemented First Boundary

- Rust request builders match the documented async upload and send-image shapes.
- Response parsers cap JSON bodies before parsing and reject non-zero API status.
- Callback parsing requires exactly one matching `cmd=20000` event and complete file
  credentials.
- Send request construction requires the target group in the reviewed allowlist. Send
  response parsing requires both `code=0` and `isSendSuccess=1`, matching the existing
  QiWe rich-message adapter fixtures; every other value fails closed.
- Header and protocol values reject control characters, endpoints require the reviewed
  HTTPS path, media URLs reject credentials/query/fragment and non-allowlisted hosts,
  and JPEG MIME/JPG naming are enforced.
- The Huabaosi final-artifact path converts provider PNG to the immutable reviewed JPEG
  while preserving source and final hashes plus the fixed transform metadata.
- `qiwe-image-send-preflight` checks only local configuration and emits a sanitized
  report. It opens no network or database connection. Huabaosi production release
  artifacts must not compile `qiwe-production-adapter`; release-local production
  observation keeps secrets out of shell children and checks the immutable manifest plus
  disabled runtime state.
- Preflight `missing_configuration` may list only fixed public variable names from
  `.env.example`, never values, URLs, hosts, group ids, or enable flags. An empty list
  with `config_valid=false` means present configuration failed format, readiness, or
  allowlist validation and must still fail closed.
- `QINTOPIA_QIWE_IMAGE_SEND_ENABLED` defaults to `0`; guarded upload/callback commands
  exist, but default binaries fail apply before Postgres or network access even if the
  runtime enable flag is misconfigured. Staging and production feature builds both
  require their exact owner approval phrase, database URL hash, Feishu delivery config,
  webhook readiness, and allowlists before Postgres, callback stdin, or network access.
  If both QiWe live features are present in a test or accidental build, apply chooses
  the production gate and never falls back to staging approval or staging database
  hashing. Production observation installs no worker service/timer and does not install
  a production callback listener.
- The QiWe capture producer sanitizes any `cmd=20000` event before NATS publication, and
  the Rust sidecar independently repeats the boundary before Postgres persistence. Both
  rebuild the entire callback payload from hashed correlation ids and fixed `msgData`
  field-presence metadata, so file credentials, media URLs, filenames, identities,
  message content, envelope siblings, unknown field values, and callback ids cannot
  enter JetStream, `raw_events`, or normalized message rows. Invalid dead letters retain
  only payload byte count and SHA-256. This is a storage safety prerequisite, not
  callback processing or send enablement.
- The additive `qiwe_image_send_attempts` state records hashed upload correlation,
  callback idempotency, unique per-attempt claims, immutable artifact/target hashes, and
  the approved final JPEG MD5 and byte size. Callback credentials remain memory-only;
  their filename, MD5, and byte size must match the approved artifact before the
  callback can open the send gate. The callback transition commits `sending` before an
  external send can occur, and ambiguous outcomes are terminal/manual rather than
  automatically retried. After that send gate, HTTP failures or provider non-success
  responses remain ambiguous unless the bounded client proves the request was not sent.
- The upload worker now persists `uploading` in the claim transaction before external
  I/O. If it cannot prove that an interrupted upload stayed local, that attempt and the
  work item become terminal ambiguous state with no automatic retry. Dry-run and
  disabled previews enforce the same exact group and media-host allowlists as apply.
- `run-qiwe-image-send-worker` connects that state API to one guarded asynchronous
  upload request, and `process-qiwe-image-send-callback` reads one bounded callback from
  stdin before opening the at-most-once send gate. Both use the same bounded Rust HTTP
  client as Huabaosi, zeroize sensitive buffers, and have local fake-server coverage.
  The live helpers compile only with `qiwe-staging-adapter` or
  `qiwe-production-adapter`. Huabaosi production release artifacts record exactly
  `huabaosi-production-adapter` and `huabaosi-feishu-mirror-adapter`, and must reject
  QiWe live features, staging approval, staging databases, or missing Feishu delivery
  configuration before apply. The guarded staging smoke remains an owner-approved
  one-shot operator entrypoint.
- A combined staging build containing both `huabaosi-staging-adapter` and
  `qiwe-staging-adapter` may claim an exact Feishu primary-storage URI. It commits the
  existing `uploading` attempt before Feishu or QiWe I/O, revalidates the approved JPEG,
  performs the non-deprecated SDK temporary upload, reads the temporary URL back, and
  proves SHA-256, MD5, and byte size before invoking the existing async upload. The
  bytes, multipart body, and temporary URL are memory-only and zeroized; default and
  single-feature builds reject this route, while production requires the matching
  Huabaosi Feishu mirror plus QiWe production feature pair.
- Callback parsing classifies the raw `msgData` field names into one of four fixed,
  reviewed credential schema ids before deserializing credential values. Reports expose
  only that fixed id and an additional-field count. They reject simultaneous canonical
  and alias spellings and never expose the request id, credential values, filename, MD5,
  unknown field names, or unknown values. This makes an owner-approved staging callback
  safe to inspect, but it is instrumentation only and is not staging evidence.
- The existing Hermes webhook has a disabled-by-default bridge that recognizes
  `cmd=20000` before ordinary Agent dispatch and streams the bounded callback only to a
  fixed `process-qiwe-image-send-callback --apply` child over stdin. Enablement requires
  the exact owner phrase, canonical approved staging database URL hash, explicit send
  and webhook readiness flags, and an executable staging sidecar path. Stderr is
  discarded and stdout must match the bounded sanitized Rust report schema.

## Next Implementation

1. Run one owner-approved staging image generation and verify the final JPEG media
   metadata and same-byte readback without sending.
2. Use the guarded upload smoke for one explicit send-ready work item and receive its
   real callback through the disabled-by-default webhook bridge in the isolated staging
   runtime. The existing callback-phase smoke remains the operator-controlled stdin
   entrypoint when the webhook bridge is not used. Neither path may persist callback
   bytes or credential values.
3. Commit only the staging database URL SHA-256, final JPEG `artifact_content_hash`,
   sanitized callback schema id, fixed outcome labels, and reviewed rollback evidence.
   The QiWe upload/callback hash must match the Huabaosi generated-image `content_hash`
   and pass `check-xiaoman-image-send-staging-evidence.mjs` before any production
   enablement PR. Do not commit the database URL, callback body, request id,
   credentials, group id, media URL, or provider response. Record the full staging
   sequence with `docs/reports/templates/xiaoman-image-send-staging-evidence.md` only
   after the Huabaosi, QiWe, and cross-flow evidence checkers pass.
4. Add production scheduling only in a separate owner-approved PR with a reviewed live
   adapter artifact, fixed systemd worker timer, rollback ownership, callback boundary,
   and exact production allowlists.

## Guarded Staging Smoke Contract

`qiwe-image-send-staging-preflight` is a local-only staging readiness check. Before any
database connection, callback read, or network request it requires:

- a binary compiled with only the reviewed `qiwe-staging-adapter` live feature;
- `QINTOPIA_QIWE_IMAGE_SEND_ENABLED=1` and webhook readiness;
- the exact one-shot owner approval phrase;
- complete API, media-host, and case-sensitive target-group allowlists; and
- the complete Feishu primary-storage release, database-hash, Base/table allowlist,
  schema, profile-path, and enablement boundary required for authenticated JPEG
  readback;
- a staging database whose exact URL hash is supplied in the owner-reviewed one-shot
  command and matches the sourced database URL.
- the packaged staging sidecar binary whose exact SHA-256 is supplied in the same
  owner-reviewed command.

The smoke runs as two explicit invocations because the QiWe upload callback is
asynchronous. A separate `preflight` phase validates the staging boundary without
claiming work or contacting QiWe:

```bash
QINTOPIA_QIWE_IMAGE_STAGING_SMOKE_ENABLE=1 \
QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL=approved-staging-qiwe-image-send \
QINTOPIA_QIWE_IMAGE_STAGING_PHASE=preflight \
QINTOPIA_QIWE_IMAGE_STAGING_ENV_FILE=/etc/qintopia/message-sidecar-staging.env \
QINTOPIA_QIWE_IMAGE_STAGING_DATABASE_URL_SHA256='<approved staging database URL sha256>' \
QINTOPIA_QIWE_IMAGE_STAGING_SIDECAR_SHA256='<approved staging sidecar binary sha256>' \
deploy/sidecar/scripts/qiwe-image-send-staging-smoke.sh

QINTOPIA_QIWE_IMAGE_STAGING_SMOKE_ENABLE=1 \
QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL=approved-staging-qiwe-image-send \
QINTOPIA_QIWE_IMAGE_STAGING_PHASE=upload \
QINTOPIA_QIWE_IMAGE_STAGING_ENV_FILE=/etc/qintopia/message-sidecar-staging.env \
QINTOPIA_QIWE_IMAGE_STAGING_DATABASE_URL_SHA256='<approved staging database URL sha256>' \
QINTOPIA_QIWE_IMAGE_STAGING_SIDECAR_SHA256='<approved staging sidecar binary sha256>' \
QINTOPIA_QIWE_IMAGE_STAGING_WORK_ITEM_ID='<approved send-ready UUID>' \
deploy/sidecar/scripts/qiwe-image-send-staging-smoke.sh

trusted-staging-callback-source | \
QINTOPIA_QIWE_IMAGE_STAGING_SMOKE_ENABLE=1 \
QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL=approved-staging-qiwe-image-send \
QINTOPIA_QIWE_IMAGE_STAGING_PHASE=callback \
QINTOPIA_QIWE_IMAGE_STAGING_ENV_FILE=/etc/qintopia/message-sidecar-staging.env \
QINTOPIA_QIWE_IMAGE_STAGING_DATABASE_URL_SHA256='<same approved staging database URL sha256>' \
QINTOPIA_QIWE_IMAGE_STAGING_SIDECAR_SHA256='<same approved staging sidecar binary sha256>' \
QINTOPIA_QIWE_IMAGE_STAGING_WORK_ITEM_ID='<same approved send-ready UUID>' \
deploy/sidecar/scripts/qiwe-image-send-staging-smoke.sh
```

The callback source must stream one callback directly to stdin. It must not create a
callback file, environment variable, CLI argument, NATS event, or log record containing
the raw credentials. Preflight and upload subprocesses receive `/dev/null`; only the
callback processor inherits the stream. The staging env file is parsed as a fixed
allowlist of literal assignments and is never evaluated as shell. The smoke stores only
subprocess output in shell memory and validates the fixed report schema through an
anonymous pipe. It never writes successful, failed, or sensitive subprocess output to a
file. Successful phases print fixed `qiwe_image_send_staging_evidence=<json>` objects
that contain only the reviewed evidence fields. The QiWe operator checklist lives in
`docs/operations/qiwe-image-send-staging-runbook.md`; the full Xiaoman Huabaosi-to-QiWe
evidence template lives in
`docs/reports/templates/xiaoman-image-send-staging-evidence.md`.

## Production Boundary

Default execution still cannot contact QiWe or send messages. Huabaosi production
artifacts may contain only the reviewed `huabaosi-production-adapter` and
`huabaosi-feishu-mirror-adapter` feature set; QiWe live features, staging adapters, and
all-features builds remain forbidden. The production observation smoke accepts only the
immutable `release/current` binary and fixed production env file, and it must not run
`--apply` or process callbacks. Because that binary is the Huabaosi production artifact,
the observation may prove only the disabled QiWe worker and callback-bridge state. It
must fail closed if either QiWe production enable flag is `1`; an enabled state requires
a separate reviewed QiWe production artifact and must never be made possible by adding
`qiwe-production-adapter` to the Huabaosi artifact.

Production activation is guarded rather than automatic. It requires the persistent
enablement flag, exact production owner phrase, canonical production database URL hash,
and release-local `qiwe-image-send-production-preflight` before enabling the worker
timer. That worker can upload one approved send-ready image and persist sanitized
`awaiting_callback` state, but final `/msg/sendImage` still depends on one reviewed
`cmd=20000` callback reaching `process-qiwe-image-send-callback --apply`. The activation
and rollback scripts read only the fixed reviewed `/etc/qintopia/message-sidecar.env`
and do not accept caller-provided env-file or systemctl command overrides; they reset
PATH and call systemctl by absolute path.

The Hermes webhook callback bridge is the reviewed callback ingress for that final step.
In production mode it must use exactly
`/home/ubuntu/qintopia-agent-os-releases/current/sidecar/qintopia-message-sidecar` with
root `/home/ubuntu/qintopia-agent-os-releases/current`, the production owner/database
gate, explicit image-send and webhook readiness flags, and an approved sidecar SHA-256.
It streams the bounded callback only through child stdin and never via argv, env, files,
NATS, logs, or HTTP response bodies. It does not publish a Release, enable timers,
approve images, or bypass the Rust production apply gate.

The release-local callback bridge production observation is read-only:

```bash
QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_OBSERVATION_ENABLE=1 \
  deploy/sidecar/scripts/qiwe-image-callback-bridge-production-observation-smoke.sh
```

It verifies that Erhua's QiWe plugin symlink resolves to `release/current`, the callback
bridge is explicitly disabled, and the immutable release binary carries exactly the
approved Huabaosi production features. It ignores unrelated secret-bearing env values
and fails closed if the bridge enable flag is `1`. It does not process callbacks, start
services, enable timers, read Feishu, write Postgres, call QiWe, or publish.

Production callback bridge activation is guarded and separate from the image-send worker
timer. The current Huabaosi-only release cannot pass its enabled observation, so the
activation command must not restart Erhua until a separate reviewed QiWe production
artifact and required staging evidence exist. That future boundary must first persist
the reviewed Erhua env values with `QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_ENABLED=1`,
production mode, release/current processor path/root, the approved sidecar SHA-256,
image-send enablement, webhook readiness, production approval, and the database URL hash
that matches the runtime database URL. The release-local activation script then runs the
same observation with `expected_state=enabled`, restarts only
`hermes-gateway-erhua.service` as the `ubuntu` user, and observes the enabled state
again. The command below documents that guarded future boundary; it is not an activation
instruction for the current Huabaosi artifact:

```bash
QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_ACTIVATION=approved-production-qiwe-image-callback-bridge \
  deploy/sidecar/scripts/activate-qiwe-image-callback-bridge-production.sh
```

Rollback requires the persistent Erhua env to set
`QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_ENABLED=0`, then restarts only Erhua and proves
the bridge is disabled:

```bash
QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_ROLLBACK=approved-production-qiwe-image-callback-bridge-rollback \
  deploy/sidecar/scripts/rollback-qiwe-image-callback-bridge-production.sh
```

Neither script enables the QiWe worker timer, processes callback JSON, calls Postgres,
calls QiWe, writes Feishu, publishes, sends, sources env files, or prints credentials.

Until a new immutable Release is owner-published, deployed, activated, and backed by one
real Xiaoman activity evidence bundle, QiWe image send remains code-ready rather than a
completed production workflow.
