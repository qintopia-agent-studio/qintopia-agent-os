# Workflow: Xiaoman Weekly Activity Preview

`workflows/xiaoman-weekly-preview` replaces the old natural-language Monday cron task
with a deterministic, reviewable script.

## Responsibility

- Read the next 7 days of Xiaoman activity records (Monday through Sunday).
- Filter out temporary meals and activities missing required fields (title / time /
  location / owner).
- Produce a single "下周活动预告" draft, a matching weekly poster brief, and a
  human-review message.
- Never generate the image, send, publish, write Feishu, or call Erhua/QiWe. A human or
  separately reviewed auto-publish policy confirms first.

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
4. The text draft, `weekly_poster_brief`, and an `operator_review_message` are returned.
   If the week is empty, the output clearly says "下周暂无已确认活动，暂不生成预告"
   instead of sending stale text.
5. The poster brief includes only complete weekly activities. Records with an explicit
   unconfirmed schedule state stay out of the poster brief.

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

The script prints the `operator_review_message` (or the full JSON with `--json`). The
release worker also writes `latest-weekly-poster-brief.json` next to the operator
message. A human reads it, replies "发" in the operations chat, and only then can the
text and poster brief move into the existing AgentOS image-generation and group-send
gates.

## Weekly Poster Intake

When `weekly_poster_brief.status` is `ready_for_human_confirmation`, the reviewed intake
is `qintopia_xiaoman_weekly_poster_workflow_prepare`. It converts the brief into a
bounded `operations-workflow-start` command (dry-run by default) that creates one
`activity_promotion` parent plus evidence and visual children. From there the existing
release-managed workers take over: the evidence and visual workers produce the
`evidence_summary` and pending `poster_brief`, the image-generation starter consumes an
approved `poster_brief`, and the send-request starter consumes an approved
`generated_image`. Every artifact review and the final group-send confirmation remain
human gates; this workflow never calls the image provider or QiWe by itself.

## Acceptance Scenarios

- Running with an empty week prints "下周暂无已确认活动，暂不生成预告" and exits 0.
- Running with activities returns `publishable_count`, `skipped_count`,
  `missing_followups`, `weekly_poster_brief`, and `operator_review_message`.
- The weekly poster brief is `ready_for_human_confirmation` only when at least one
  complete confirmed activity is available.
- `external_send_executed` is always `false`; `requires_human_confirmation` is always
  `true`.
- The script fails closed (non-zero exit) if read-through is not enabled or the week
  cannot be read.

## Validation

```bash
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover \
  -s skills/qintopia-tools/variants/xiaoman/tests \
  -p 'test_qintopia_tools.py' \
  -k weekly_preview

node tools/workflows/check-workflows.mjs
node tools/deploy/check-deploy-contracts.mjs
```

## Production Boundary

- Reads Feishu activity tables; does not write them.
- Produces review artifacts only; never sends externally.
- Does not call the image-generation provider. The weekly poster brief must still become
  an approved AgentOS `poster_brief` before any `image_generation_request` or group
  send.
- Requires the same secrets as the Xiaoman read-through path.

## Production Activation

This workflow replaces the legacy natural-language Monday cron task. Activation is
performed only through the reviewed release-local production scripts:

- Runbook:
  [`docs/operations/xiaoman-weekly-preview-cutover-runbook.md`](../operations/xiaoman-weekly-preview-cutover-runbook.md)
- Release unit: `qintopia-agentos-xiaoman-weekly-preview.timer`
- Worker: `deploy/sidecar/scripts/xiaoman-weekly-preview-worker.sh`

The timer keeps the human confirmation gate and refuses activation while the old Xiaoman
Hermes cron observation finds runtime job declarations. Installing the unit is done by
the normal Release deploy; enabling it requires the owner-approved config, activation,
observation, and rollback scripts. Do not hot-edit production units.
