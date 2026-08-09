# Workflow: Xiaoman Weekly Minimum Loop

`workflows/xiaoman-weekly-loop` provides the deterministic draft path for the first two
steps of the weekly Xiaoman activity loop:

- Saturday 10:00: resident activity recruitment draft.
- Sunday 20:00: plan-sheet confirmation draft for the operations group.

The Monday 09:30 confirmed preview remains in `workflows/xiaoman-weekly-preview`.

## Responsibility

- Produce fixed operations-review drafts through
  `qintopia_xiaoman_activity_announcement_prepare`.
- Keep the human confirmation gate before any Erhua handoff or group send.
- Avoid Feishu writes, database writes, Erhua calls, QiWe calls, and external sends.
- Keep live form URLs, Feishu table IDs, chat IDs, and secrets in runtime configuration,
  not in git or operator-facing output.

## Running It

```bash
export QINTOPIA_PROFILE_ID=xiaoman
export QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE=1

python workflows/xiaoman-weekly-loop/weekly_loop.py \
  --mode weekly_recruitment_form \
  --json

python workflows/xiaoman-weekly-loop/weekly_loop.py \
  --mode weekly_plan_confirmation \
  --json
```

Without `--date`, the script targets the next ISO week. This matches the production
timers: Saturday and Sunday prepare drafts for the following Monday-Sunday week.

## Production Boundary

- Release units:
  - `qintopia-agentos-xiaoman-weekly-recruitment.timer`
  - `qintopia-agentos-xiaoman-weekly-plan-confirmation.timer`
- Workers:
  - `deploy/sidecar/scripts/xiaoman-weekly-recruitment-worker.sh`
  - `deploy/sidecar/scripts/xiaoman-weekly-plan-confirmation-worker.sh`

Production activation is release-managed only: apply config, activate, observe, and
rollback through the reviewed release-local scripts. Do not recreate Hermes cron entries
or hot-edit production units.

## Acceptance Scenarios

- Running `weekly_recruitment_form` returns `workflow_step=weekly_recruitment_form`,
  `record_source=not_required`, and an operator-review recruitment draft.
- Running `weekly_plan_confirmation` returns `workflow_step=weekly_plan_confirmation`,
  `mentions=["张百忍"]`, and an operator-review confirmation draft.
- Both modes return `requires_human_confirmation=true` and
  `external_send_executed=false`.
- Neither mode emits live Feishu URLs, table IDs, chat IDs, local paths, tokens, or
  traceback text in human-facing output.

## Validation

```bash
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover \
  -s skills/qintopia-tools/variants/xiaoman/tests \
  -p 'test_qintopia_tools.py' \
  -k weekly_minimum_loop

node tools/workflows/check-workflows.mjs
node tools/deploy/check-deploy-contracts.mjs
```
