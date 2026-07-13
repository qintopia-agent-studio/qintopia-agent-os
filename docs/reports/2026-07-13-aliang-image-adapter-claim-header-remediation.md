# Aliang Image Adapter Claim And Header Remediation

Date: 2026-07-13

## Review Findings

The handwritten image-adapter HTTP client appended dynamic header values directly to the
request text. A newline in an API key or work-item-derived idempotency value could
inject an extra header. Separately, all attempts used the same worker identifier as
their claim owner, so a result from an expired attempt could update a newer attempt's
work item.

## Resolution

- Validate every outbound header name as an HTTP token and every value as printable
  header text before connecting. CR, LF, other control bytes, and invalid header names
  are rejected without echoing the rejected value.
- Assign each claim a UUID-derived token. Before recording a generated artifact, the
  worker locks the work item and requires `status=processing`, the exact token, and an
  unexpired claim. The subsequent state update must affect exactly one row.
- Failure handling uses the same token and expiry condition. When an attempt has become
  stale it writes neither a failure event nor a work-item state change, and reports
  `image_generation_stale_claim`.

## Validation

- Rust tests cover header CR/LF rejection, unique per-attempt claim tokens, bounded
  responses, rejected chunked bodies, same-byte media readback, and immutable reviewed
  artifacts.
- Full Rust, Clippy, repository checks, and the disposable PostgreSQL integration job
  are required before merge.

## Remaining Boundary

The worker remains disabled by default and has no timer. This change does not authorize
a real provider, media storage, Feishu writeback, QiWe sending, or external publishing.
