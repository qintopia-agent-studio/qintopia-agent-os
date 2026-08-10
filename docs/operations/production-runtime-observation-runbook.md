# Production Runtime Observation Runbook

Updated: 2026-08-10

This runbook sends signed, read-only production observation requests through the
deploy-runner. It is for checking the current QiWe image-send and Xiaoman daily
case-report runtime state before a reviewed activation decision, and for collecting
worker-run evidence after a release-managed timer should have fired.

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

## Worker-Run Evidence

The `*-worker-run` targets prove a timer actually fired and its worker finished
successfully; the timer-state targets above only prove a timer is armed. Each worker-run
target checks, through the fixed release-local
`production-worker-run-evidence-smoke.sh`:

- The paired timer is enabled and active.
- The worker service has a non-empty `ExecMainStartTimestampUSec` (it started at least
  once), `ExecMainStatus=0`, and `Result=success`.
- For the three Xiaoman weekly loop targets, the worker's fixed `latest-summary.json`
  exists and keeps the reviewed draft invariants (`requires_human_confirmation=true`,
  `external_send_executed=false`, `safe_for_member_chat=false`) plus a valid
  `date`/`week_start`.

Successful evidence records only sanitized fields such as
`erhua_morning_brief_worker_run_result=success`,
`erhua_morning_brief_worker_run_epoch=<unix>`, and for weekly targets
`xiaoman_weekly_preview_worker_summary_present=true` and
`xiaoman_weekly_preview_worker_summary_date=<YYYY-MM-DD>`. Failure evidence records only
the fixed reason token, one of `systemctl_unavailable`, `timer_not_enabled`,
`timer_not_active`, `service_never_started`, `worker_failed`, `summary_missing`,
`summary_invalid`, or `python_unavailable`. `service_never_started` before the first
scheduled trigger means the timer has not fired yet; rerun the observation after the
scheduled time instead of treating it as a regression. The script echoes no journal
output, env values, group ids, or summary text.

Worker-run evidence is read-only: it inspects systemd state and reads the worker's
summary JSON only. It does not start or stop services, write state, or call QiWe,
Feishu, or Postgres.

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
