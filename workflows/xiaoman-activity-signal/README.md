# Workflow: Xiaoman Activity Signal

`workflows/xiaoman-activity-signal` defines how Xiaoman turns activity signals into
reviewable Agent OS state.

## Responsibility

- Detect activity signals and map them to activity records.
- Track status transitions without treating Feishu as the system source of truth.
- Trigger downstream work requests only when required fields are present.
- Keep Xiaoman's path read-only or database-scoped until owner-reviewed runtime config
  enables more behavior.

## Current Status

This workflow is active for the AgentOS-only production preflight path:
`event_signals -> activity request -> evidence/visual children -> internal artifacts -> awaiting-publish group message request`.
The remaining production gate is an owner-approved, read-only aggregate preflight run
recorded in `deploy/smoke/docs/xiaoman-production-preflight-record.md`. Passing that
gate requires sanitized observation output with `safe_for_chat=false` where present and
still does not approve Feishu writeback, QiWe sends, poster publishing, real Wenyuange
retrieval, or Huabaosi production generation.

## Signal Intake Contract

The sidecar entrypoint is `xiaoman-activity signal-ingest`. It accepts structured
activity signals from `qintopia_agent_os.event_signals` or sanitized replay fixtures and
turns them into a `xiaoman.create_activity_request` work item preview or apply write.

Required payload fields:

- `actor_agent`: must be `xiaoman`.
- `operation`: must be `signal-ingest`.
- `event_signal_id`: stable event-signal id or sanitized fixture id.
- `signal_type`: expected to be an activity signal such as `活动/聚会`.
- `activity_title`: short activity title.
- `signal_date`: activity signal date in `YYYY-MM-DD`.

Optional fields:

- `chat_id`, `source_message_ids`, `owner_name`, `priority`, `location`,
  `brief_summary`, `gap_summary`, and `related_member_names`.

The worker builds a stable idempotency key from `event_signal_id` and writes through the
existing operations control plane. Replays of the same signal must return the existing
work item instead of creating duplicates. If required fields are missing, the worker
still produces an operations work item preview with `review_needed` metadata and does
not trigger downstream visual, evidence, or send work.

Signal replay fixtures under `fixtures/xiaoman/` carry an `expected` block that defines
the acceptance contract for `signal-ingest`: status, capability routing, idempotency,
review-needed fields, and the no-external-send boundary. `pnpm workflows:check`
validates that static contract, and `pnpm check:runtime` runs the same fixtures through
the sidecar smoke.

The guarded Postgres apply smoke seeds a matching `qintopia_agent_os.event_signals` row,
replays its UUID through `xiaoman-activity signal-ingest --apply`, verifies that it
creates exactly one `xiaoman.create_activity_request` work item, verifies that the work
item stores `source_event_signal_id`, and verifies that replaying the same signal
returns the existing work item by idempotency key.

After an owner-approved deploy, the guarded
`deploy/sidecar/scripts/xiaoman-activity-signal-timer-observation-smoke.sh` checks that
the runtime timer is active, that its service command is fixed to
`run-xiaoman-activity-signal-worker --once --apply`, that recent journal output does not
leak known sensitive markers, and that `run-xiaoman-activity-signal-worker --check-only`
can preview the current AgentOS queue. It is a read-only production observation smoke.

`run-xiaoman-activity-promotion-starter-worker` completes the next AgentOS-only step: it
scans existing `xiaoman.create_activity_request` work items and creates only missing
`evidence_request` and `visual_asset_request` child work items under the same parent. It
does not execute evidence retrieval, visual generation, Feishu writes, QiWe sends, or
group-send readiness. It may be scheduled by the reviewed
`qintopia-agentos-xiaoman-activity-promotion-starter-worker.timer`, whose service
command is fixed to `run-xiaoman-activity-promotion-starter-worker --once --apply`.

