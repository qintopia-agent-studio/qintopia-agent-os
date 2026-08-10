# Task 4: Xiaoman Daily Case Report Auto-Publish (Daily 08:00)

Updated: 2026-08-10 Status: ready for implementation after task 0 merges Profile:
`xiaoman`

## Goal

Move the daily case-report auto-publish timer from the release-managed systemd timer
back to a Xiaoman Hermes cron job, with identical behavior - including its existing send
path. This is the heaviest of the five migrations because the timer's service is part of
a real group-send chain.

## Current Production State

- Timer: `qintopia-agentos-xiaoman-daily-case-report-auto-publish.timer`, daily 08:00
  (Asia/Shanghai). Active; last ran 2026-08-10 08:00.
- Worker: `deploy/sidecar/scripts/xiaoman-daily-case-report-auto-publish-worker.sh`.
- Rollback:
  `deploy/sidecar/scripts/rollback-xiaoman-daily-case-report-auto-publish-production.sh`.
- Observation:
  `deploy/sidecar/scripts/xiaoman-daily-case-report-auto-publish-production-observation-smoke.sh`.
- The service uploads the rendered JPEG through the Huabaosi Feishu primary-storage
  boundary, creates/updates the approved `generated_image` artifact and one automatic
  `group_message_request`; actual QiWe delivery rides the separate
  `operations-group-send-ready` worker chain. That chain stays untouched.
- The systemd `ExecStart` binds
  `QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA=${TARGET_SHA}` alongside
  `QINTOPIA_DEPLOYED_COMMIT_SHA` (AGENTS.md rule). The Hermes wrapper must reproduce
  both bindings; see below.
- The worker depends on the fixed `/usr/bin/psql` fallback and system Pillow through
  `/usr/bin/python3`; the wrapper must export a PATH that keeps `/usr/bin` first.
- Config script: `apply-xiaoman-daily-case-report-production-config.py`; backfill:
  `xiaoman-daily-case-report-auto-publish-backfill.sh`.

## Deliverables

Follow "Per-Task Shape" in `README.md`. Concrete names:

1. `runtime/hermes/scripts/qintopia_xiaoman_daily_case_report.sh`
   (`TASK_NAME=xiaoman-daily-case-report`,
   `WORKER_SCRIPT=xiaoman-daily-case-report-auto-publish-worker.sh`), extended beyond
   the task-0 template with:

   ```bash
   export PATH="/usr/bin:/bin"
   DEPLOYED_SHA="$(basename "$(readlink -f /home/ubuntu/qintopia-agent-os-releases/current)")"
   export QINTOPIA_DEPLOYED_COMMIT_SHA="$DEPLOYED_SHA"
   export QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA="$DEPLOYED_SHA"
   ```

2. `runtime/hermes/cron/xiaoman/daily-case-report.job.json` - same shape as task 1, with
   name `小满·每日案例日报`, expr `0 8 * * *`, script
   `qintopia_xiaoman_daily_case_report.sh`.
3. `deploy/sidecar/scripts/apply-xiaoman-daily-case-report-hermes-cron.sh`, approval
   gate
   `QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_HERMES_CRON=approved-production-xiaoman-daily-case-report-hermes-cron`.
4. Registry entry: profile `xiaoman`, expr `0 8 * * *`.
5. `docs/operations/xiaoman-daily-case-report-hermes-cron-runbook.md`.
6. Apply-script tests (same case list as task 1) plus a wrapper test asserting the SHA
   binding lines exist and PATH is fixed.
7. AGENTS.md rewrite of the "Xiaoman daily case-report production auto-publish uses a
   release-managed timer" rule and the release-SHA binding rule (the binding moves into
   the wrapper); update `docs/operations/xiaoman-weekly-minimum-loop-runbook.md` if it
   references this timer.

## Task-Specific Requirements

- Idempotency: the worker's Feishu-backed publish reuses only artifacts matching the
  reviewed upload evidence (AGENTS.md). A duplicate fire (systemd + Hermes overlap even
  within one morning) must not create conflicting artifacts - still, the cutover window
  rule is absolute: disable the systemd timer before enabling the Hermes job, on the
  same day, after the 08:00 run.
- Verification: run the wrapper manually mid-morning; confirm it either produces the
  same artifact/request state as the 08:00 systemd run or no-ops idempotently. Compare
  sanitized evidence only; never print media URIs or group ids in the runbook log.
- The `Run Production Runtime One-Shot` GitHub workflow target
  `xiaoman-daily-case-report-auto-publish-backfill` stays valid after migration (it
  calls the worker boundary directly). Note this in the runbook so nobody "fixes" it.

## Server Phase

Standard six steps from `README.md`, sequenced inside one day: after the 08:00 systemd
run, apply (disabled), manual verification, rollback the timer, enable the Hermes job,
then watch the next 08:00 fire.

## Forbidden

- Same list as task 1, plus: never weaken the release-SHA binding to a stale persistent
  env value, and never let the wrapper print the database URL, media URIs, or group ids
  (stdout becomes a WeCom message on failure).
