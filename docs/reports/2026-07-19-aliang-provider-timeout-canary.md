# Aliang Production Provider Timeout Canary

Date: 2026-07-19 Asia/Shanghai

## Observed Evidence

The owner approved one real community-activity `poster_brief` for the first Aliang
production canary. The production starter created exactly one bound
`image_generation_request`; its parent, approved brief, and idempotency key matched the
reviewed workflow.

Release `v0.2.15` had the production image-generation timer active and passed the
release-local read-only observation. The worker claimed the request three times. Every
attempt ended after approximately 61 seconds with the sanitized classification
`retryable_provider/provider_transport`. The first two attempts used the reviewed 60 and
120 second retry delays. The third exhausted the three-attempt limit and moved the
request to `failed`.

No `generated_image` artifact was created. The worker did not report a Feishu storage
write, approval, publication, or external send. The release-local rollback initially
failed before mutation because the unprivileged operator could not change systemd state;
the same immutable script then ran through non-interactive `sudo` and left the
generation timer `disabled` and `inactive`.

## Root Cause

The shared bounded HTTP client used a fixed 60 second socket timeout. That default is
appropriate for the other short adapter calls, but the real OpenAI-compatible image
provider did not return the bounded image response within that window. The repeated
61-second production durations match the client timeout and not the timer, claim, or
configuration gates.

The image adapter had no reviewed way to select a longer timeout without also changing
the shared default for QiWe, WeCom, Feishu, and other HTTP users.

The canary also proved that a socket timeout after provider request bytes were sent was
classified as retryable. That caused three provider requests even though each timed-out
request had an unknown external generation and billing outcome. A post-send timeout is
not a recoverable connection failure and must fail closed without automatic retry.

## Resolution

- Keep the shared bounded HTTP client default at 60 seconds.
- Give the Huabaosi image adapter an image-only HTTP timeout with a default of 180
  seconds.
- Accept an optional `QINTOPIA_HUABAOSI_IMAGE_HTTP_TIMEOUT_SECONDS` only from 60 through
  240 seconds. The upper bound reserves the five shared-client 60-second Feishu calls
  plus final transform and transaction time inside the fixed 10-minute claim lease.
- Retry only transport failures proven to occur before provider request bytes may have
  been sent. Record a post-send transport or protocol error as an ambiguous provider
  outcome with `external_generation_executed=null` and `automatic_retry_allowed=false`.
- Carry generation and media-write outcomes explicitly through every failure path.
  Record generation as `false` only before provider execution, `true` only after
  accepting a valid provider payload, and `null` when execution is uncertain. Record a
  media write as `false` before storage, `null` for an unprovable upload or Feishu
  write, and `true` only after storage is confirmed by readback or completed
  persistence.
- Validate the timeout during the existing no-network preflight. Invalid values fail
  before Postgres or provider access and are never emitted in chat-safe output.
- Preserve the three-attempt retry limit, retry classifications, idempotency, and
  terminal state. The failed production request is not reset or retried by this change.

## Validation

Local validation completed:

- `cargo fmt --check --manifest-path runtime/sidecar/Cargo.toml`;
- focused all-feature Huabaosi image-generation tests: 48 passed, 0 failed, and 1
  guarded PostgreSQL test ignored by design;
- `cargo test --manifest-path runtime/sidecar/Cargo.toml --all-features`: 411 passed, 0
  failed, and 12 guarded PostgreSQL tests ignored by design;
- warning-denied Clippy with exact production features and with all features;
- `node tools/deploy/check-deploy-contracts.mjs`;
- `node tools/security/check-secrets.mjs`;
- Markdown lint and `git diff --check`; and
- `sh .husky/pre-commit`.

The six fake provider/media tests initially could not bind loopback inside the Codex
sandbox. The unchanged test command passed outside that network sandbox. PR CI and
review are still required on the final head SHA.

## Remaining Boundary

This remediation does not call the provider, write Feishu, create or approve an image,
publish, call QiWe, or send a message. The production generation timer must remain
disabled until the fix is merged, released, deployed, and the release-local preflight
passes.

The terminal failed request must remain immutable audit history. A later production
canary requires a newly approved real brief and a distinct image-generation request. Its
first resulting JPEG must remain `pending` until a human inspects it.

## Follow-Up Owner Action

1. Merge the reviewed timeout fix and manually publish the current Release Please
   Release.
2. Confirm production deploy and release-local observation on the new immutable SHA.
3. Approve one new real canary brief, activate the fixed timer, and inspect the first
   pending Feishu-backed JPEG.
4. Roll back the timer immediately on another provider, cost, storage, integrity, or
   claim anomaly.
