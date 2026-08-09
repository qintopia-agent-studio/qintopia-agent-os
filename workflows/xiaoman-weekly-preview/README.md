# Workflow: Xiaoman Weekly Activity Preview

`workflows/xiaoman-weekly-preview` replaces the old natural-language Monday cron task
with a deterministic, reviewable script.

## Responsibility

- Read the next 7 days of Xiaoman activity records (Monday through Sunday).
- Filter out temporary meals and activities missing required fields (title / time /
  location / owner).
- Produce a single "下周活动预告" draft plus a human-review message.
- Never send, publish, write Feishu, or call Erhua/QiWe. A human confirms first.

## Why this exists

The previous Monday task stored a natural-language prompt in the server `jobs.json` and
let the model improvise every run, which was unstable and unauditable. This workflow
makes the Monday preview a fixed code path: the same input always yields the same draft,
and the only human decision left is "发 / 改".

## How it works

`weekly_preview.py` calls `qintopia_xiaoman_activity_announcement_prepare` with
`mode=weekly_preview`:

1. The Monday `date` is expanded to the full Mon–Sun window.
2. For each day it reads both `activity_plan` and `activity_occurrence` tables through
   the read-through path, then deduplicates by `record_ref`.
3. `_xiaoman_activity_missing_fields` drops activities without title / time / location /
   owner; temporary meals are skipped.
4. The draft and an `operator_review_message` are returned. If the week is empty, the
   output clearly says "下周暂无已确认活动，暂不生成预告" instead of sending stale text.

## Running it

The script runs under the Xiaoman runtime profile with read-through enabled:

```bash
export QINTOPIA_PROFILE_ID=xiaoman
export QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE=1
export QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE=1
# plus the Feishu base token / table id env the read-through worker needs

# preview for this week's Monday (defaults to today's Monday)
python workflows/xiaoman-weekly-preview/weekly_preview.py

# preview for an explicit Monday, full JSON output
python workflows/xiaoman-weekly-preview/weekly_preview.py --date 2026-08-10 --json
```

The script prints the `operator_review_message` (or the full JSON with `--json`). A
human reads it, replies "发" in the operations chat, and only then is the text handed to
Erhua for delivery.

## Acceptance

- Running with an empty week prints "下周暂无已确认活动，暂不生成预告" and exits 0.
- Running with activities returns `publishable_count`, `skipped_count`,
  `missing_followups`, and `operator_review_message`.
- `external_send_executed` is always `false`; `requires_human_confirmation` is always
  `true`.
- The script fails closed (non-zero exit) if read-through is not enabled or the week
  cannot be read.

## Production Boundary

- Reads Feishu activity tables; does not write them.
- Produces a draft only; never sends externally.
- Requires the same secrets as the Xiaoman read-through path.

## Production Activation

This workflow replaces the legacy natural-language Monday cron task. The release bundle
installs the systemd unit disabled by default; activation is still a separate,
owner-approved production action:

- Runbook:
  [`docs/operations/xiaoman-weekly-preview-cutover-runbook.md`](../operations/xiaoman-weekly-preview-cutover-runbook.md)
- It registers `qintopia-agentos-xiaoman-weekly-preview.timer` as a release-managed
  systemd timer (not a conversation-created cron) and keeps the human confirmation gate.
- Activation fails if `xiaoman-legacy-cron-observation-smoke.sh` finds any runtime cron
  declaration in the live Xiaoman Hermes `jobs.json`.
- Enabling or rolling back the timer must use the release-local activation, observation,
  and rollback scripts. Do not hot-edit production units.
