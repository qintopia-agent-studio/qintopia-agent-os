# QiWe Feishu Primary-Storage Revalidation Failure

Date: 2026-07-29

## Summary

The owner-approved QiWe production timer activation passed on release `v0.2.53`
(`c57da23fb2ce51994a6db0a31c28c27dc805afa1`). The first reviewed candidate was the only
claimable send request, but the worker stopped before any QiWe upload or group send.

The attempt was conservatively terminalized as `ambiguous`. It must not be retried or
reset. The persistent send flag was restored to `0`, and the production timer was rolled
back to disabled and inactive while the code path is repaired.

## Evidence

- Production preflight and timer activation passed against the immutable
  `qiwe-production` companion artifact.
- The selected work item was queued, unclaimed, human-confirmed, and the only claimable
  send request before the worker started.
- The worker reported `feishu_delivery_revalidation_outcome_unknown` with
  `external_upload_requested=false` and `external_send_executed=false`.
- Postgres recorded no callback and no send start. No `/msg/sendImage` call occurred.
- The release-local read-only primary-storage revalidation command returned
  `Huabaosi generated image artifact was not found` for the approved artifact UUID.
- A read-only production SQL contrast matched zero rows through the old HTTPS predicate
  and one reviewed row through the exact primary-storage URI predicate.

## Root Cause

`revalidate_primary_storage_for_delivery` reused the Feishu mirror queue's
`peek_candidate` query. That query intentionally accepts only source artifacts whose URI
starts with `https://`, so the mirror worker cannot select already-mirrored rows.

The approved production artifact correctly used the exact primary-storage URI
`feishu-base://huabaosi-generated-image/<artifact-uuid>`. Reusing the HTTPS-only mirror
query therefore made every Feishu-primary delivery artifact invisible before
authenticated readback.

## Remediation Boundary

- Keep the mirror candidate query HTTPS-only.
- Add a separate explicit primary-storage lookup that requires the artifact UUID and
  exact `feishu-base://huabaosi-generated-image/<artifact-uuid>` URI.
- Route read-only revalidation, approval revalidation, and QiWe delivery revalidation
  through the primary-storage lookup.
- Do not change the upload, callback, send, retry, timer, or CI contracts.
- Do not retry or rewrite the terminal ambiguous attempt. After the corrected release is
  deployed, create a new human-confirmed send request for the same approved image.

## Acceptance

1. Unit coverage proves the mirror query remains HTTPS-only and the primary-storage
   query uses exact URI equality.
2. The guarded PostgreSQL integration proves a `feishu-base://` artifact is excluded
   from the mirror queue and is available to the primary-storage lookup.
3. The relevant Rust tests and repository PR tier pass.
4. After release, read-only primary-storage revalidation succeeds before the timer is
   re-enabled.
5. A new candidate completes upload, callback, `/msg/sendImage`, and visual
   group-arrival confirmation exactly once.
