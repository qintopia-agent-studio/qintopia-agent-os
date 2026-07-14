# 2026-06-30.007 Operations Control Plane

## Purpose

This migration adds the first AgentOS operations control plane. The goal is to support
multi-agent operations workflows such as:

- Xiaoman requesting Huabaosi visual material drafts.
- Xiaoman requesting Erhua to send an approved activity poster to an allowlisted group.
- Xiaoman or Huabaosi requesting Wenyuange evidence before drafting or sending.

This is intentionally broader than a single poster workflow. `visual_asset_request` is
the first golden path, not the only supported operating-system capability.

## Scope

New tables under `qintopia_agent_os`:

- `capabilities`: controlled Agent capability registry.
- `work_items`: claimable operating work across Agents and humans.
- `artifacts`: intermediate or final outputs created by work items.
- `work_item_events`: append-only audit trail for state changes, denials, approvals,
  failures, and external-send results.
- `human_workbench_refs`: references to human-facing surfaces such as Feishu Task,
  Feishu Base, or Feishu docs.

Seeded capabilities:

- `huabaosi.create_visual_asset`
- `erhua.send_group_message`
- `wenyuange.retrieve_evidence`
- `xiaoman.create_activity_request`

`operations-capability-list` exposes the same registry as a safe discovery surface for
non-technical request flows. It can run from the built-in offline registry before
deployment, or from Postgres with `--use-db` after migration. The list does not grant
permission by itself; every execution still creates a policy-checked work item.

`operations-request-plan` is the first deterministic request-planning surface above the
registry. It accepts a short non-technical request, maps clear intent to a registered
capability, and returns a dry-run work-item plan. It does not use LLM free-form routing
and does not execute the work item. Ambiguous or under-specified requests return
clarification questions.

`operations-request-submit` uses the same deterministic planner and then routes the
resulting request through the normal work-item creation policy. Dry-run mode previews
the result. Apply mode writes `work_items` and `created` events, but it does not run
collaboration workers, create Feishu Tasks, publish artifacts, or send group messages.
Ambiguous requests remain `needs_clarification` and are not created.

`operations-workflow-start` is the first workflow-level starter above individual work
items. v1 supports `activity_promotion`, creating one parent
`activity_promotion_request` and initial `evidence_request` plus `visual_asset_request`
child work items. Parent and child creation still run through capability policy and
idempotency. The command does not execute workers, create Feishu Tasks, publish
artifacts, or send group messages. It intentionally does not pre-create
`group_message_request` because that high-risk step must wait for an approved artifact
and human final confirmation.

`run-evidence-worker` is the first worker for `wenyuange.retrieve_evidence`. It claims
queued `evidence_request` work items, creates an internal `evidence_summary` artifact
with `review_status=not_required`, marks the work item `completed`, and appends
`evidence_artifact_created`. The current implementation is a controlled skeleton: it
records `external_calls_executed=false` and does not query live message-store, Feishu,
Wenyuange, or web adapters.

## Ownership

Postgres remains the system source of truth. Feishu Task boards, Base tables, and docs
are human workbenches only. Hermes remains the Agent runtime. Hermes Kanban is not used
for new operations workflows.

`qintopia-message-sidecar` owns the schema and repository/service-side policy.
Profile-local Hermes tools may request work items, but they must not bypass the
capability and work item layer.

The first human workbench worker is a Feishu Task mirror payload generator. It uses
provider `feishu_task_dry_run`, writes only allowlisted fields to
`human_workbench_refs`, and records `mirror_dry_run_recorded`. Immediate child refs are
kept for compatibility; a separate descendant summary includes every nested work item
with its direct parent and depth. It does not call Feishu Task APIs or treat Feishu as a
source of truth. The dry-run description caps traversal at depth 8 and 32 refs and
records an explicit truncation flag instead of producing an unbounded task description.

`operations-workbench-event-record` is the matching controlled intake path for future
Feishu Task comments, section changes, review requests, and final confirmation requests.
Apply mode requires an existing active `human_workbench_refs` row for the same
`provider`, `external_id`, and `work_item_id`, then appends
`human_workbench_event_recorded`. It does not mutate `work_items` or `artifacts`
directly; artifact review decisions and group message final confirmations remain
separate policy-checked commands. `operations-workbench-event-process` can process one
recorded event by `work_item_events.id` when the event has a dedicated policy path. v1
supports `review_decision_requested`, `final_confirmation_requested`, controlled
`status_change_requested` cancellations, `owner_changed`, and `attachment_added`. Review
and confirmation events delegate to `operations-artifact-review-decision` or
`operations-group-message-confirm`; status changes can only move non-terminal work items
to `cancelled` with a human reason; owner changes update `work_items.human_owner` from
`metadata.new_human_owner` after validation; attachments create `workbench_attachment`
artifacts with `review_status=pending` and do not publish or send anything. Completion,
queueing, publish, and processing status changes from the workbench are rejected rather
than trusted as AgentOS facts. Successful processing appends
`human_workbench_event_processed` for idempotency. Generic comments remain audit-only.
`run-workbench-event-worker --once` is the worker form of the same policy. It selects
the oldest unprocessed processable workbench event unless `--event-id` is provided,
processes only one event per run, and never calls Feishu, QiWe, or external publish
adapters. The server deployment script installs an optional systemd oneshot service and
timer for this worker; the timer only processes events that were already recorded into
Postgres by the guarded intake path.

