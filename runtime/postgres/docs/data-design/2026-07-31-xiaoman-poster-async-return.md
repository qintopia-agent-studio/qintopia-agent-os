# Xiaoman Poster Async Return

Date: 2026-07-31

## Decision

An explicit poster-generation request in a trusted Xiaoman Feishu direct session is both
the workflow intake and the authorization to generate from the resulting source-grounded
poster brief. It is not authorization to publish or send to a group.

Postgres remains the fact source. Hermes supplies per-call session context, the Xiaoman
plugin removes target selection from model arguments, and the sidecar persists only an
opaque origin reference in ordinary work-item metadata. Raw Feishu target identifiers
remain in `poster_return_targets`, which is revoked from `PUBLIC`.

## Tables

- `poster_return_targets` resolves an opaque direct-conversation reference inside the
  delivery boundary.
- `poster_notifications` provides restart-safe, idempotent delivery state for
  `image_ready`, `generation_failed`, and `generation_ambiguous`. Failure and ambiguous
  notifications intentionally have no image artifact; `ambiguous` is terminal until
  human reconciliation.
- `poster_notification_attempts` persists the at-most-once upload and card-send boundary
  before any external write.
- `poster_review_actions` binds exactly one authenticated approve, modify, or abandon
  decision to each delivered notification. Replayed Feishu event ids for the same
  notification, artifact, actor, and decision are idempotent; any changed binding is
  rejected.
- `poster_revision_requests` links a trusted follow-up instruction to the next image
  request in the same workflow.

## Invariants

- Only `feishu` plus `direct` targets are accepted.
- Model arguments cannot provide target, requester, source message, reviewer, or
  authorization fields.
- A generated image and its `image_generation_request` remain `pending` and
  `awaiting_review` respectively until a review callback is recorded.
- Only a delivered `image_ready` notification can accept a review callback. Failure and
  ambiguous status messages have no review actions.
- Notification delivery cannot create `group_message_request`, `send_executed`, or
  `external_published` facts.
- Automatic and scheduled activity workflows keep the existing poster-brief review
  requirement.
- A direct-generation visual work item is not claimable until its fact gate is
  `complete`, its facts come from the originating request or a trusted activity record,
  and both missing and conflicting field lists are empty. The brief worker revalidates
  the same invariant before recording automatic authorization.
- An expired `uploading` or `sending` attempt becomes terminal `ambiguous` and is never
  automatically reclaimed or resent.

## Rollback

Disable the intake, notification starter, delivery worker, and callback processor before
rolling back code. The additive tables may remain for audit and restart safety; do not
drop rows while a notification is pending, claimed, or ambiguous.
