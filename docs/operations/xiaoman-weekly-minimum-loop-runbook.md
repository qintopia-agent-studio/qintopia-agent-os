# Xiaoman Weekly Minimum Loop Runbook

Updated: 2026-08-02

## Current Setup

| Step                        | Schedule       | Status               | Next action                                                       |
| --------------------------- | -------------- | -------------------- | ----------------------------------------------------------------- |
| Resident recruitment form   | Saturday 10:00 | Hermes cron ready    | Activate via `xiaoman-weekly-recruitment-hermes-cron-runbook.md`. |
| Plan-sheet confirmation     | Sunday 20:00   | Hermes cron migrated | Run `xiaoman-weekly-plan-confirmation-hermes-cron-runbook.md`.    |
| Confirmed next-week preview | Monday 09:30   | Hermes cron ready    | Activate via `xiaoman-weekly-preview-hermes-cron-runbook.md`.     |

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

On 2026-08-10 the owner reopened weekly poster generation for the Monday preview. The
Monday `weekly_preview` output now includes a `weekly_poster_brief` review artifact next
to the text draft, and the reviewed intake is
`qintopia_xiaoman_weekly_poster_workflow_prepare`: it starts one AgentOS
`activity_promotion` workflow (parent plus evidence and visual children) from the
approved brief. Downstream poster brief approval, image generation, generated-image
review, and final group-send confirmation still follow the existing AgentOS human gates.
Automatic final confirmation and direct QiWe send remain deferred:

- feedback tables or feedback forms;
- material recap automation;
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

The durable timer paths are
`docs/operations/xiaoman-weekly-recruitment-hermes-cron-runbook.md` (Saturday
recruitment), `docs/operations/xiaoman-weekly-plan-confirmation-hermes-cron-runbook.md`
(Sunday plan confirmation), and
`docs/operations/xiaoman-weekly-preview-hermes-cron-runbook.md` (Monday preview). Hermes
cron is the source of truth for all three weekly jobs;
`docs/operations/xiaoman-weekly-loop-cutover-runbook.md` and
`docs/operations/xiaoman-weekly-preview-cutover-runbook.md` are kept only as the systemd
rollback targets. Do not hot-edit production units, hand-edit `jobs.json`, or recreate
the Hermes crons after cutover; use the reviewed apply scripts.

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

## Operating rules

These constraints supplement the scoped AGENTS.md summaries. Conditions and historical
exceptions remain binding. Backtick paths from root rules are repository-relative;
Sidecar rules retain their original `runtime/sidecar/` path base.

### Commands

- Xiaoman activity Feishu read-through is not a generic Base browser: it stays
  read-only, bound to the two configured tables, and output-capped. Record columns pass
  through to chat-safe outputs by default (`ActivityRecordView.fields` from
  `runtime/sidecar/src/xiaoman_activity.rs` `passthrough_fields`, sanitized under the
  `fields` key by `skills/qintopia-tools/variants/xiaoman/__init__.py`), so a new
  activity-table column needs no code change to become chat-visible. Only internal
  identifiers, attachment payloads, and record links are blocked: the reviewed denylist
  lives in both `FEISHU_READ_DENIED_FIELDS` (Rust) and
  `XIAOMAN_ACTIVITY_READ_DENIED_FIELDS` (Python) and must stay in sync; adding a new
  sensitive column means an owner-reviewed denylist PR in both places, nothing else. A
  field still needs canonical treatment only when it drives workflow logic (parity
  comparison, weekly workers, schedule confirmation): then add it to
  `ActivityRecord`/`ActivityRecordView` normalization,
  `XIAOMAN_ACTIVITY_RECORD_READ_FIELDS`,
  `deploy/sidecar/scripts/xiaoman-activity-parity-check.py` (`COMPARABLE_FIELDS` and
  `canonical_record` aliases), and
  `deploy/sidecar/scripts/xiaoman-activity-production-parity-capture.sh`
  (`canonical_legacy_record`), or production parity reports false drift.

