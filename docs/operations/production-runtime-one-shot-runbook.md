# Production Runtime One-Shot Runbook

Use the `Run Production Runtime One-Shot` GitHub workflow only after the reviewed
Release has been deployed and the corresponding production timer is already enabled.
This path is for an explicit, owner-approved immediate run of a fixed production
worker/backfill when waiting for the next timer tick would delay recovery or launch
evidence.

## Scope

The workflow creates a signed `production-runtime-one-shot` deploy-runner request. The
runner accepts exactly one target per request:

- `xiaoman-daily-case-report-auto-publish-backfill`
- `erhua-morning-brief`

The request must target the current production release SHA, must use
`restart_targets=["qintopia-system-services"]`, and must set both `dry_run=false` and
`rollback_on_smoke_failure=false`.

The runner first observes that the corresponding release-managed timer is enabled. It
then runs only the fixed release-local script for that target and records sanitized
result evidence. The result must not include worker raw output, raw message content,
group ids, database URLs, tokens, Feishu payloads, QiWe payloads, or journal logs.

## Fixed Targets

### Xiaoman Daily Case Report Backfill

Use when a reviewed production daily case report must be generated and auto-publish
requested for a specific date after the timer has already been enabled.

Workflow inputs:

```text
release_sha=<current-production-release-sha>
runtime_one_shot_target=xiaoman-daily-case-report-auto-publish-backfill
backfill_date=YYYY-MM-DD
approval=approved-production-xiaoman-daily-case-report-auto-publish-backfill
```

This may create/update production artifacts and group message requests through the
reviewed daily case-report and QiWe image-send boundaries.

### Erhua Morning Brief

Use when the reviewed Erhua morning brief worker must be run immediately after the timer
has already been enabled.

Workflow inputs:

```text
release_sha=<current-production-release-sha>
runtime_one_shot_target=erhua-morning-brief
backfill_date=
approval=approved-production-erhua-morning-brief-one-shot
```

This may create a reviewed text activity announcement and send it through the reviewed
QiWe text-send boundary when the persistent production gates are enabled.

## Non-Goals

This workflow must not:

- write persistent production config;
- enable, disable, or roll back timers;
- retire legacy Hermes cron files;
- accept arbitrary commands, service names, dates for Erhua, or multiple targets;
- run if the target timer is not already observed as enabled.

Use `Activate Production Timers` for timer activation, `Retire Production Legacy Crons`
for legacy cron retirement, and target-specific rollback workflows or scripts for
rollback.
