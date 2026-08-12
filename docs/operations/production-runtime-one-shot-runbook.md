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
- `xiaoman-creative-profile-candidates-apply`
- `erhua-morning-brief`
- `hermes-cron-snapshot-install`

The request must target the current production release SHA, must use
`restart_targets=["qintopia-system-services"]`, and must set both `dry_run=false` and
`rollback_on_smoke_failure=false`.

The runner first observes that the corresponding release-managed timer is enabled. It
then runs only the fixed release-local script for that target and records sanitized
result evidence. The result must not include worker raw output, raw message content,
group ids, person ids, database URLs, tokens, Feishu payloads, QiWe payloads, reviewed
profile payload content, or journal logs.

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

### Xiaoman Creative Profile Candidates Apply

Use only after the daily case-report private review bundle has produced
`eligible_for_review` candidates and an owner has prepared the separate reviewed payload
on the production host at the fixed path:

```text
/home/ubuntu/.local/state/qintopia-agentos/xiaoman-creative-profile-candidates/reviewed-payload.json
```

Workflow inputs:

```text
release_sha=<current-production-release-sha>
runtime_one_shot_target=xiaoman-creative-profile-candidates-apply
backfill_date=
payload_sha256=<64-hex-sha256-of-fixed-reviewed-payload>
approval=approved-production-xiaoman-creative-profile-candidates
```

This may write reviewed `creative_profile` snapshots to
`qintopia_identity.member_profile_snapshots`. The workflow must not accept payload JSON,
payload paths, display names, person ids, candidate text, raw messages, or profile fact
text, and must not retain reviewed profile payload content. Production evidence may
retain only sanitized counts/privacy flags and the reviewed payload SHA-256.

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

### Hermes Cron Snapshot Install

Use when `hermes-cron-snapshot` production observation reports
`hermes_cron_snapshot_observation_error=unit_missing` after the reviewed Release has
been deployed. This installs the fixed server-local snapshot timer and creates the
baseline local git snapshot.

Workflow inputs:

```text
release_sha=<current-production-release-sha>
runtime_one_shot_target=hermes-cron-snapshot-install
backfill_date=
approval=approved-production-hermes-cron-snapshot
```

This writes only the fixed Hermes snapshot systemd user units and the server-local
snapshot repo. It must not print live cron JSON, group ids, prompts, env values, raw
script output, or raw logs. Verify with `Observe Production Runtime` using
`observation_targets=hermes-cron-snapshot,hermes-cron-live-parity`.

## Non-Goals

This workflow must not:

- write persistent production config;
- enable, disable, or roll back business worker timers;
- retire legacy Hermes cron files;
- accept arbitrary commands, service names, payload paths, payload JSON, dates for
  Erhua, or multiple targets;
- run business workers if the target timer is not already observed as enabled.

Use `Activate Production Timers` for timer activation, `Retire Production Legacy Crons`
for legacy cron retirement, and target-specific rollback workflows or scripts for
rollback.
