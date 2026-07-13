# Aliang Image Adapter Review Remediation

Date: 2026-07-13

## Review Findings

The initial guarded image-adapter implementation had two production-safety gaps:

1. Provider, media-upload, and media-readback responses were read with `read_to_end`
   before PNG or JSON validation. An unhealthy or hostile endpoint could return an
   unbounded response and exhaust worker memory.
2. The generated-image conflict path updated URI and metadata without considering the
   existing review status. A retry could change the storage reference of an approved
   artifact while downstream still trusted its approval.

No secret, token, live environment file, Hermes runtime state, Feishu identifier, or raw
chat content was present in the reviewed change.

## Resolution

- Add explicit response limits before parsing: a bounded raw HTTP read, declared
  `Content-Length` validation, and a matching decoded chunked-body limit. Provider and
  upload metadata responses have narrow limits; image readback is limited by the
  reviewed media-size setting.
- Change generated-image conflict handling to `DO NOTHING`. Only an existing `pending`
  artifact with the same work-item and content hash may be reused. Approved, rejected,
  and changes-requested artifacts cause the attempt to fail without changing the stored
  URI, metadata, review status, or audit history.

## Validation

- Rust unit tests cover bounded raw reads, content-length rejection, chunked-body
  rejection, successful same-byte round-trip, mismatched readback, and reviewed-artifact
  reuse policy.
- Full Rust tests, Clippy, Markdown checks, workflow/deploy contracts, and the guarded
  PostgreSQL integration job remain required before merge.

## Remaining Boundary

The adapter is still disabled by default. This remediation does not add a timer, real
provider account, media storage configuration, Feishu writeback, QiWe sending, or any
external publish path.
