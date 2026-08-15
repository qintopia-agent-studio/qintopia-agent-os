# Production Runtime One-Shot Runbook

Use the `Run Production Runtime One-Shot` GitHub workflow only after the reviewed
Release has been deployed. Erhua immediate runs still require the corresponding
production timer to be enabled. Xiaoman daily case-report backfill is different after
the Hermes cutover: it runs the fixed release-local worker directly for one reviewed
date and must not require the retired systemd timer to be enabled. This path is for an
explicit, owner-approved immediate run of a fixed production worker/backfill when
waiting for the next timer tick would delay recovery or launch evidence.

## Scope

The workflow creates a signed `production-runtime-one-shot` deploy-runner request. The
runner accepts exactly one target per request:

- `xiaoman-daily-case-report-auto-publish-backfill`
- `xiaoman-daily-case-report-approval-repair`
- `xiaoman-daily-case-report-read-through-repair`
- `xiaoman-creative-profile-candidates-apply`
- `erhua-morning-brief`
- `hermes-cron-snapshot-install`

The request must target the current production release SHA, must use
`restart_targets=["qintopia-system-services"]`, and must set both `dry_run=false` and
`rollback_on_smoke_failure=false`.

The runner observes the corresponding release-managed timer only for targets that still
depend on that timer boundary, such as Erhua morning brief. Xiaoman daily case-report
backfill runs only the fixed release-local backfill script, which sources the fixed
production env file, temporarily exports the worker enablement/date override, and
discards worker stdout/stderr in a private temp directory. The result must not include
worker raw output, raw message content, group ids, person ids, database URLs, tokens,
Feishu payloads, QiWe payloads, reviewed profile payload content, or journal logs.

## Fixed Targets

### Xiaoman Daily Case Report Backfill

Use when a reviewed production daily case report must be generated and auto-publish
requested for a specific date. This target remains valid after the Hermes migration even
though the retired systemd timer and persistent
`QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED` flag are disabled.

Workflow inputs:

```text
release_sha=<current-production-release-sha>
runtime_one_shot_target=xiaoman-daily-case-report-auto-publish-backfill
backfill_date=YYYY-MM-DD
approval=approved-production-xiaoman-daily-case-report-auto-publish-backfill
```

This may create/update production artifacts and group message requests through the
reviewed daily case-report and QiWe image-send boundaries. It does not enable the
retired timer or change persistent production configuration.

### Xiaoman Daily Case Report Approval Repair

Use only when Xiaoman daily case-report backfill or worker-run evidence reports the
fixed safe failure
`QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_PRODUCTION_APPROVAL count invalid`
after Hermes cutover. This target may add exactly one fixed production approval line to
the fixed persistent env file; it does not accept chat ids, group ids, database hashes,
payload JSON, env values, or arbitrary config fields.

Workflow inputs:

```text
release_sha=<current-production-release-sha>
runtime_one_shot_target=xiaoman-daily-case-report-approval-repair
backfill_date=
payload_sha256=
approval=approved-production-xiaoman-daily-case-report-config-v1
```

The repair script validates the current release SHA and the fixed env file shape. It
no-ops when the exact approval already exists, fails closed on duplicate or wrong
approval values, and emits only sanitized runtime one-shot evidence.

### Xiaoman Daily Case Report Read-Through Repair

Use only when Xiaoman daily case-report backfill or worker-run evidence reports the
fixed safe failure
`QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_READ_THROUGH_ENABLE count invalid` after Hermes
cutover. This target may add exactly one fixed `READ_THROUGH_ENABLE=1` line to the fixed
persistent env file; it does not accept chat ids, group ids, database hashes, payload
JSON, env values, or arbitrary config fields.

Workflow inputs:

```text
release_sha=<current-production-release-sha>
runtime_one_shot_target=xiaoman-daily-case-report-read-through-repair
backfill_date=
payload_sha256=
approval=approved-production-xiaoman-daily-case-report-config-v1
```

The repair script validates the current release SHA and the fixed env file shape. It
no-ops when the exact read-through key already exists, fails closed on duplicate or
wrong values, and emits only sanitized runtime one-shot evidence.

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

- write persistent production config, except the single fixed Xiaoman daily case-report
  production approval key through the dedicated approval-repair target;
- enable, disable, or roll back business worker timers;
- retire legacy Hermes cron files;
- accept arbitrary commands, service names, payload paths, payload JSON, dates for
  Erhua, or multiple targets;
- run business workers if their required timer boundary is not already observed as
  enabled; Xiaoman daily case-report backfill is the reviewed exception because its
  scheduler is now Hermes and the one-shot calls the worker boundary directly;
- repair any other persistent config value.

Use `Activate Production Timers` for timer activation, `Retire Production Legacy Crons`
for legacy cron retirement, and target-specific rollback workflows or scripts for
rollback.