- Xiaoman weekly loop production:
  - Saturday recruitment recurrence lives in Xiaoman Hermes cron, not in a
    release-managed systemd timer.
    `/home/ubuntu/.hermes/profiles/xiaoman/cron/jobs.json` owns the schedule, the
    enablement, and the delivery target, while the business logic stays release-managed
    in `xiaoman-weekly-recruitment-worker.sh`. The reviewed declaration is
    `runtime/hermes/cron/xiaoman/weekly-recruitment.job.json`, the wrapper is
    `runtime/hermes/scripts/qintopia_xiaoman_weekly_recruitment.sh`, and the allowlist
    entry lives in `runtime/hermes/cron/reviewed-cron-jobs.json`. Apply it only with:

    ```bash
    QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_HERMES_CRON=approved-production-xiaoman-weekly-recruitment-hermes-cron \
      deploy/sidecar/scripts/apply-xiaoman-weekly-recruitment-hermes-cron.sh --install
    QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_HERMES_CRON=approved-production-xiaoman-weekly-recruitment-hermes-cron \
      deploy/sidecar/scripts/apply-xiaoman-weekly-recruitment-hermes-cron.sh --enable
    ```

    Because the rollback script requires `QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_ENABLED=0`
    in the sidecar env while the worker requires `1`, the wrapper exports that flag
    itself and the env file keeps `0` for the retired systemd path. Health signals after
    cutover are `xiaoman-legacy-cron-observation-smoke.sh` reporting reviewed
    declarations only plus the `sync-hermes-cron-snapshot.sh` history. The full
    procedure is `docs/operations/xiaoman-weekly-recruitment-hermes-cron-runbook.md`;
    the legacy systemd activation path in
    `docs/operations/xiaoman-weekly-loop-cutover-runbook.md` is kept only as the
    rollback target.

  - Xiaoman weekly plan confirmation now uses a Hermes cron job (task 3), not the
    release-managed Sunday timer. The reviewed declaration is
    `runtime/hermes/cron/xiaoman/weekly-plan-confirmation.job.json`, the wrapper is
    `runtime/hermes/scripts/qintopia_xiaoman_weekly_plan_confirmation.sh`, and the
    registry entry pins expr `0 20 * * 0`. Persistent config still goes through
    `apply-xiaoman-weekly-plan-confirmation-production-config.sh`; install and enable
    the Hermes job with
    `QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_HERMES_CRON=approved-production-xiaoman-weekly-plan-confirmation-hermes-cron`
    plus `apply-xiaoman-weekly-plan-confirmation-hermes-cron.sh --install` then
    `--enable`. Disable the old timer with
    `rollback-xiaoman-weekly-plan-confirmation-production.sh` before enabling the job;
    because the rollback script requires
    `QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_ENABLED=0` in the sidecar env while the
    worker requires `1`, the wrapper exports that flag itself and the env file keeps `0`
    for the retired systemd path. The wrapper also re-exports
    `QINTOPIA_DEPLOYED_COMMIT_SHA` from `release/current` after sourcing the persistent
    env, then executes the worker from the same resolved release directory. Health is
    the allowlist observation smoke plus the snapshot git history. Follow
    `docs/operations/xiaoman-weekly-plan-confirmation-hermes-cron-runbook.md`.

  The recruitment and plan-confirmation Hermes crons both keep using the Xiaoman
  activity wrapper boundary. Config, activation, observation, and workers must require
  `QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE=1`,
  `QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE=1`, and
  `QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE=1`. Activation and observation for both
  jobs must verify the installed unit's `QINTOPIA_DEPLOYED_COMMIT_SHA` against the
  owner-reviewed release SHA and must first pass
  `xiaoman-legacy-cron-observation-smoke.sh`. The recruitment Hermes cron's allowlist
  coverage is proven by that same smoke after cutover, so its health no longer depends
  on a live systemd unit. Rollback scripts are
  `rollback-xiaoman-weekly-recruitment-production.sh` and
  `rollback-xiaoman-weekly-plan-confirmation-production.sh`.

- Xiaoman weekly preview now uses a Hermes cron job (task 1), not the release-managed
  Monday timer. The reviewed declaration is
  `runtime/hermes/cron/xiaoman/weekly-preview.job.json`, the wrapper is
  `runtime/hermes/scripts/qintopia_xiaoman_weekly_preview.sh`, and the registry entry
  pins expr `30 9 * * 1`. Install and enable the Hermes job with
  `QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_HERMES_CRON=approved-production-xiaoman-weekly-preview-hermes-cron`
  plus `apply-xiaoman-weekly-preview-hermes-cron.sh --install` then `--enable`. Disable
  the old timer with `rollback-xiaoman-weekly-preview-production.sh` before enabling the
  job; the rollback script requires `QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_ENABLED=0` in the
  sidecar env while the worker requires `1`, so the wrapper exports that flag itself and
  the env file keeps `0` for the retired systemd path. The wrapper also re-exports
  `QINTOPIA_DEPLOYED_COMMIT_SHA` from `release/current` after sourcing the persistent
  env, so a stale env value cannot override the release binding. Health is the allowlist
  observation smoke plus the snapshot git history. Follow
  `docs/operations/xiaoman-weekly-preview-hermes-cron-runbook.md`; the legacy systemd
  activation path in `docs/operations/xiaoman-weekly-preview-cutover-runbook.md` is kept
  only as the rollback target. The weekly preview worker now writes both
  `latest-operator-review-message.txt` and `latest-weekly-poster-brief.json`. The poster
  brief is a review artifact only: it must not call the image provider, approve a
  generated image, queue QiWe, or send to a group unless a later reviewed AgentOS
  image-generation and auto-publish policy gate explicitly does that work. The reviewed
  intake for a ready weekly poster brief is
  `qintopia_xiaoman_weekly_poster_workflow_prepare`. It emits a bounded
  `operations-workflow-start` command (dry-run by default, week-plus-content idempotency
  key, `source_record_ref=weekly_preview:<monday>`) that creates one
  `activity_promotion` parent plus evidence and visual children. It must not call image
  providers, write Feishu, approve artifacts, queue QiWe, publish, or send; poster brief
  approval, generated-image review, and final group-send confirmation stay on the
  existing AgentOS human gates.

- Xiaoman activity read-through production config for release-managed Erhua/weekly
  workers must be applied through the reviewed release-local allowlist copier, not by
  sourcing the Xiaoman Hermes profile or hand-editing
  `/etc/qintopia/message-sidecar.env`. It copies only the five reviewed Xiaoman activity
  read-through keys from the fixed `0600 ubuntu:ubuntu` profile env into the fixed
  `0640 root:ubuntu` sidecar env after verifying `release/current`, and writes the
  non-secret `QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE=1` switch required by the Hermes
  wrappers:

  ```bash
  sudo -n /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/apply-xiaoman-activity-read-through-production-config.py \
    --release-sha <published-production-release-sha> \
    --apply \
    --approval approved-production-xiaoman-activity-read-through-config-v1
  ```