`operations-workflow-sync` persists a recursive workflow parent summary after descendant
work items change. It reads the same status tree as `operations-work-item-status`, keeps
immediate child refs for compatibility, adds every descendant with its direct parent and
depth, writes `workflow_summary` into the parent `work_items.metadata`, updates the
parent aggregate status, and appends `workflow_status_synced`. It does not execute
workers, schedule a general DAG, create Feishu Tasks, publish artifacts, or send group
messages.

`run-workflow-sync-worker --once` is the worker form of the same operation. It selects
the oldest syncable `activity_promotion_request` parent unless a specific
`--work-item-id` is supplied, then calls the same policy and write path as
`operations-workflow-sync`. It still does not execute child workers or call external
systems.

## Privacy and Safety Boundaries

Work item payloads must contain redacted summaries, source ids, risk labels, and
structured references. They must not contain raw private group text, member dossier
text, app secrets, access tokens, Base app tokens, table ids, or raw system prompts.

Work item creation, request planning, and workflow start all require an allowlisted
`source_type` plus structured `source_refs`. v1 allows only `manual_request`,
`apply_smoke`, `xiaoman_activity`, `event_signal`, and `operations_workflow`.
`manual_request`, `apply_smoke`, `xiaoman_activity`, and `operations_workflow` must
include `source_refs.source_record_ref`; `event_signal` must include an event-signal id
either in `source_refs` or the dedicated `source_event_signal_id` field. Unknown sources
such as `daily_digest`, Hermes Kanban cards, arbitrary URLs, and raw chat-derived input
are rejected before capability execution.

Workbench mirror descriptions are allowlisted to stable operational fields:
`work_item_id`, `work_item_type`, `capability_key`, requester/target agents, human
owner, sanitized source refs, risk/review policy, artifact counts, immediate child refs,
recursive descendant refs, and current status. Raw payload, internal prompts, Base table
ids, tokens, and raw private text are rejected rather than mirrored.

External-send capabilities such as `erhua.send_group_message` are high risk. They
require approved artifacts, allowlisted targets, human final confirmation, bounded
retries, and audit events.

Human review, final confirmation, workbench owner assignment, and workbench attachment
intake can also be constrained by service-side allowlists. When
`QINTOPIA_OPERATIONS_ALLOWED_REVIEWER_IDS` is non-empty, artifact review decisions must
come from a listed reviewer. When `QINTOPIA_OPERATIONS_ALLOWED_CONFIRMER_IDS` is
non-empty, group-message final confirmation must come from a listed confirmer. When
`QINTOPIA_OPERATIONS_ALLOWED_OWNER_IDS` is non-empty, `owner_changed` can only assign
`metadata.new_human_owner` to a listed owner. When
`QINTOPIA_OPERATIONS_ALLOWED_ATTACHMENT_HOSTS` is non-empty, `attachment_added` can only
create a pending artifact from an HTTPS `metadata.attachment_uri` whose normalized host
is listed. Empty allowlists preserve local dry-run ergonomics, but production should
configure explicit identities and attachment hosts. In apply mode,
reviewer/confirmer/owner/attachment-host allowlist denials append
`work_item_events.denied_by_policy` with `policy` metadata before the command returns
the policy error.

New `group_message_request` work items for `erhua.send_group_message` should start in
`awaiting_publish` rather than `queued`. That keeps approved-but-not- finally-confirmed
sends out of claimable worker queues until a separate human confirmation path records
the decision and intentionally promotes the work item. The confirmation path records an
audit event and changes state only; it does not execute the external send. A later Erhua
send worker must claim the queued item and write the actual send result as another
`work_item_events` entry. The first send worker implementation is intentionally a
send-readiness worker: it validates the queued item and records
`group_message_send_ready_recorded` with `send_executed=false`. It does not call QiWe or
the Erhua external-send adapter. The server deployment script installs an optional
systemd oneshot service and timer for this send-readiness worker; the timer still
records audit state only and never performs the production send. Already-ready work
items are skipped by checking for an existing `group_message_send_ready_recorded` event
with `send_executed=false`, so periodic runs do not duplicate the audit entry. The
worker increments `work_items.attempts` on claim and stops claiming send-ready requests
once attempts reach 3, providing a bounded retry guard before the production external
send adapter exists. A successful send-ready transition and a policy-denied terminal
transition both clear `claimed_by`, `locked_at`, and `claim_expires_at` in the same
transaction. The worker must update exactly the row it locked before appending either
audit event, so persisted claim ownership never outlives the claim lease.

