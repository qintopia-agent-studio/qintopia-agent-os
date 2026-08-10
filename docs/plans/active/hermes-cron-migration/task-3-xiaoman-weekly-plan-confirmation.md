# Task 3: Xiaoman Weekly Plan Confirmation (Sun 20:00)

Updated: 2026-08-10 Status: PR phase implemented 2026-08-10 (uncommitted); server phase
pending release + owner approval Profile: `xiaoman`

## Goal

Move the Sunday plan-confirmation timer from the release-managed systemd timer back to a
Xiaoman Hermes cron job, with identical behavior.

## Current Production State

- Timer: `qintopia-agentos-xiaoman-weekly-plan-confirmation.timer`,
  `OnCalendar=Sun *-*-* 20:00:00` (Asia/Shanghai). Active; last ran 2026-08-09 20:16.
- Worker: `deploy/sidecar/scripts/xiaoman-weekly-plan-confirmation-worker.sh`.
- Rollback:
  `deploy/sidecar/scripts/rollback-xiaoman-weekly-plan-confirmation-production.sh`.
- Observation:
  `deploy/sidecar/scripts/xiaoman-weekly-plan-confirmation-production-observation-smoke.sh`.
- Env gates (confirm in the worker):
  `QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_ENABLED`,
  `QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_PRODUCTION_APPROVAL`, plus the three shared
  Xiaoman activity read-through flags.
- Executor must read the worker and document its outputs in the new runbook.

## Deliverables

Follow "Per-Task Shape" in `README.md`. Concrete names:

1. `runtime/hermes/scripts/qintopia_xiaoman_weekly_plan_confirmation.sh`
   (`TASK_NAME=xiaoman-weekly-plan-confirmation`,
   `WORKER_SCRIPT=xiaoman-weekly-plan-confirmation-worker.sh`).
2. `runtime/hermes/cron/xiaoman/weekly-plan-confirmation.job.json` - same shape as task
   1, with name `小满·周日活动计划确认`, expr `0 20 * * 0`, script
   `qintopia_xiaoman_weekly_plan_confirmation.sh`.
3. `deploy/sidecar/scripts/apply-xiaoman-weekly-plan-confirmation-hermes-cron.sh`,
   approval gate
   `QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_HERMES_CRON=approved-production-xiaoman-weekly-plan-confirmation-hermes-cron`.
4. Registry entry: profile `xiaoman`, expr `0 20 * * 0`.
5. `docs/operations/xiaoman-weekly-plan-confirmation-hermes-cron-runbook.md`.
6. Apply-script tests (same case list as task 1).
7. AGENTS.md rewrite of the weekly-loop release-managed-timer rule for the
   plan-confirmation half, plus updates to
   `docs/operations/xiaoman-weekly-loop-cutover-runbook.md` and
   `docs/operations/xiaoman-weekly-minimum-loop-runbook.md`.

## Task-Specific Requirements

- Same `QINTOPIA_DEPLOYED_COMMIT_SHA` check as task 1.
- Tasks 2 and 3 edit the same rule blocks in AGENTS.md and the same two runbooks;
  whichever lands second rebases and keeps both halves updated.
- Verification compares against the 2026-08-09 20:16 run; structure and work-item types,
  not exact text.

## Server Phase

Standard six steps from `README.md`. Window: must settle before Sunday 2026-08-16 20:00;
watch that run.

## Forbidden

- Same list as task 1: never both schedulers enabled, never hand-edit `jobs.json`, never
  resurrect prompt-style retired jobs.
