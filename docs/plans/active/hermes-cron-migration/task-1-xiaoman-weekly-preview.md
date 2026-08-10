# Task 1: Xiaoman Weekly Preview (Mon 09:30)

Updated: 2026-08-10 Status: ready for implementation after task 0 merges Profile:
`xiaoman`

## Goal

Move the Monday weekly-activity-preview timer from the release-managed systemd timer
back to a Xiaoman Hermes cron job, with identical behavior: read the next Monday-Sunday
activity window, write the operator-review draft and the weekly poster brief to
server-local state, send nothing extra.

## Current Production State

- Timer: `qintopia-agentos-xiaoman-weekly-preview.timer`,
  `OnCalendar=Mon *-*-* 09:30:00` (Asia/Shanghai). Active; last ran 2026-08-10 09:30.
- Service runs `deploy/sidecar/scripts/xiaoman-weekly-preview-worker.sh` from
  `release/current` with `EnvironmentFile=/etc/qintopia/message-sidecar.env` and
  `QINTOPIA_DEPLOYED_COMMIT_SHA` bound at the exec boundary.
- Worker env gates (self-checked, exit 0 skip when unset):
  `QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_ENABLED=1`,
  `QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_PRODUCTION_APPROVAL=approved-production-xiaoman-weekly-preview`,
  `QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE=1`,
  `QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE=1`,
  `QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE=1`.
- Output: latest draft + `latest-weekly-poster-brief.json` under
  `/home/ubuntu/.local/state/qintopia-agentos/xiaoman-weekly-preview/`; sanitized counts
  to stdout. Never sends to QiWe.

## Deliverables

Follow "Per-Task Shape" in `README.md`. Concrete names for this task:

1. `runtime/hermes/scripts/qintopia_xiaoman_weekly_preview.sh` from the task-0 wrapper
   template (`TASK_NAME=xiaoman-weekly-preview`,
   `WORKER_SCRIPT=xiaoman-weekly-preview-worker.sh`).
2. `runtime/hermes/cron/xiaoman/weekly-preview.job.json`:

   ```json
   {
     "name": "小满·周一活动预告（文字稿+海报简报）",
     "schedule": { "kind": "cron", "expr": "30 9 * * 1", "display": "30 9 * * 1" },
     "no_agent": true,
     "script": "qintopia_xiaoman_weekly_preview.sh",
     "deliver": "origin",
     "origin": {
       "platform": "wecom",
       "chat_id": "{{QINTOPIA_XIAOMAN_TECHNICAL_HOME_CHANNEL}}",
       "chat_name": null,
       "thread_id": null
     },
     "enabled": false,
     "skills": []
   }
   ```

3. `deploy/sidecar/scripts/apply-xiaoman-weekly-preview-hermes-cron.sh` with approval
   gate
   `QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_HERMES_CRON=approved-production-xiaoman-weekly-preview-hermes-cron`.
   It resolves the real chat id from the Xiaoman profile env at apply time (read-only,
   never printed), generates the 12-hex id, inserts the job `enabled: false`, and on
   `--enable` flips it. It also installs the wrapper into
   `/home/ubuntu/.hermes/scripts/` (mode `0700`) and runs the task-0 snapshot sync at
   the end of each pass.
4. Registry entry: profile `xiaoman`, expr `30 9 * * 1`, script
   `qintopia_xiaoman_weekly_preview.sh`.
5. `docs/operations/xiaoman-weekly-preview-hermes-cron-runbook.md`.
6. Tests: apply-script test modeled on
   `tools/deploy/test-xiaoman-legacy-cron-retirement.mjs` (insert into empty file,
   insert preserves other jobs' runtime fields, refuses duplicate name, `--enable` flips
   only the matching job, backup file created).
7. AGENTS.md: replace the "Xiaoman weekly preview production uses a release-managed
   timer, not Hermes conversation cron" rule with the Hermes-cron source-of-truth rule
   for this timer, and update
   `docs/operations/xiaoman-weekly-preview-cutover-runbook.md` plus
   `docs/operations/xiaoman-weekly-minimum-loop-runbook.md` to point at the new runbook.

## Task-Specific Requirements

- Check whether the worker validates `QINTOPIA_DEPLOYED_COMMIT_SHA`; if yes, the wrapper
  must derive it (e.g.
  `basename "$(readlink -f /home/ubuntu/qintopia-agent-os-releases/current)"`) and
  export it before invoking the worker.
- The poster brief (`latest-weekly-poster-brief.json`, added 2026-08-10) rides the same
  worker; no extra job for it. The "poster goes straight to the group" enhancement is
  explicitly out of scope.
- Verification step compares against the systemd run from 2026-08-10 09:30: same draft
  file shape, same summary counts (activity titles may shift if the table changed -
  compare structure, not exact content).
- After cutover, `xiaoman-weekly-preview-production-observation-smoke.sh` will report
  the systemd timer disabled; that is the expected end state. Update the runbook to
  treat the task-0 allowlist smoke plus the snapshot log as the health signal.

## Server Phase

Standard six steps from `README.md`. Window: any day; next natural fire is Monday
2026-08-17 09:30, so finish cutover before then and watch that run.

## Forbidden

- Do not enable both the systemd timer and the Hermes job at once.
- Do not hand-edit `/home/ubuntu/.hermes/profiles/xiaoman/cron/jobs.json`; only the
  apply script writes it.
- Do not copy the retired `jobs.json.retired-*.bak` prompt-style jobs back; this
  migration uses the deterministic `no_agent` script form only.
