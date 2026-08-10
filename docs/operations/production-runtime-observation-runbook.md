# Production Runtime Observation Runbook

Updated: 2026-08-10

This runbook sends signed, read-only production observation requests through the
deploy-runner. It is for checking the current QiWe image-send and Xiaoman daily
case-report runtime state before a reviewed activation decision.

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

## Preconditions

- `release_sha` must match production `release/current`.
- The release must include both observation scripts.
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
