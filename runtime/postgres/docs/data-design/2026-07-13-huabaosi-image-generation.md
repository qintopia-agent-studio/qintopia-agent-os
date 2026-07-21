# Huabaosi Image-Generation Request Capability

Date: 2026-07-13

## Purpose

Register `huabaosi.generate_image_asset` so an approved Xiaoman `poster_brief` can
create an auditable `image_generation_request` under its visual work item.

## Contract

- Provider: `huabaosi`.
- Callers: `xiaoman` and `default`.
- Work item type: `image_generation_request`.
- Risk: `high`; review policy: `before_external_use`.
- Input records an approved brief id/hash, evidence hash when available, an allowlisted
  image specification, and a redacted prompt hash.
- Output contract is a future `generated_image` artifact with `review_status=pending`.

## Boundary

The capability registration does not enable an external provider, media upload, image
artifact creation, Feishu writeback, QiWe sending, or publication. The worker defaults
to `QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED=0`; a separate owner-reviewed adapter and
isolated media storage decision are required before any network call.

When enabled by that reviewed configuration, the adapter accepts only OpenAI-compatible
`b64_json` PNG output, uploads it to the dedicated media boundary, verifies a same-byte
readback, and writes one `generated_image` artifact with `review_status=pending`.
Provider URLs and temporary provider download tokens are never stored as `artifact_uri`.

Enabled staging attempts are bounded at three. Provider transport failures and HTTP 408,
429, or 5xx responses return the work item to `queued` with 60/120-second delayed
availability. The third recoverable failure becomes terminal. Authentication, malformed
provider payloads, invalid images, media upload/readback, persistence, and stale-claim
failures are never retried. Retry and terminal events store only attempt number, failure
class/stage, delay, and boundary booleans; raw errors and responses are not persisted.

A `processing` claim that expires or loses a complete claim tuple has an unknown
external outcome because the provider or media service may already have accepted the
request. Before selecting queued work, the worker atomically marks one such request
`failed`, clears `claimed_by`, `locked_at`, and `claim_expires_at`, and appends one
`image_generation_outcome_ambiguous` event. The event records only the attempt number,
claim-loss reason, unknown generation/upload booleans, and that automatic retry is
disabled. The same request is never reclaimed automatically.

For Xiaoman activity promotion, an approved `poster_brief` only authorizes an
`image_generation_request`. A completed image request with an approved `generated_image`
is required before the group-message starter can create `group_message_request`.

The artifact review command does not trust `artifact_type=generated_image` alone. An
approval must match the awaiting-review image request, Huabaosi worker marker, stable
HTTPS URI, canonical sha256, PNG dimensions and byte size, source brief/prompt refs,
risk labels, and `generated_image_created` audit. Integrity denial leaves both artifact
and request unchanged and records a sanitized policy event.

## Rollback

Set the generation flag to `0` and stop any future reviewed timer. Retain requests,
artifacts, and audit events for traceability; do not delete historical approval records.