## Compatibility

The migration is additive:

- Existing message capture, event signal, digest, profile, and knowledge tables are
  unchanged.
- Existing Xiaoman activity wrappers can continue to return validation-only reports
  until their apply path is connected to `work_items`.
- Existing Feishu apps for Xiaoman and Huabaosi remain valid; this migration does not
  introduce a replacement app identity.

## Follow-up Work

1. Add a service module that validates capability requests and writes `work_items` plus
   `work_item_events`.
2. Connect Xiaoman `handoff-create --apply` to `huabaosi.create_visual_asset`.
3. Add Feishu Task mirror/sync workers using `human_workbench_refs`.
4. Add collaboration workers that claim work items and create `artifacts`.
5. Add the production external-send adapter for `erhua.send_group_message` after
   allowlisted group, final confirmer, retry, and rollback policy are accepted.

## Acceptance Checks

- `scripts/operations-control-plane-smoke.sh` is the no-credential local smoke. It
  validates discovery, planning, work-item dry-runs, fixture workers, review, final
  confirmation, send-readiness, workbench mirroring, and negative guardrails without
  reading Postgres or calling external systems. It also validates
  `operations-readiness-check` JSON output and strict failure for missing production
  allowlists. The negative checks include unknown source types, missing required source
  references, sensitive payloads, Hermes Kanban/raw prompt bypass attempts, and
  non-allowlisted group sends.
- `scripts/operations-control-plane-apply-smoke.sh` is the guarded Postgres apply smoke.
  It requires `QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE=1` and
  `QINTOPIA_SIDECAR_DATABASE_URL`, runs migrations, verifies the DB-backed capability
  registry, replays a sanitized Xiaoman activity signal through
  `xiaoman-activity signal-ingest --apply`, verifies the resulting
  `xiaoman.create_activity_request` control-plane row and idempotent replay behavior,
  creates one planned `visual_asset_request` through `operations-request-submit`,
  confirms planner-submit idempotency, starts an `activity_promotion` workflow with
  parent/evidence/visual work items through `operations-workflow-start`, confirms
  workflow idempotency, runs the evidence worker for the workflow-created
  `evidence_request`, runs the collaboration worker for the workflow-created
  `visual_asset_request` to create a pending `poster_brief` artifact and move the work
  item to `awaiting_review`, records an approved artifact review without publishing or
  sending and marks the visual work item completed, creates a controlled child
  `erhua.send_group_message` request from the approved artifact on the same parent,
  records final confirmation, records send-readiness without sending, verifies
  unapproved artifacts cannot create group-message requests, verifies the evidence,
  visual, and group-message child work items share one parent
  `activity_promotion_request`, reads that parent status tree and current blocking
  point, syncs the durable parent workflow summary with `workflow_status_synced`,
  records a dry-run Feishu Task workbench reference with safe immediate-child and
  recursive-descendant status summaries and the current blocking point, records a human
  workbench event against that mirror reference without mutating work item status,
  confirms duplicate external workbench event ids are idempotent, records and processes
  review-request, final-confirmation, controlled cancellation status-change,
  owner-change, and attachment workbench events through `run-workbench-event-worker`,
  verifies processing idempotency, and confirms policy denial audit events. The script
  uses `run-collaboration-worker --work-item-id`, `run-evidence-worker --work-item-id`,
  `operations-workflow-sync --work-item-id`, `run-workflow-sync-worker --work-item-id`,
  `run-group-message-send-worker --work-item-id`, and
  `run-workbench-mirror-worker --work-item-id` so it processes only the smoke work item.
  It does not call Feishu, QiWe, Huabaosi, Wenyuange, or external send/publish adapters.
- The apply smoke also verifies that configured reviewer/confirmer/owner allowlists
  reject unauthorized human actors, append `denied_by_policy`, and avoid the requested
  review/confirmation/owner state mutation.
- The apply smoke verifies that configured attachment host allowlists reject
  non-allowlisted workbench attachment URLs, append `denied_by_policy`, and do not
  create processed events or attachment artifacts for the denied event.
- The apply smoke verifies the send-readiness retry boundary: a claimed group send
  request increments `attempts`, and a request already at 3 attempts is not claimed or
  marked send-ready.