`deploy/sidecar/scripts/xiaoman-activity-downstream-observation-smoke.sh` is the
read-only production-readiness check for the evidence and visual worker previews. It
runs the existing evidence and visual workers in dry-run mode only:
`run-evidence-worker --once --dry-run` and
`run-collaboration-worker --work-item-type visual_asset_request --once --dry-run`. The
smoke proves the child queues can be previewed without writing Postgres, reading or
writing Feishu, calling QiWe, generating posters, or sending externally.

The downstream evidence and visual workers may also be scheduled by the reviewed runtime
deployment path. `qintopia-agentos-operations-evidence-worker.timer` runs
`run-evidence-worker --once --apply` to create internal `evidence_summary` artifacts,
and `qintopia-agentos-operations-visual-worker.timer` runs
`run-collaboration-worker --work-item-type visual_asset_request --once --apply` to
create pending `poster_brief` artifacts. These timers still do not call live Wenyuange
search, Huabaosi production generation, Feishu, QiWe, poster publishing, or external
send adapters.

`run-xiaoman-activity-send-request-starter-worker` adds the next AgentOS-only handoff:
after a Xiaoman visual child has a reviewed `approved` `poster_brief`, it creates one
missing `erhua.send_group_message` / `group_message_request` child under the same
activity parent. The new child starts at `awaiting_publish`, references the approved
artifact, and records `send_executed=false`. It does not record final confirmation, move
the request to `queued`, run send-ready, publish, call QiWe, write Feishu, or call
external adapters.

`deploy/sidecar/scripts/xiaoman-activity-send-request-starter-observation-smoke.sh`
checks this handoff after an owner-approved deploy. It verifies that the reviewed timer
command is fixed to `run-xiaoman-activity-send-request-starter-worker --once --apply`,
inspects recent journal output for known sensitive markers, and runs
`run-xiaoman-activity-send-request-starter-worker --check-only` to verify the sanitized
report shape without writing.

`deploy/sidecar/scripts/xiaoman-activity-production-preflight-smoke.sh` is the aggregate
read-only production preflight for this path. It composes the Xiaoman signal timer
observation, promotion starter timer observation, shared evidence/visual timer
observation, Xiaoman downstream evidence/visual preview, send request starter
observation, and group send-ready timer observation. It does not run the send-ready
worker, deploy, write Feishu, call QiWe, publish, or send externally.

`xiaoman-activity shadow-validate` is a guarded, read-only Feishu shadow check. It reads
the allowlisted Feishu activity Base and the same-date AgentOS `event_signals`, compares
coverage by normalized activity title and date, and reports sanitized
`missing_in_agentos` / `missing_in_feishu` lists. It does not write Feishu, write
Postgres, send QiWe messages, or make Feishu the source of truth.

`run-xiaoman-activity-signal-worker` scans eligible Xiaoman `event_signals` and submits
the same `signal-ingest` work item contract in batches. `--check-only` previews the
batch without writes; `--once --apply` creates missing AgentOS work items. It may be
scheduled by the reviewed `qintopia-agentos-xiaoman-activity-signal-worker.timer`, whose
service command is fixed to `run-xiaoman-activity-signal-worker --once --apply`. The
timer does not write Feishu, send QiWe messages, create visual assets, or call external
send adapters.

## Feishu Write Boundaries

There are two Feishu-writing paths today, and they have different jobs:

- The QiWe solitaire activity path parses activity/registration messages and writes a
  configured Feishu activity table through `FeishuActivityWriter`. This is the activity
  ledger path for activity records, participant counts, status mapping, and table-level
  defaults.
- The Xiaoman event radar path writes daily digest views through the sidecar publisher:
  `event_signals` becomes `事件信号表`, and daily digest/archive rows become `日报总表`
  / `文档归档表`.

The activity ledger can be a useful human workbench, but AgentOS still treats Postgres
`event_signals` and `work_items` as the workflow source of truth. Shadow validation
exists to compare the ledger mirror against AgentOS coverage, not to infer facts from
raw Feishu record ids.

