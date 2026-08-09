# Erhua Morning Brief Production Activation Runbook

This runbook activates the reviewed Erhua morning brief timer that generates a pending
`text_announcement` artifact. It does not send to QiWe, confirm group messages, or post
to any chat channel.

## Reviewed Assets

- `deploy/sidecar/scripts/erhua-morning-brief-worker.sh`
- `deploy/sidecar/scripts/erhua-morning-brief-timer-observation-smoke.sh`
- `deploy/sidecar/scripts/erhua-legacy-cron-observation-smoke.sh`
- `deploy/sidecar/scripts/apply-erhua-morning-brief-production-config.sh`
- `deploy/sidecar/scripts/activate-erhua-morning-brief-production.sh`
- `deploy/sidecar/scripts/rollback-erhua-morning-brief-production.sh`
- `workflows/erhua-morning-brief/`

The deploy bundle must include these assets so they exist under
`/home/ubuntu/qintopia-agent-os-releases/current` after release promotion. Do not copy
them by hand into `/etc/systemd/system`, a Hermes profile, or a standalone checkout.

## Persistent Runtime Values

Set these only in the reviewed production sidecar env file:
`/etc/qintopia/message-sidecar.env`.

Required activation value:

```text
QINTOPIA_SIDECAR_DATABASE_URL=<production-agentos-postgres-url>
QINTOPIA_ERHUA_MORNING_BRIEF_ENABLED=1
QINTOPIA_ERHUA_MORNING_BRIEF_PRODUCTION_APPROVAL=approved-production-erhua-morning-brief
QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE=1
QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE=1
```

Apply or disable the non-secret Erhua flags only through the reviewed release-local
config script. The script requires the production database URL to already exist exactly
once, preserves the env file owner and mode, and does not print secrets:

```bash
QINTOPIA_ERHUA_MORNING_BRIEF_PRODUCTION_CONFIG=approved-production-erhua-morning-brief-config \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/apply-erhua-morning-brief-production-config.sh --enable
```

Optional QunMind values:

```text
QINTOPIA_ERHUA_MORNING_BRIEF_QUNMIND_BIN=<absolute-reviewed-qunmind-binary>
QINTOPIA_ERHUA_MORNING_BRIEF_QUNMIND_CONFIG=<absolute-readable-qunmind-config>
```

When QunMind is missing, the workflow uses built-in public RSS/Atom feed fallback for AI
news. The QunMind binary and config paths are host-local runtime state when used. Do not
commit them or print their contents into evidence. The systemd unit binds the Python
interpreter to the fixed Hermes venv entry:
`/home/ubuntu/.hermes/hermes-agent/venv/bin/python`, validated by the release-local
`runtime/hermes/validate_hermes_python.py`.

## Pre-activation Checks

Run read-only observations from the promoted release:

```bash
QINTOPIA_ERHUA_LEGACY_CRON_OBSERVATION_ENABLE=1 \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/erhua-legacy-cron-observation-smoke.sh

QINTOPIA_XIAOMAN_LEGACY_CRON_OBSERVATION_ENABLE=1 \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/xiaoman-legacy-cron-observation-smoke.sh

QINTOPIA_ERHUA_MORNING_BRIEF_TIMER_OBSERVATION_ENABLE=1 \
QINTOPIA_ERHUA_MORNING_BRIEF_TIMER_EXPECTED_STATE=disabled \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/erhua-morning-brief-timer-observation-smoke.sh
```

If either Hermes cron observation finds runtime cron declarations, stop. Capture the
non-secret hash/count evidence and replace that runtime ownership through another
reviewed deploy/profile-bundle change before enabling this timer.

## Activate

After the Release is promoted and the persistent config has been applied:

```bash
QINTOPIA_ERHUA_MORNING_BRIEF_ACTIVATION=approved-production-erhua-morning-brief \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/activate-erhua-morning-brief-production.sh
```

Activation enables only `qintopia-agentos-erhua-morning-brief.timer`. The timer uses:

```text
OnCalendar=*-*-* 08:05:00
Persistent=true
Unit=qintopia-agentos-erhua-morning-brief.service
```

The first run creates a pending text artifact and prints sanitized artifact metadata.
Human review, artifact approval, group-message request creation, final confirmation, and
send-ready recording remain separate gates.

## Observe

After activation:

```bash
QINTOPIA_ERHUA_MORNING_BRIEF_TIMER_OBSERVATION_ENABLE=1 \
QINTOPIA_ERHUA_MORNING_BRIEF_TIMER_EXPECTED_STATE=enabled \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/erhua-morning-brief-timer-observation-smoke.sh
```

The observation checks the fixed unit shape, calendar schedule, future realtime trigger,
and sanitized journal output. It must not run the worker, read the QunMind config
contents, call QiWe, or print message text.

## Rollback

Run:

```bash
QINTOPIA_ERHUA_MORNING_BRIEF_PRODUCTION_CONFIG=approved-production-erhua-morning-brief-config \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/apply-erhua-morning-brief-production-config.sh --disable

QINTOPIA_ERHUA_MORNING_BRIEF_ROLLBACK=approved-production-erhua-morning-brief-rollback \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/rollback-erhua-morning-brief-production.sh
```

Rollback disables and stops only the Erhua morning brief timer/service, resets the
service failed state, and re-runs disabled-state observation. It does not delete
artifacts or edit Hermes profile files.
