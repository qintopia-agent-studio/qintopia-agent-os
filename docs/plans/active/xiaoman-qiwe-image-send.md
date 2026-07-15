# Xiaoman QiWe Image Send

Updated: 2026-07-15

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
  -> validate immutable HTTPS URI, target allowlist, final confirmation, and format
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
  report. It opens no network or database connection and fails closed if the send-enable
  flag is already `1` because this PR does not approve enablement.
- Preflight `missing_configuration` may list only fixed public variable names from
  `.env.example`, never values, URLs, hosts, group ids, or enable flags. An empty list
  with `config_valid=false` means present configuration failed format, readiness, or
  allowlist validation and must still fail closed.
- `QINTOPIA_QIWE_IMAGE_SEND_ENABLED` defaults to `0`; guarded upload/callback commands
  exist, but default and production binaries compile without the non-default
  `qiwe-staging-adapter` feature. Apply fails before Postgres or network access even if
  the runtime enable flag is misconfigured. A staging-feature apply also requires the
  exact owner approval phrase before Postgres or network access. No callback listener,
  staging smoke, service, or timer is installed.
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
  The live helpers compile only with the staging-only Cargo feature. Production release
  artifacts record only the unrelated `huabaosi-production-adapter` feature and cannot
  execute either QiWe external call; no service, timer, staging endpoint, or production
  enablement is installed. The guarded staging smoke exists only as an owner-approved
  one-shot operator entrypoint.
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
4. Add production scheduling only after staging evidence, rollback ownership, and
   allowlists are reviewed in a separate PR.

## Guarded Staging Smoke Contract

`qiwe-image-send-staging-preflight` is a local-only staging readiness check. Before any
database connection, callback read, or network request it requires:

- a binary compiled with only the reviewed `qiwe-staging-adapter` live feature;
- `QINTOPIA_QIWE_IMAGE_SEND_ENABLED=1` and webhook readiness;
- the exact one-shot owner approval phrase;
- complete API, media-host, and case-sensitive target-group allowlists; and
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

Default and production execution do not contain the live QiWe adapter and cannot contact
QiWe or send messages. A separately built staging-feature binary can write Postgres and
contact an allowlisted endpoint only with explicit enablement, but this plan does not
build, install, or enable one, write Feishu, or change production configuration.
Rollback is to retain default builds and `QINTOPIA_QIWE_IMAGE_SEND_ENABLED=0`; no
current internal Xiaoman timer depends on this adapter.
