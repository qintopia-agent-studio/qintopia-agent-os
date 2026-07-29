# QiWe Temporary-Storage URL Validation Failure

Date: 2026-07-29

## Summary

The owner-approved QiWe production timer activation on release `v0.2.54`
(`05aaaefb4bcde3b7a93814c431c5675ba826b178`) selected one new human-confirmed send
request. Authenticated Feishu primary-storage revalidation succeeded, and QiWe's SDK
temporary-storage upload returned HTTP 200, but the returned `cloudUrl` failed the local
reviewed URL policy before readback or asynchronous upload.

No callback was received and `/msg/sendImage` was not called. The attempt was
conservatively terminalized as `ambiguous` and must not be reset or retried. Production
image sending was restored to disabled, and the worker timer was disabled and stopped.

## Evidence

- The immutable runtime was the `qiwe-production` companion from release `v0.2.54`.
- The generated JPEG revalidated as `1254x1254`, `681271` bytes, with matching content
  identity before the worker reached QiWe temporary storage.
- The worker reported `image_upload_outcome_unknown`, HTTP status `200`, failure stage
  `temporary_storage_url_validation`, `callback_received=false`, and
  `external_send_executed=false`.
- A second timer wake found no claimable request, so it performed no external upload or
  send.
- The persistent media allowlist has one host and already contains the exact OSS host
  shown in QiWe's official temporary-storage documentation. It does not contain only the
  QiWe API host.
- The release-local disabled-state observation passed after rollback; the timer and
  worker were both inactive, and the timer unit was disabled.

## Root Cause

The immediate provider URL difference is not yet proven. The validator rejected
non-HTTPS schemes, missing hosts, userinfo, query strings, fragments, ambiguous paths,
overlong URLs, and non-allowlisted hosts, but collapsed every case into the single
sanitized stage `temporary_storage_url_validation`.

Because the returned URL is intentionally memory-only and zeroized, the production
evidence cannot distinguish a signed query from a host or structure mismatch. Changing
the allowlist or accepting signed queries without that distinction would weaken the
boundary without evidence.

## Remediation Boundary

- Emit only fixed validation categories such as `query_not_allowed`,
  `host_not_allowlisted`, and `scheme_not_allowed`, prefixed by the existing
  temporary-storage stage.
- Do not emit the URL, host, path, query, fragment, credentials, or response body.
- Keep the current exact host allowlist, HTTPS requirement, query rejection,
  at-most-once state machine, callback contract, timer state, and CI scope unchanged.
- Do not reuse either terminal ambiguous production attempt.

## Acceptance

1. Unit tests cover each fixed URL-validation category and prove stage strings contain
   no fixture host or query value.
2. Existing QiWe adapter tests and the repository PR tier pass.
3. After a reviewed Release is deployed, create one third, new human-confirmed request
   for the same approved image.
4. Activate only the production timer and let its persistent wake run the worker; do not
   also start the worker manually.
5. If the fixed category is `query_not_allowed`, review a narrowly scoped change that
   permits a query only on an HTTPS URL whose host exactly matches the allowlist and
   that still passes same-byte readback. If it is `host_not_allowlisted`, identify and
   review the exact real host before changing the allowlist. Do not broaden both
   controls at once.
