# Erhua Morning Brief Production Activation Runbook

This runbook activates the reviewed Erhua morning brief timer that generates a pending
`text_announcement` artifact. It does not send to QiWe, confirm group messages, or post
to any chat channel.

## Reviewed Assets

- `deploy/sidecar/scripts/erhua-morning-brief-worker.sh`
- `deploy/sidecar/scripts/erhua-morning-brief-timer-observation-smoke.sh`
- `deploy/sidecar/scripts/erhua-legacy-cron-observation-smoke.sh`
- `deploy/sidecar/scripts/retire-erhua-legacy-cron-production.sh`
- `deploy/sidecar/scripts/xiaoman-legacy-cron-observation-smoke.sh`
- `deploy/sidecar/scripts/retire-xiaoman-legacy-cron-production.sh`
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
QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE=1
QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE=1
```

Apply or disable the non-secret Erhua flags only through the reviewed release-local
config script. The script requires the production database URL to already exist exactly
once, preserves the env file owner and mode, and does not print secrets:

```bash
QINTOPIA_ERHUA_MORNING_BRIEF_PRODUCTION_CONFIG=approved-production-erhua-morning-brief-config \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/apply-erhua-morning-brief-production-config.sh --enable
```

Apply the Xiaoman activity read-through Feishu Base values through the reviewed
allowlist copier before activation:

```bash
sudo -n /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/apply-xiaoman-activity-read-through-production-config.py \
  --release-sha <published-production-release-sha> \
  --apply \
  --approval approved-production-xiaoman-activity-read-through-config-v1
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

If Erhua legacy cron observation finds runtime cron declarations, do not activate the
new timer until those declarations are retired through a reviewed Erhua path. For the
observed Erhua legacy cron state with SHA-256
`59edf8abc1602a10a5ffb83120c631395d8c486df66343bfd1591a94da30412c`, retire it only
through the promoted release-local script:

```bash
QINTOPIA_ERHUA_LEGACY_CRON_RETIREMENT=approved-production-erhua-legacy-cron-retirement \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/retire-erhua-legacy-cron-production.sh
```

The retirement script accepts no caller-provided cron path, checks the exact reviewed
hash before writing, creates a same-directory `0600` backup, replaces the runtime cron
file with an empty retired manifest, and emits only sanitized counts and hashes. Re-run
the Erhua legacy cron observation after retirement; activation must still fail closed if
any runtime cron declarations remain. If the reviewed hash does not match the live cron
file, the failure evidence may include only the live `actual_sha256`, reviewed
`expected_sha256`, declaration count, and safe-for-chat/external-call booleans. Use that
evidence to update the reviewed expected hash in a follow-up PR and Release before
retrying retirement; do not bypass the hash gate or retire from an unreviewed script.

If Xiaoman legacy cron observation finds the reviewed production state with SHA-256
`01b211896c85fcd36628993408cdb696c20baf92f07b2fa957520c5bbfa3bd21`, retire it only
through the promoted release-local script:

```bash
QINTOPIA_XIAOMAN_LEGACY_CRON_RETIREMENT=approved-production-xiaoman-legacy-cron-retirement \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/retire-xiaoman-legacy-cron-production.sh
```

The Xiaoman retirement script accepts no caller-provided cron path, checks the exact
reviewed hash before writing, creates a same-directory `0600` backup, replaces the
runtime cron file with an empty retired manifest, and normalizes the retired file mode
to `0600`. Re-run the Xiaoman legacy cron observation after retirement; activation must
still fail closed if any runtime cron declarations remain.

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
