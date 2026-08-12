# Production Runtime Observation Runbook

Updated: 2026-08-11

This runbook sends signed, read-only production observation requests through the
deploy-runner. It is for checking the current QiWe image-send and Xiaoman daily
case-report runtime state before a reviewed activation decision, and for collecting
worker-run evidence after a reviewed Hermes cron job should have fired.

## Workflow

Use the `Observe Production Runtime` workflow from `master`.

Required inputs:

```text
release_sha=<current production release SHA>
observation_targets=<comma-separated fixed targets>
```

Allowed targets:

```text
qiwe-image-send
xiaoman-daily-case-report-auto-publish
hermes-cron-snapshot
hermes-cron-live-parity
erhua-morning-brief-worker-run
xiaoman-daily-case-report-worker-run
xiaoman-weekly-recruitment-worker-run
xiaoman-weekly-plan-confirmation-worker-run
xiaoman-weekly-preview-worker-run
```

The workflow creates a signed deploy request with
`release_scope=["production-observation"]`. The production runner accepts only that
fixed scope and the fixed target enum above. It does not execute caller-provided shell.

## Boundary

Observation is read-only:

- It does not enable or disable timers.
- It does not write persistent production config.
- It does not call QiWe, Feishu, Postgres mutation commands, or any activation script.
- It does not retire legacy cron files.
- It records only target status and sanitized script output in the deploy result.

`qiwe-image-send` uses the release-local
`qiwe-image-send-production-observation-smoke.sh` with `EXPECTED_STATE=auto`, so the
result records whether production image-send currently observes as enabled or disabled.

`xiaoman-daily-case-report-auto-publish` first observes the enabled state. If that
fails, it observes the disabled state. If both fail, the deploy result records sanitized
failure tails for both attempts so the missing config or systemd boundary is visible
without exposing raw secrets. Successful evidence includes
`xiaoman_daily_case_report_auto_publish_observation_state=enabled` or
`xiaoman_daily_case_report_auto_publish_observation_state=disabled`.

## Hermes Cron Snapshot And Parity

`hermes-cron-snapshot` checks only the fixed server-local snapshot boundary. Successful
evidence is limited to:

- `hermes_cron_snapshot_observation_result=success`
- `hermes_cron_snapshot_timer_unit_present=true`
- `hermes_cron_snapshot_service_unit_present=true`
- `hermes_cron_snapshot_repo_present=true`
- `hermes_cron_snapshot_remote_absent=true`
- `hermes_cron_snapshot_latest_commit_epoch=<unix>`

`hermes-cron-live-parity` compares the reviewed registry in `release/current` with the
live Hermes profile `jobs.json` files. It verifies schedule, script, `no_agent`,
`deliver`, and the fixed `origin` routing boundary, resolving live chat ids only from
the reviewed server-local profile env files. Successful evidence is limited to:

- `hermes_cron_live_parity_result=success`
- `hermes_cron_live_parity_reviewed_count=5`
- `hermes_cron_live_parity_live_count=5`
- `hermes_cron_live_parity_enabled_count=<count>`

Both targets emit only fixed failure reason tokens through
`hermes_cron_snapshot_observation_error=<token>` or
`hermes_cron_live_parity_observation_error=<token>`. They must not print live
`jobs.json`, snapshot contents, group ids, prompts, env values, or raw script output.

## Worker-Run Evidence

The `*-worker-run` targets prove a reviewed Hermes cron wrapper actually reached its
worker and the worker finished successfully; the timer-state targets above only prove
the older systemd boundary is armed where that boundary still exists. Each worker-run
target checks, through the fixed release-local
`production-worker-run-evidence-smoke.sh`:

- The fixed server-local Hermes cron log for the target contains a latest
  `<timestamp> <task> run=ok` sentinel. If the latest sentinel is `run=failed`, the
  observation fails with `worker_failed`.
- For the three Xiaoman weekly loop targets, the worker's fixed `latest-summary.json`
  exists and keeps the reviewed draft invariants (`requires_human_confirmation=true`,
  `external_send_executed=false`, `safe_for_member_chat=false`) plus a valid
  `date`/`week_start`.
- For `xiaoman-daily-case-report-worker-run`, a new worker summary is optional for
  backward-compatible observation. When present, it is parsed only for safe counters and
  character-universe schema flags, and the observation fails if the summary claims
  `raw_messages_included=true` or `profile_fact_text_included=true`.

Successful evidence records only sanitized fields such as
`erhua_morning_brief_worker_run_result=success`,
`erhua_morning_brief_worker_run_epoch=<unix>`, and for weekly targets
`xiaoman_weekly_preview_worker_summary_present=true` and
`xiaoman_weekly_preview_worker_summary_date=<YYYY-MM-DD>`. For the daily case report,
new runs may also record `xiaoman_daily_case_report_worker_character_count=<count>` and
`xiaoman_daily_case_report_worker_character_universe_schema_version=<schema>`. Character
universe meme, callback, and same-topic relationship surfaces are recorded only as
bounded counts, never as labels or excerpts. When the fixed Hermes cron log is absent or
contains no reviewed sentinel for the task, the observation passes with
`<key>_worker_run_result=not_started`: before the first scheduled trigger this means the
Hermes job has not fired yet, not a regression. Rerun the observation after the
scheduled time; `not_started` after the scheduled time means the Hermes job did not
reach the reviewed wrapper and needs reviewed investigation. Failure evidence records
only the fixed reason token, one of `worker_failed`, `summary_missing`,
`summary_invalid`, or `python_unavailable`. The script echoes no log output, env values,
group ids, or summary text.

Worker-run evidence is read-only: it inspects the fixed Hermes cron log and reads the
worker's summary JSON only. It does not start or stop services, write state, or call
QiWe, Feishu, or Postgres.

## Preconditions

- `release_sha` must match production `release/current`.
- The release must include the observation scripts for the selected targets.
- The request uses `restart_targets=["qintopia-system-services"]`, `dry_run=false`, and
  `rollback_on_smoke_failure=false`.

## Evidence

Acceptance requires:

- The workflow run succeeds.
- The server deploy result has `status=succeeded`.
- The `production-observation` check is present and passed.
- Each selected target appears in the check detail with `status=passed`.

If a selected target fails, use the check detail as the next reviewed remediation input.
Do not treat a failed observation as approval to activate or hot-edit production state.
