# Xiaoman Activity Lifecycle Phases

Status: implementation in this PR; production scheduling and external adapters unchanged

Date: 2026-07-15

## Goal

Give Xiaoman activity work one explicit lifecycle phase and one deterministic internal
route. The phase is an AgentOS fact, not a prompt label or a Feishu field.

The allowed phases are:

- `pre_event`: 事前准备和宣发；
- `in_event`: 事中现场信息和变更支持；
- `post_event`: 事后证据整理和复盘内容。

## State Machine

`qintopia_agent_os.event_signals.activity_phase` owns the current phase for
Xiaoman-owned `活动/聚会` signals. New activity signals start at `pre_event`. Existing
activity signals with no stored phase are interpreted as `pre_event` for backward
compatibility.

Allowed transitions:

| Current      | Next         | Decision   |
| ------------ | ------------ | ---------- |
| unset        | any phase    | allow      |
| `pre_event`  | `pre_event`  | idempotent |
| `pre_event`  | `in_event`   | allow      |
| `pre_event`  | `post_event` | allow      |
| `in_event`   | `in_event`   | idempotent |
| `in_event`   | `post_event` | allow      |
| `post_event` | `post_event` | idempotent |

Backward transitions are denied. A direct `pre_event -> post_event` transition is
allowed because AgentOS may first observe an activity after it has finished.

Phase mutation uses the existing event-signal mutation boundary: internal
`event_signal_id`, caller-supplied UUID `mutation_id`, one field per transaction, and
one append-only audit row.

## Route Matrix

Each phase creates a separate idempotent root work item. Previous phase roots remain as
workflow history and are not rewritten.

| Phase        | Route                   | Root work item                  | Internal children |
| ------------ | ----------------------- | ------------------------------- | ----------------- |
| `pre_event`  | `promotion_preparation` | `activity_promotion_request`    | evidence + visual |
| `in_event`   | `live_support`          | `activity_live_support_request` | evidence only     |
| `post_event` | `activity_recap`        | `activity_recap_request`        | evidence + visual |

The existing `run-xiaoman-activity-promotion-starter-worker` name is retained for
runtime compatibility, but its internal route is phase-aware. It may create only the
child capabilities in this table.

The `post_event` visual child produces a reviewable recap brief. After that brief is
approved, the same reviewed internal starter path may create one image-generation
request, and after the generated image is approved it may create one awaiting-publish
group-message request. Those starter steps only create AgentOS work items; they do not
call an image provider, confirm, queue, publish, write Feishu, call QiWe, or send. The
`in_event` route does not create a visual, image-generation, or message-send request.

## Compatibility

- Existing activity signals and root work items without phase metadata remain
  `pre_event`.
- The `pre_event` root keeps the existing idempotency key
  `xiaoman_activity_signal:<event-signal-id>`.
- Later phases add the phase to the root idempotency key so a forward transition creates
  one new root and replay creates no duplicate.
- Existing activity-promotion child keys are retained because the parent UUID already
  separates phase instances.

## Acceptance

- Invalid phases and backward transitions fail before mutation.
- Phase mutation updates one Xiaoman activity signal and appends one audit row
  transactionally.
- Signal intake records both `activity_phase` and `activity_route`.
- The starter creates exactly the allowlisted child set for each phase.
- Replays do not duplicate roots, children, or mutation audit rows.
- No starter route writes Feishu, calls a provider, records final confirmation,
  publishes, or sends QiWe messages.

## Production Boundary

This PR adds an additive database migration and changes internal AgentOS work-item
routing after an explicit phase transition. It does not add or enable a timer, external
adapter, image provider, Feishu write, final confirmation, QiWe send, or profile runtime
change. Rollback uses the previous immutable sidecar; the nullable phase column and
append-only audit rows may remain because older runtimes ignore them.
