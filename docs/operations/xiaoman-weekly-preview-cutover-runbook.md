# Xiaoman Weekly Preview Production Cutover Runbook

Updated: 2026-08-07

This document records the reviewed release-managed systemd path for Xiaoman weekly
activity preview. Do not copy unit files into `/etc/systemd/system`, create a
conversation cron, or mutate server cron state by hand.

Production activation may happen only after the Release containing the deploy scripts is
promoted and the owner-approved persistent runtime values are present.

It is **not** production-completion evidence and must not be used to claim real group
delivery. The script only drafts; a human confirms before any Erhua handoff or send.

## Scope

In scope:

- Install a release-managed systemd timer for the weekly preview draft.
- Keep the human confirmation gate; the timer only prints `operator_review_message`.
- Require the legacy Xiaoman Hermes cron observation to pass before activation.

Out of scope:

- Auto-send, QiWe delivery, feedback forms, material recap, poster generation.
- Hand-edits to `.env`, secrets, cron files, `/etc/systemd/system`, or any production
  timer.
- Any direct Erhua or QiWe send.

## Preconditions

1. **Live-read gate.** Confirm the Xiaoman read-through can read both `activity_plan`
   and `activity_occurrence` for the target week on the live host. The script fails
   closed (non-zero exit) if read-through is not enabled.
2. **Deploy contract gate.** The Release includes `xiaoman-weekly-preview-worker.sh`,
   `xiaoman-weekly-preview-timer-observation-smoke.sh`,
   `activate-xiaoman-weekly-preview-production.sh`, and
   `rollback-xiaoman-weekly-preview-production.sh`.
3. **Bundle/smoke gate.** Run `xiaoman-legacy-cron-observation-smoke.sh` on the target
   host. The production baseline expects the legacy runtime cron file to be **empty**;
   activation must not leave a conversation-created Monday cron behind.
4. **Review.** The owner approves the persistent env values and activation phrase. No
   hot-edits to production units.

## Persistent Runtime Values

Set these only in the reviewed production sidecar env file:
`/etc/qintopia/message-sidecar.env`.

```text
QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_ENABLED=1
QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE=1
QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE=1
```

Optional values:

```text
QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_OPERATOR_NAME=<operator display name>
QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_AUDIENCE=<member-facing audience label>
QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_TIMER_CALENDAR=Mon *-*-* 09:30:00
```

The worker uses the fixed Hermes venv Python by default and validates it against the
immutable release before running.

## Rendered Units

Timer:

```ini
[Timer]
OnCalendar=Mon *-*-* 09:30:00
AccuracySec=1min
Persistent=true
Unit=qintopia-agentos-xiaoman-weekly-preview.service
```

Service:

```ini
[Service]
Type=oneshot
User=ubuntu
Group=ubuntu
WorkingDirectory=/home/ubuntu/qintopia-agent-os-releases/current
EnvironmentFile=/etc/qintopia/message-sidecar.env
ExecStart=/usr/bin/env QINTOPIA_DEPLOYED_COMMIT_SHA=<release-sha> QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_PYTHON=/home/ubuntu/.hermes/hermes-agent/venv/bin/python /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/xiaoman-weekly-preview-worker.sh
NoNewPrivileges=true
PrivateTmp=true
UMask=0077
```

## Activate

After the Release is deployed and persistent values are reviewed:

```bash
QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_ACTIVATION=approved-production-xiaoman-weekly-preview \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/activate-xiaoman-weekly-preview-production.sh
```

Activation enables only `qintopia-agentos-xiaoman-weekly-preview.timer`. It first runs
the Xiaoman legacy cron observation and fails if any runtime cron declaration exists.

## Observe

```bash
QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_TIMER_OBSERVATION_ENABLE=1 \
QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_TIMER_EXPECTED_STATE=enabled \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/xiaoman-weekly-preview-timer-observation-smoke.sh
```

The observation checks the fixed unit shape, calendar schedule, future realtime trigger,
and persistent env boundary. It must not run the worker, write Postgres, call
Erhua/QiWe, or print draft text.

## Rollback

First set the persistent flag to `0` in `/etc/qintopia/message-sidecar.env` through the
reviewed runtime config path:

```text
QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_ENABLED=0
```

Then run:

```bash
QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_ROLLBACK=approved-production-xiaoman-weekly-preview-rollback \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/rollback-xiaoman-weekly-preview-production.sh
```

Rollback disables and stops only the weekly-preview timer/service, resets the service
failed state, and re-runs disabled-state observation.

## Acceptance

- `external_send_executed` is always `false`; `requires_human_confirmation` is always
  `true`.
- Reviewed activation requires the legacy natural-language Monday cron task to be absent
  from `jobs.json`.
- Reviewed activation runs the weekly preview as a release-managed systemd timer, not a
  conversation-created cron.
- The aggregate Xiaoman production preflight smoke and legacy-cron smoke both pass.
