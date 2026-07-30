# QiWe Callback Report And Xiaoman Evidence Contract Drift

Date: 2026-07-29

## Summary

The owner-approved real Xiaoman image-send exercise on release `v0.2.56` completed the
QiWe asynchronous upload callback and `/msg/sendImage`. PostgreSQL recorded the selected
request as `completed` and the matching attempt as `sent`, but the Erhua callback bridge
then rejected the successful Rust report and returned a processor failure to QiWe.

The sanitized seven-phase production evidence export also completed, but its repository
checker rejected the generated-image phase even though the JPEG, Feishu storage, review,
and send fields matched the reviewed production contract.

## Sanitized Evidence

- The Rust callback report used `action_status=image_send_completed`,
  `callback_received=true`, and `external_send_executed=true`.
- The report contained the canonical final JPEG `artifact_content_hash`; it contained no
  callback credentials, provider response, media URL, group id, token, or message id.
- The bridge returned `processor_failed` only after the Rust process had completed the
  external send and committed the terminal state.
- The production evidence exported one pending `1254x1254` Feishu-backed JPEG and bound
  the same content hash through human approval, send readiness, upload, callback, and
  retention.
- The generated-image artifact id was a canonical UUID v8 produced by the repository's
  deterministic content-bound id function.

## Root Cause

Two independently maintained consumers had drifted behind producer contracts:

1. `skills/qiwe/image_callback_bridge.py` required an exact Rust report key set that did
   not include `artifact_content_hash`, although the Rust callback success path emits
   the field to bind the final approved JPEG identity.
2. The Xiaoman production evidence checker accepted only UUID versions 1 through 5,
   while `generated_image_artifact_id` intentionally creates deterministic UUID v8 ids.
   The same stale UUID rule also existed in the Huabaosi canary and final Xiaoman
   completion checkers.

## Resolution Boundary

- Keep the bridge report schema closed to unknown fields, but allow the single canonical
  `artifact_content_hash` field.
- Require a lowercase `sha256:` content hash for callback outcomes that the Rust
  producer binds to an artifact; reject missing or malformed values for completed and
  rejected send requests.
- Do not return the content hash through the webhook result or expose callback values.
- Accept standard UUID versions 1 through 8 in the three production evidence checkers.
- Use UUID v8 generated-image ids in the affected fixtures so tests match the real
  producer contract.
- Do not change QiWe request construction, send state transitions, timers, database
  schema, target routing, or external retry behavior.

## Acceptance

1. The Python bridge accepts the real Rust success report shape and still rejects
   unknown fields, malformed content hashes, missing required hashes, inconsistent
   outcomes, oversized output, and callback secret leakage.
2. The retained `v0.2.56` real-activity production evidence passes its checker without
   editing the evidence file.
3. Huabaosi canary, real-activity, group-arrival, completion, manifest-builder, and
   finalizer contract tests pass with generated-image UUID v8 fixtures.
4. `pnpm test:qiwe`, `pnpm deploy:contracts:check`, and the repository PR auto tier
   pass.
5. The loopback-binding Rust tests are rerun outside the restricted sandbox, as required
   by `runtime/sidecar/AGENTS.md`.

## Production Follow-Up

After merge and an owner-published Release, deploy the immutable release, confirm the
Erhua bridge is bound to its reviewed QiWe production companion, and use a fresh bounded
callback exercise to prove the webhook returns success after the already terminal Rust
send result. Do not retry the completed `v0.2.56` request or enable the production timer
until the remaining claimable backlog has been reviewed.
