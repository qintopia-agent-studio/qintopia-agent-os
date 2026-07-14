# Xiaoman QiWe Image Send

Updated: 2026-07-14

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
  exist, but no callback listener, staging smoke, service, or timer is installed.
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
  sanitized terminal audit. Callback credentials remain memory-only. The callback
  transition commits `sending` before an external send can occur, and ambiguous outcomes
  are terminal/manual rather than automatically retried. After that send gate, HTTP
  failures or provider non-success responses remain ambiguous unless the bounded client
  proves the request was not sent.
- `run-qiwe-image-send-worker` connects that state API to one guarded asynchronous
  upload request, and `process-qiwe-image-send-callback` reads one bounded callback from
  stdin before opening the at-most-once send gate. Both use the same bounded Rust HTTP
  client as Huabaosi, zeroize sensitive buffers, and have local fake-server coverage.
  They are code-only capabilities: no listener, service, timer, staging endpoint, or
  production enablement is installed.

## Next Implementation

1. Run one owner-approved staging image generation and verify the final JPEG media
   metadata and same-byte readback without sending.
2. Capture one owner-approved staging async callback through a dedicated callback path
   before generic raw-event sanitization, and confirm the exact credential field names
   and the existing `isSendSuccess=1` success assumption without storing raw credentials
   in git or logs.
3. Complete CI review of the code-only
   [guarded adapter worker](qiwe-image-send-adapter-worker.md), including local fake
   QiWe upload/send behavior and disposable PostgreSQL crash/timeout recovery. Keep real
   endpoints disabled until steps 1 and 2 have owner-approved evidence.
4. Add one guarded staging smoke with an isolated group and explicit approval phrase.
5. Add production scheduling only after staging evidence, rollback ownership, and
   allowlists are reviewed in a separate PR.

## Production Boundary

Default and production execution do not contact QiWe or send messages. The guarded
commands can write Postgres and contact an allowlisted endpoint only with explicit
enablement, but this plan does not install or enable them, write Feishu, or change
production configuration. Rollback is to keep `QINTOPIA_QIWE_IMAGE_SEND_ENABLED=0`; no
current internal Xiaoman timer depends on this adapter.
