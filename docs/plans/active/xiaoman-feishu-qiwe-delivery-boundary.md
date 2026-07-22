# Xiaoman Feishu-To-QiWe Delivery Boundary

Updated: 2026-07-18

## Goal

Define the missing boundary between Huabaosi Feishu-backed generated-image storage and
QiWe group image delivery before any staging or production enablement claims that the
full Xiaoman activity workflow is complete.

This plan does not publish a Release, provision staging env files, approve an artifact,
call Feishu or QiWe, install a listener or timer, or send externally.

## Current Facts

- Huabaosi primary storage may create a pending `generated_image` whose `artifact_uri`
  is `feishu-base://huabaosi-generated-image/<artifact-id>`.
- The Feishu primary-storage writer uploads the exact final JPEG attachment and reads it
  back through the authenticated Feishu media API before creating the pending AgentOS
  artifact.
- The sidecar has a read-only
  `huabaosi-feishu-primary-storage-revalidate --artifact-id <uuid>` entrypoint that can
  reload the fixed Feishu Base record by `AgentOS产物ID`, download the `最终JPEG`
  attachment through the authenticated Feishu media API, and compare those bytes with
  the AgentOS artifact metadata and creation audit. Its report is sanitized and it does
  not approve, write Postgres or Feishu, call QiWe, publish, or send.
- Generated-image approval can consume authenticated Feishu primary-storage revalidation
  for an explicit manual `approved` apply. Rejection and changes-requested decisions
  remain available without Feishu I/O, and a Feishu field or automation event alone is
  still not approval.
- A combined live build containing both the Huabaosi Feishu primary-storage path and a
  reviewed QiWe live adapter can deliver an exact `feishu-base://` artifact by
  committing the existing `uploading` attempt, reading the authenticated Feishu bytes,
  uploading those bytes to QiWe SDK temporary storage, reading the returned temporary
  URL back, and then invoking the existing async URL upload path. Default,
  Huabaosi-only, and QiWe-only builds continue to reject this route. Staging requires
  `huabaosi-staging-adapter` plus `qiwe-staging-adapter`; production requires
  `huabaosi-feishu-mirror-adapter` plus `qiwe-production-adapter` with production
  owner/database/Feishu delivery gates.
- The reviewed QiWe protocol plan says the synchronous local and URL upload APIs are
  marked for deprecation and must not become the production foundation.

## Official QiWe Storage Bridge Evidence

