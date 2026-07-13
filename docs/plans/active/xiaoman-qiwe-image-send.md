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

## Current Compatibility Gap

The official send-image page names JPG as the supported image format and asynchronous
upload uses `fileType=1` for JPG. The current Huabaosi adapter deliberately produces and
verifies only `image/png`. Renaming PNG bytes to `.jpg` is invalid and must fail closed.

Before staging, the owner and provider must choose one reviewed path:

1. add deterministic PNG-to-JPEG conversion after generated-image validation while
   preserving source and derived hashes; or
2. obtain vendor evidence that the current upload/send path accepts PNG bytes and update
   the checked contract and tests accordingly.

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
- Header values reject CR/LF, endpoints require the reviewed HTTPS path, media URLs
  reject credentials/query/fragment and non-allowlisted hosts, and JPEG MIME/JPG naming
  are enforced until compatibility is decided.
- `qiwe-image-send-preflight` checks only local configuration and emits a sanitized
  report. It opens no network or database connection.
- `QINTOPIA_QIWE_IMAGE_SEND_ENABLED` defaults to `0`; no worker, callback listener,
  staging smoke, service, or timer is installed.

## Next Implementation

1. Resolve and test the PNG/JPG decision.
2. Capture one owner-approved staging async callback and confirm the exact credential
   field names and `isSendSuccess` semantics without storing raw credentials in git or
   logs.
3. Add Postgres upload-correlation state, callback idempotency, claim-token validation,
   and durable sanitized external-send audit.
4. Add a local fake QiWe server for upload acceptance, callback, send response, timeout,
   oversized response, duplicate callback, stale claim, and retry tests.
5. Add one guarded staging smoke with an isolated group and explicit approval phrase.
6. Add production scheduling only after staging evidence, rollback ownership, and
   allowlists are reviewed in a separate PR.

## Production Boundary

This plan and contract do not contact QiWe, upload media, send messages, write Postgres
or Feishu, install services, or change production configuration. Rollback is to keep
`QINTOPIA_QIWE_IMAGE_SEND_ENABLED=0`; no current internal Xiaoman path depends on this
adapter.
