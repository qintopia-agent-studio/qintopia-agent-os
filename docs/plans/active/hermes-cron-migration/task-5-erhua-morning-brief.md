# Task 5: Erhua Morning Brief (Daily 08:10)

Updated: 2026-08-10 Status: ready for implementation after task 0 merges Profile:
`erhua`

## Goal

Move the daily Erhua morning brief from the release-managed systemd timer back to an
Erhua Hermes cron job, with identical behavior - including the reviewed auto-publish
chain that sends the brief text to the group. This task also ports the task-0 governance
redesign to the Erhua observation smoke.

## Current Production State

- Timer: `qintopia-agentos-erhua-morning-brief.timer`, daily 08:10 (Asia/Shanghai).
  Active; a manual one-shot also ran 2026-08-10 09:31.
- Worker: `deploy/sidecar/scripts/erhua-morning-brief-worker.sh`; one-shot helper:
  `erhua-morning-brief-one-shot-production.sh`; timer observation:
  `erhua-morning-brief-timer-observation-smoke.sh`.
- Rollback: `deploy/sidecar/scripts/rollback-erhua-morning-brief-production.sh`;
  activation: `activate-erhua-morning-brief-production.sh`; config:
  `apply-erhua-morning-brief-production-config.sh`.
- Auto-publish: the service produces the brief; the actual QiWe text send rides the
  separate `run-qiwe-text-send-worker` path and may send only the reviewed
  `text_activity_announcement` / `text_announcement` work item after artifact approval,
  final confirmation, send-ready evidence, exact content-hash binding, and target-group
  allowlist (AGENTS.md). That chain stays untouched; the Hermes job only replaces the
  scheduler.
- Erhua `jobs.json` (`/home/ubuntu/.hermes/profiles/erhua/cron/jobs.json`) currently has
  envelope `{"jobs": [], "updated_at": ...}` with no retirement metadata - different
  from Xiaoman's. The apply script must accept this envelope and preserve `updated_at`.
- AI-news rule: five items, English items need explicit Chinese title and summary
  translations (AGENTS.md); irrelevant to scheduling but the runbook must not suggest
  prompt-style jobs that could bypass the deterministic worker.

## Deliverables

Follow "Per-Task Shape" in `README.md`. Concrete names:

1. `runtime/hermes/scripts/qintopia_erhua_morning_brief.sh`
   (`TASK_NAME=erhua-morning-brief`, `WORKER_SCRIPT=erhua-morning-brief-worker.sh`).
   Check the unit for extra `Environment=` bindings (same method as task 4) and bind
   whatever the worker requires.
2. `runtime/hermes/cron/erhua/morning-brief.job.json` - same shape as task 1, with name
   `二花·每日早报`, expr `10 8 * * *`, script `qintopia_erhua_morning_brief.sh`, and the
   origin chat id placeholder `{{QINTOPIA_ERHUA_TECHNICAL_HOME_CHANNEL}}` (confirm the
   real env var name from the Erhua profile config at apply time; never commit it).
3. `deploy/sidecar/scripts/apply-erhua-morning-brief-hermes-cron.sh`, approval gate
   `QINTOPIA_ERHUA_MORNING_BRIEF_HERMES_CRON=approved-production-erhua-morning-brief-hermes-cron`.
4. Registry entry: profile `erhua`, expr `10 8 * * *`.
5. Governance: port the task-0 allowlist redesign to
   `deploy/sidecar/scripts/erhua-legacy-cron-observation-smoke.sh` (profile `erhua`,
   same matching rule) and extend `tools/deploy/test-erhua-legacy-cron-observation.mjs`
   with the same new cases.
6. `docs/operations/erhua-morning-brief-hermes-cron-runbook.md`.
7. Apply-script tests (same case list as task 1, plus envelope-preservation case for
   `updated_at`).
8. AGENTS.md rewrite of the "Erhua morning brief production uses a release-managed
   timer" rule and the reviewed 08:10 schedule note (schedule now lives in the Hermes
   job; the registry pins the reviewed expr).

## Task-Specific Requirements

- The activation script `activate-erhua-morning-brief-production.sh` and the timer
  observation smoke reference the legacy-cron smoke; after this task they pass only via
  the registry entry. Keep the auto-publish approval values
  (`approved-production-erhua-morning-brief-auto-publish`,
  `approved-production-qiwe-text-send`) exactly as they are - the worker and send worker
  still consume them from the same env file.
- Verification: use `erhua-morning-brief-one-shot-production.sh` semantics as the
  reference; run the wrapper manually after the 08:10 systemd run and confirm it no-ops
  or reproduces the same brief artifact. The auto-publish chain must not send a
  duplicate brief; check the send-ready evidence before enabling the Hermes job.
- Cutover must happen inside one day (after 08:10, before midnight) so the next morning
  has exactly one scheduler enabled.

## Server Phase

Standard six steps from `README.md`, same-day sequencing: after the 08:10 run, apply
(disabled), manual verification, rollback the timer, enable the Hermes job, then watch
the next 08:10 fire and the group arrival of the brief.

## Forbidden

- Same list as task 1, plus: never generalize the QiWe text-send worker, never make
  `run-group-message-send-worker` perform a real send, and never relax the exact
  content-hash binding while touching scheduler code.
