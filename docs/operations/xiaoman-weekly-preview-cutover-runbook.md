# Xiaoman Weekly Preview Production Cutover Runbook

Updated: 2026-08-09

This runbook activates the reviewed Xiaoman weekly activity-preview timer. It reads the
next Monday-Sunday activity window through the Xiaoman read-through path, writes the
latest operator-review draft to server-local state, and prints only sanitized counts to
the journal. It never sends to QiWe, calls Erhua, writes Feishu, or marks a group
message ready.

## Reviewed Assets

- `workflows/xiaoman-weekly-preview/weekly_preview.py`
- `deploy/sidecar/scripts/xiaoman-weekly-preview-worker.sh`
- `deploy/sidecar/scripts/xiaoman-weekly-preview-production-observation-smoke.sh`
- `deploy/sidecar/scripts/apply-xiaoman-weekly-preview-production-config.sh`
- `deploy/sidecar/scripts/activate-xiaoman-weekly-preview-production.sh`
- `deploy/sidecar/scripts/rollback-xiaoman-weekly-preview-production.sh`
- `deploy/sidecar/scripts/xiaoman-legacy-cron-observation-smoke.sh`

The deploy bundle must include these assets under
`/home/ubuntu/qintopia-agent-os-releases/current`. Do not copy unit files, edit
`/etc/systemd/system`, or recreate the old Hermes `cron/jobs.json` entry by hand.

## Persistent Runtime Values

Set only through the release-local config script:

```text
QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_ENABLED=1
QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_PRODUCTION_APPROVAL=approved-production-xiaoman-weekly-preview
QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE=1
QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE=1
```

Apply the values after the Release deploy result has succeeded:

```bash
QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_PRODUCTION_CONFIG=approved-production-xiaoman-weekly-preview-config \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/apply-xiaoman-weekly-preview-production-config.sh --enable
```

The script requires the production sidecar env file to already contain exactly one
`QINTOPIA_SIDECAR_DATABASE_URL`, preserves file permissions, and does not print secrets.

## Pre-activation Checks

The reviewed baseline requires the live Xiaoman Hermes cron file to contain no runtime
cron declarations before enabling the systemd timer:

```bash
QINTOPIA_XIAOMAN_LEGACY_CRON_OBSERVATION_ENABLE=1 \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/xiaoman-legacy-cron-observation-smoke.sh
```

If this fails, stop. Retire the old Hermes cron only through a separate reviewed
retirement path for the exact observed `jobs.json` hash. Do not edit
`/home/ubuntu/.hermes/profiles/xiaoman/cron/jobs.json` manually.

## Activate

```bash
QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_PRODUCTION_ACTIVATION=approved-production-xiaoman-weekly-preview \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/activate-xiaoman-weekly-preview-production.sh
```

Activation enables only `qintopia-agentos-xiaoman-weekly-preview.timer`.

```text
OnCalendar=Mon *-*-* 09:30:00
Persistent=true
Unit=qintopia-agentos-xiaoman-weekly-preview.service
```

The worker writes:

```text
/home/ubuntu/.local/state/qintopia-agentos/xiaoman-weekly-preview/latest-operator-review-message.txt
/home/ubuntu/.local/state/qintopia-agentos/xiaoman-weekly-preview/latest-summary.json
```

Both files are mode `0600`. The summary records publishable/skipped counts,
`requires_human_confirmation=true`, `external_send_executed=false`, and the review
message path.

## Observe

```bash
QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_OBSERVATION_ENABLE=1 \
QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_EXPECTED_STATE=enabled \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/xiaoman-weekly-preview-production-observation-smoke.sh
```

This proves the timer state, exact Monday schedule, release-local worker command, and
persistent env gate. It must not run the worker, inspect Feishu data, publish, or send.

## Rollback

First set the persistent enablement to disabled:

```bash
QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_PRODUCTION_CONFIG=approved-production-xiaoman-weekly-preview-config \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/apply-xiaoman-weekly-preview-production-config.sh --disable
```

Then stop the timer:

```bash
QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_PRODUCTION_ROLLBACK=approved-production-xiaoman-weekly-preview-rollback \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/rollback-xiaoman-weekly-preview-production.sh
```

Rollback disables only the weekly preview timer/service and leaves activity, poster,
daily case-report, QiWe, and Erhua timers unchanged.

Verify the disabled state:

```bash
QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_OBSERVATION_ENABLE=1 \
QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_EXPECTED_STATE=disabled \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/xiaoman-weekly-preview-production-observation-smoke.sh
```

## Acceptance

- The weekly preview is a release-managed systemd timer, not a Hermes conversation cron.
- The old Xiaoman Hermes cron observation passes before activation.
- The timer generates an operations-review draft and never performs external send.
- Human confirmation remains required before Erhua handoff or any group message.
- Activation, observation, and rollback scripts all pass from `release/current`.