The QiWe Open Platform documentation rechecked on 2026-07-18 also exposes the released,
non-deprecated [SDK temporary-storage upload](https://doc.qiweapi.com/api-344613899)
`/cloud/cloudUpload` multipart API. It accepts local bytes into QiWe's SDK temporary
object storage and returns a `cloudUrl`; the documentation states that this storage does
not call the WeCom service API and is periodically cleared. The released, non-deprecated
[asynchronous URL upload](https://doc.qiweapi.com/api-347221662)
`/cloud/cdnUploadByUrlAsync` API can then consume that URL and retain the existing
callback/send-image protocol.

This made the reviewed staging delivery bridge possible without exposing a Feishu
attachment token or introducing a new Qintopia-hosted public proxy. It does not make the
temporary URL a stable AgentOS artifact URI. The implemented bridge keeps the URL
memory-only, exact-allowlists its host, downloads it back through a bounded client,
proves the returned bytes match the approved JPEG identity, and only then calls the
existing asynchronous upload API.

The reviewed request contract is `POST /qiwe/api/qw/doFileApi` with exactly the
multipart fields `method=/cloud/cloudUpload`, `guid`, and `file`. Success requires
`code=0` and one HTTPS `data.cloudUrl`. The file API URL is derived only from the
already validated `/qiwe/api/qw/doApi` endpoint by replacing that exact terminal path;
it is not caller-selectable. The returned temporary host must be present in the
owner-reviewed Huabaosi media host allowlist, while the QiWe host allowlist remains
limited to the API host.

## Gap

The code-level bridge now exists, but real runtime evidence is still missing.

Therefore the current fail-closed boundary is:

- human approval may approve a `feishu-base://` generated image only after authenticated
  Feishu primary-storage revalidation succeeds inside the explicit manual review apply;
- QiWe preview, claim, upload, and callback policy may accept `feishu-base://` only in
  the combined staging feature artifact with exact owner approval, staging database
  hash, Feishu and QiWe allowlists, same-byte temporary-storage readback, and the
  existing at-most-once callback/send gate; and
- staging evidence must not claim a Huabaosi-to-QiWe hash match until one real Huabaosi
  staging image, one QiWe staging upload/callback/send, and the cross-flow evidence
  checker all pass.

## Implementation Phases

The boundary was implemented in reviewed phases, but runtime evidence is still pending:

1. Authenticated Feishu attachment revalidation.
   - Load only the fixed Huabaosi generated-image Base/table boundary.
   - Resolve the row by the generated-image artifact id.
   - Download the attachment through the authenticated Feishu media API.
   - Recompute the final JPEG SHA-256, MD5, byte size, MIME type, dimensions, and fixed
     transform identity.
   - Compare those facts with the AgentOS artifact metadata and creation audit event.
   - Keep Feishu credentials, attachment tokens, file ids, filenames, media URLs, and
     raw bytes out of Postgres, logs, reports, CLI args, and environment-derived output.
   - The first entrypoint is read-only and does not change the approval gate by itself.

2. Authenticated approval consumption.
   - Only an explicit human `approved` apply may consume the read-only revalidation.
   - Revalidation must finish before the review mutation and remain memory-only.
   - The review transaction must reload and lock the artifact, then match artifact id,
     image-request id, Feishu URI, content hash, MD5, byte size, and dimensions against
     the revalidated evidence before recording approval.
   - Rejection and changes-requested decisions must remain available without Feishu I/O.
   - A Feishu workbench field or automation event must not become approval merely
     because the row says it was reviewed.

3. A reviewed QiWe delivery path for the exact revalidated JPEG.
   - It must not expose Feishu attachment tokens or private media URLs.
   - It must not introduce an unreviewed public proxy, upload service, or mutable
     source-tree fallback.
   - It must not fall back to QiWe synchronous upload APIs marked deprecated in
     `docs/plans/active/xiaoman-qiwe-image-send.md`.
   - It must preserve the current at-most-once send gate, sanitized callback evidence,
     target-group allowlist, and rollback ownership.
   - The Postgres `uploading` attempt must be committed before authenticated Feishu
     readback or either QiWe upload call. Interrupted external work remains terminal
     ambiguous and is never retried automatically.
   - Only a combined live artifact containing both the Huabaosi Feishu primary-storage
     path and a reviewed QiWe live adapter may claim a `feishu-base://` artifact.
     Default, Huabaosi-only, and QiWe-only builds must continue to reject it. Staging
     requires `huabaosi-staging-adapter` plus `qiwe-staging-adapter`; production
     requires `huabaosi-feishu-mirror-adapter` plus `qiwe-production-adapter` with
     production owner/database/Feishu delivery gates.
   - The revalidated JPEG bytes, multipart body, returned `cloudUrl`, and readback bytes
     remain memory-only and are zeroized. No temporary URL or attachment credential may
     enter Postgres, reports, logs, CLI arguments, or environment-derived output.
   - The callback filename is fixed as `generated-image-<artifact-id>.jpg`; it is not
     accepted from Feishu fields or QiWe temporary-storage metadata.
   - It must update the QiWe staging evidence checker and cross-flow checker so the
     Huabaosi final JPEG `content_hash` is proven against the exact bytes delivered to
     QiWe.

## Non-Solutions

- Treating `feishu-base://` as a media URL.
- Putting Feishu attachment tokens, private download links, table ids, or callback file
  credentials into AgentOS facts or reports.
- Adding a temporary public file host outside a reviewed architecture decision.
- Bypassing the async callback evidence by using deprecated QiWe synchronous uploads as
  the production path.
- Enabling a production QiWe listener, service, timer, feature build, or send flag
  before isolated staging proves the new delivery path.

## Next Work

1. Keep #180 and any infrastructure Release classified as not production-complete.
2. Provision staging runtime env only after the owner-approved values file exists and
   release/current contains the reviewed staging runbooks and checks.
3. Run Huabaosi staging generation to retain one pending Feishu-backed generated-image
   evidence record.
4. Run the explicit manual approval apply for that artifact so authenticated Feishu
   revalidation is exercised before the review mutation.
5. Run QiWe staging preflight, upload, and callback/send with the combined staging
   artifact, retaining only sanitized evidence.
6. Run the cross-flow checker to prove Huabaosi `content_hash` equals the QiWe
   `artifact_content_hash` before any production enablement PR.
