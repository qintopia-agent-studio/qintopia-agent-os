# Workflow: Xiaoman Activity Signal

`workflows/xiaoman-activity-signal` defines how Xiaoman turns activity signals into
reviewable Agent OS state.

## Responsibility

- Detect activity signals and map them to activity records.
- Track status transitions without treating Feishu as the system source of truth.
- Trigger downstream work requests only when required fields are present.
- Keep Xiaoman's path read-only or database-scoped until owner-reviewed runtime config
  enables more behavior.

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

## Production Boundary

- This workflow can write Agent OS control-plane rows after the sidecar contract is
  used.
- It must not directly send external messages.
- It must not create visual assets or group-send drafts by itself.

## Acceptance Scenarios

- New activity signal creates one `xiaoman.create_activity_request` work item through
  the operations control plane.
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
```
