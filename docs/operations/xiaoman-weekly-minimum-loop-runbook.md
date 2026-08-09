# Xiaoman Weekly Minimum Loop Runbook

Updated: 2026-08-02

## Current Setup

| Step                        | Schedule       | Status                     | Next action                                               |
| --------------------------- | -------------- | -------------------------- | --------------------------------------------------------- |
| Resident recruitment form   | Saturday 10:00 | Built                      | Keep observed; no repository change needed.               |
| Plan-sheet confirmation     | Sunday 20:00   | Timer built; link provided | Fill the scheduled action content below.                  |
| Confirmed next-week preview | Monday 09:30   | Release timer ready        | Activate via `xiaoman-weekly-preview-cutover-runbook.md`. |

This runbook records the 2026-08-01 simplified Xiaoman loop. It is not Xiaoman
production-completion evidence and must not be used to claim real QiWe group delivery.

## Step 2 Action Content

Use this Xiaoman draft mode for the Sunday 20:00 营造司群 timer:

```json
{
  "date": "<YYYY-Www>",
  "mode": "weekly_plan_confirmation",
  "operator_name": "小乔",
  "community_audience": "营造司群",
  "confirmation_owner_name": "张百忍",
  "plan_sheet_label": "下周活动计划表",
  "actor_agent": "xiaoman"
}
```

The timer may attach the already-provided plan-sheet link from runtime configuration. Do
not commit the live Feishu URL, Base token, table id, view id, chat id, or user id.

Expected draft boundary:

- `workflow_step=weekly_plan_confirmation`;
- `mentions=["张百忍"]`;
- human-facing text includes `@张百忍`;
- `requires_human_confirmation=true`;
- `external_send_executed=false`.

## Step 3 Configuration

After 张百忍 confirms the plan sheet, read the confirmed sanitized `activity_plan`
records for the target week, then call:

```json
{
  "date": "<YYYY-Www>",
  "mode": "weekly_preview",
  "operator_name": "小乔",
  "community_audience": "居民群",
  "records": ["<sanitized confirmed activity_plan records>"],
  "actor_agent": "xiaoman"
}
```

The output is still an operations-review draft. Only after human approval should the
approved text be passed to
`qintopia_xiaoman_activity_text_group_message_request_prepare`, which creates an
awaiting-publish Erhua group-message request and still requires final confirmation
before any group send.

Expected draft boundary:

- `workflow_step=weekly_preview`;
- `safe_for_member_chat=false`;
- `requires_human_confirmation=true`;
- `external_send_executed=false`;
- no raw `record_id`, `record_ref`, Feishu table id, chat id, token, local path, or
  traceback in human-facing text.

## Deferred Scope

Do not add these to this first loop unless the owner explicitly reopens scope:

- feedback tables or feedback forms;
- material recap automation;
- poster generation or broader publicity;
- automatic final confirmation;
- direct QiWe send.

## Conversation-Created Timer Storage

Timers created by chatting with Xiaoman live only in the live Hermes profile at
`/home/ubuntu/.hermes/profiles/xiaoman/cron/jobs.json`. They are runtime state: they are
not in git, are not reviewed, and carry no evidence chain. When such a timer fires,
Hermes replays the stored message to Xiaoman in whatever session context exists at that
moment, so execution quality depends on the stored message text and the live session. If
execution drifts from expectations, read the stored messages first:

```bash
jq . /home/ubuntu/.hermes/profiles/xiaoman/cron/jobs.json
```

For this loop, the stored message for steps 2 and 3 must be exactly the fixed skill JSON
from [Step 2 Action Content](#step-2-action-content) and
[Step 3 Configuration](#step-3-configuration). Pinning the message to the reviewed skill
call keeps the timer on the deterministic draft boundary instead of Xiaoman improvising
from conversation memory.

Two standing constraints:

- The reviewed production baseline expects the legacy runtime cron file to be empty.
  `deploy/sidecar/scripts/xiaoman-legacy-cron-observation-smoke.sh` fails when it finds
  runtime cron declarations, and the aggregate production preflight requires legacy-cron
  absence. Conversation-created timers are a temporary operations convenience, not a
  production-scheduled path.
- `cron/jobs.json` is runtime-only profile state. The profile bundle migration will not
  carry it over, so these timers must be re-registered or recreated before the live
  profile symlink cutover.

The durable timer path is `docs/operations/xiaoman-weekly-preview-cutover-runbook.md`.
Do not hot-edit production units or recreate the Hermes cron after that cutover.

## Local Verification

```bash
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover \
  -s skills/qintopia-tools/variants/xiaoman/tests \
  -p 'test_qintopia_tools.py' \
  -k xiaoman_activity_announcement_prepare

node tools/skills/check-qintopia-tools.mjs

node_modules/.bin/markdownlint-cli2 \
  "docs/operations/xiaoman-weekly-minimum-loop-runbook.md" \
  "docs/operations/README.md"
```
