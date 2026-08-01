# Xiaoman Conversation Poster Participants V3

Date: 2026-08-01

## Decision

The second conversation-aware poster increment activates the policy and participant
foundation introduced by `202608010001_xiaoman_conversation_ingress_v3.sql`. AgentOS
derives the conversation from the authenticated ingress receipt, snapshots workflow
mutation authority, and keeps conversation notifications separate from publication.

## Schema

- A partial unique index on `poster_workflow_participants(workflow_root_id)` for the
  `requester` role guarantees one requester snapshot per V3 workflow.
- `poster_revision_requests.first_revision_guarded` defaults to `false` so historical V2
  rows remain unchanged. New runtime writes set it to `true`, and a partial unique index
  on guarded `source_artifact_id` values makes the generated image artifact the
  first-valid-revision boundary without rejecting a database that already contains
  multiple historical revisions. Existing source-message uniqueness remains for retry
  deduplication.
- `xiaoman.notify_conversation` is a registered internal-notification capability for a
  trusted direct conversation or internal collaboration thread. The existing
  `xiaoman.notify_direct_conversation` capability remains for V2 compatibility.

All participant and return-target tables remain revoked from `PUBLIC`.

## Runtime Invariants

- The intake payload cannot supply conversation type, audience, return target, policy,
  reviewer, or provider.
- V3 direct and group requests are resolved from the exact persisted message, receipt,
  and policy version. A group request additionally requires the persisted human sender,
  explicit Bot mention, trigger flag, and thread root.
- Workflow identity is derived from platform, source message, and poster capability.
  Return targets are message-scoped and immutable.
- The requester and policy reviewers are copied to `poster_workflow_participants` only
  after the idempotent workflow root exists. A retry completes the same snapshot and
  never creates another workflow.
- Group members in the same authorized conversation may read status. Only the requester
  or snapshotted reviewers may review or revise group work. Direct work remains
  requester-only.
- Revision work uses an artifact-scoped idempotency key and persists the winner from the
  actual created work item. Concurrent or restarted requests cannot create a second
  image request for the same source image.
- Historical unguarded revision rows stay in place. New writes are always guarded, and
  an existing historical winner prevents a new automatic revision for that image.
- Work-item creation handles an idempotency conflict inside its transaction, reloads the
  committed winner, and verifies its immutable capability, parent, source, payload, and
  policy bindings before returning it. A reused key with different bindings fails closed
  instead of routing work to an unrelated item.
- Each image notification already has one final review-action boundary. Duplicate or
  conflicting callbacks are audited as no-op rejections; they do not change the first
  decision.
- `待你审稿` requires a delivered image notification whose artifact is still pending; an
  accepted modification cannot be masked by an older delivered review card.
- Conversation notification work never creates `group_message_request`, `send_executed`,
  or `external_published` facts.

## Compatibility And Rollback

V2 records keep their direct target and requester-only behavior. The new indexes and
capability are additive and may remain during rollback. Disable internal-group ingress
before stopping workers; preserve policies, participants, targets, revisions,
notifications, and rejection audit events.
