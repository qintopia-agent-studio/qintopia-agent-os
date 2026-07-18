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
- Generated-image approval currently accepts only stable HTTPS JPEG artifact URIs.
- QiWe image-send claim and upload currently accept only stable allowlisted HTTPS media
  URLs because the reviewed staging adapter uses QiWe asynchronous URL upload.
- The reviewed QiWe protocol plan says the synchronous local and URL upload APIs are
  marked for deprecation and must not become the production foundation.

## Gap

Authenticated Feishu Base attachment storage proves the final JPEG identity to AgentOS,
but it does not give QiWe a reviewed delivery URL. The current QiWe async URL upload
request requires `fileUrl`. A `feishu-base://` artifact is not an HTTPS URL, and the
underlying Feishu attachment is private and credentialed.

Therefore the current fail-closed behavior is correct:

- human approval must continue to reject `feishu-base://` generated images;
- QiWe preview, claim, upload, and callback policy must continue to reject
  `feishu-base://` generated images; and
- staging evidence must not claim a Huabaosi-to-QiWe hash match from a Feishu-backed
  artifact until a reviewed delivery path exists.

## Required Future PR

A later implementation PR must add both halves together:

1. Authenticated Feishu attachment revalidation for approval.
   - Load only the fixed Huabaosi generated-image Base/table boundary.
   - Resolve the row by `generated_image_artifact_id` or its reviewed workbench ref.
   - Download the attachment through the authenticated Feishu media API.
   - Recompute the final JPEG SHA-256, MD5, byte size, MIME type, dimensions, and fixed
     transform identity.
   - Compare those facts with the AgentOS artifact metadata and creation audit event.
   - Keep Feishu credentials, attachment tokens, file ids, filenames, media URLs, and
     raw bytes out of Postgres, logs, reports, CLI args, and environment-derived output.

2. A reviewed QiWe delivery path for the exact revalidated JPEG.
   - It must not expose Feishu attachment tokens or private media URLs.
   - It must not introduce an unreviewed public proxy, upload service, or mutable
     source-tree fallback.
   - It must not fall back to QiWe synchronous upload APIs marked deprecated in
     `docs/plans/active/xiaoman-qiwe-image-send.md`.
   - It must preserve the current at-most-once send gate, sanitized callback evidence,
     target-group allowlist, and rollback ownership.
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
4. Implement the future Feishu-to-QiWe delivery PR described above.
5. Only then run QiWe staging upload/callback/send evidence and the cross-flow checker.
