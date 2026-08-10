# Task 2: Xiaoman Weekly Recruitment (Sat 10:00)

Updated: 2026-08-10 Status: ready for implementation after task 0 merges Profile:
`xiaoman`

## Goal

Move the Saturday weekly-recruitment timer from the release-managed systemd timer back
to a Xiaoman Hermes cron job, with identical behavior.

## Current Production State

- Timer: `qintopia-agentos-xiaoman-weekly-recruitment.timer`,
  `OnCalendar=Sat *-*-* 10:00:00` (Asia/Shanghai). Active; last ran 2026-08-09 20:13
  (that run was a manual one-shot; the natural slot is Saturday 10:00).
- Worker: `deploy/sidecar/scripts/xiaoman-weekly-recruitment-worker.sh`.
- Rollback: `deploy/sidecar/scripts/rollback-xiaoman-weekly-recruitment-production.sh`.
- Observation:
  `deploy/sidecar/scripts/xiaoman-weekly-recruitment-production-observation-smoke.sh`.
- Env gates (confirm exact names in the worker's `require_env` block before writing the
  wrapper): `QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_ENABLED`,
  `QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_PRODUCTION_APPROVAL`, plus the three Xiaoman
  activity read-through flags shared with task 1.
- Executor must read the worker and document what it produces (draft work items vs.
  anything deliverable) in the new runbook; migration must not change that behavior.

## Deliverables

Follow "Per-Task Shape" in `README.md`. Concrete names:

1. `runtime/hermes/scripts/qintopia_xiaoman_weekly_recruitment.sh`
   (`TASK_NAME=xiaoman-weekly-recruitment`,
   `WORKER_SCRIPT=xiaoman-weekly-recruitment-worker.sh`).
2. `runtime/hermes/cron/xiaoman/weekly-recruitment.job.json` - same shape as task 1,
   with name `小满·周六活动招募`, expr `0 10 * * 6`, script
   `qintopia_xiaoman_weekly_recruitment.sh`.
3. `deploy/sidecar/scripts/apply-xiaoman-weekly-recruitment-hermes-cron.sh`, approval
   gate
   `QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_HERMES_CRON=approved-production-xiaoman-weekly-recruitment-hermes-cron`.
4. Registry entry: profile `xiaoman`, expr `0 10 * * 6`.
5. `docs/operations/xiaoman-weekly-recruitment-hermes-cron-runbook.md`.
6. Apply-script tests (same case list as task 1).
7. AGENTS.md rewrite of the "Xiaoman weekly loop production uses release-managed timers"
   rule for the recruitment half, and updates to
   `docs/operations/xiaoman-weekly-loop-cutover-runbook.md` and
   `docs/operations/xiaoman-weekly-minimum-loop-runbook.md`.

## Task-Specific Requirements

- Same `QINTOPIA_DEPLOYED_COMMIT_SHA` check as task 1.
- Config apply/activation scripts
  (`apply-xiaoman-weekly-recruitment-production-config.sh`,
  `activate-xiaoman-weekly-recruitment-production.sh`) stay valid for the persistent env
  values the worker still consumes; the wrapper sources the same env file, so no config
  re-apply is needed. Note in the runbook that the activation script's
  `xiaoman-legacy-cron-observation-smoke.sh` precondition now passes only while the
  Hermes job is absent or reviewed - after this task lands, the registry entry covers
  it.
- Verification compares against the most recent recruitment run output in server-local
  state; compare structure and work-item types, not exact text.

## Server Phase

Standard six steps from `README.md`. Window: must settle before Saturday 2026-08-15
10:00; watch that run.

## Forbidden

- Same list as task 1: never both schedulers enabled, never hand-edit `jobs.json`, never
  resurrect prompt-style retired jobs.