## Production Boundary

- This workflow can write Agent OS control-plane rows after the sidecar contract is
  used.
- It must not directly send external messages.
- It must not create visual assets or group-send drafts by itself.

## Acceptance Scenarios

- New activity signal creates one `xiaoman.create_activity_request` work item through
  the operations control plane.
- Runtime scheduling can turn the activity request into missing evidence and visual
  child work items without manual CLI runs.
- Downstream observation can preview evidence and visual worker consumption without
  applying artifact writes or calling external systems.
- Downstream runtime scheduling can turn child work items into internal
  `evidence_summary` and pending `poster_brief` artifacts without external adapters.
- Approved Xiaoman `poster_brief` artifacts can create one awaiting-publish
  `group_message_request` without final confirmation, queueing, send-ready, or external
  sends.
- Runtime scheduling can create awaiting-publish `group_message_request` work items from
  approved Xiaoman `poster_brief` artifacts without external adapters.
- Activity request starter creates missing evidence and visual child work items without
  duplicating existing children.
- Duplicate signal returns the existing work item by idempotency key.
- Missing required fields produce a review-needed state.
- Valid signal can request a visual asset workflow without publishing anything.

## Source References

The following Feishu wiki pages are related product and operations references. They are
linked here as source references only; Agent OS state still comes from Postgres and
reviewed monorepo contracts.

- [NCvZwwomEio1Xmkvgl3c4YpAnmh](https://ranuox3qst4.feishu.cn/wiki/NCvZwwomEio1Xmkvgl3c4YpAnmh?from=auth_notice&hash=e8563a1c58fdd146fcfb23d0a2988f67)
- [SmPbwnVpsiuJC4kjx4ncq4S4nr6](https://ranuox3qst4.feishu.cn/wiki/SmPbwnVpsiuJC4kjx4ncq4S4nr6?from=auth_notice&hash=5fc546f98f8a8c8396eefb1c4c155c78)
- [XQ5BwtpjwiX0XrkApp7cwneenCb](https://ranuox3qst4.feishu.cn/wiki/XQ5BwtpjwiX0XrkApp7cwneenCb?from=auth_notice&hash=e1f6b934be2a99f22384f72f4c7501a0)
- [FCIFwg7j6iTEblk33DGcvdyqnaf](https://ranuox3qst4.feishu.cn/wiki/FCIFwg7j6iTEblk33DGcvdyqnaf?from=auth_notice&hash=91bd5991ce02dafa2c49d4fb9cc57284)
- [HmNnwztw7ihdT5kG7zrcVCGonXd](https://ranuox3qst4.feishu.cn/wiki/HmNnwztw7ihdT5kG7zrcVCGonXd?from=auth_notice&hash=6eb664e6468a49ba140f3c18183d0e0e)
- [QQxvwnBBiiEkOnkpCiMcuNRinNf](https://ranuox3qst4.feishu.cn/wiki/QQxvwnBBiiEkOnkpCiMcuNRinNf?from=auth_notice&hash=494ce31860515da8b2e2131aa5f8e867)

## Validation

```bash
pnpm workflows:check
pnpm check:runtime
deploy/sidecar/scripts/xiaoman-activity-shadow-read-smoke.sh
QINTOPIA_XIAOMAN_ACTIVITY_DOWNSTREAM_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/xiaoman-activity-downstream-observation-smoke.sh
QINTOPIA_XIAOMAN_ACTIVITY_SEND_REQUEST_STARTER_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/xiaoman-activity-send-request-starter-observation-smoke.sh
QINTOPIA_XIAOMAN_ACTIVITY_PRODUCTION_PREFLIGHT_ENABLE=1 deploy/sidecar/scripts/xiaoman-activity-production-preflight-smoke.sh
bash -n deploy/sidecar/scripts/operations-control-plane-apply-smoke.sh
```
