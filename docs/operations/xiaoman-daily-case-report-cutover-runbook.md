# Xiaoman Daily Case-Report Auto Publish Cutover Runbook

Updated: 2026-08-08

This document records the reviewed cutover shape for promoting
`workflows/xiaoman-daily-case-report` from a merged, `status: draft` workflow package
into a live, release-managed daily automatic publisher.

Do not copy unit files to the host, create cron jobs, edit production parameters by
hand, or send the image from a local path. Production activation uses only the
release-local reviewed scripts named below after the release has been deployed.

The target behavior is **no per-day human confirmation**. Once the reviewed production
boundary is activated, the daily run creates the report artifact and publishes it
through the governed QiWe image-send adapter.

## Scope

In scope:

- Run the daily report flow every day at 07:45 from a release-managed systemd timer.
- Render the daily JPEG from the immutable release workflow package.
- Upload the JPEG through `operations-daily-case-report-media-upload`.
- Create one idempotent automatic publish work item with
  `operations-daily-case-report-auto-publish-create --apply`.
- Let the existing QiWe image-send production adapter perform external delivery.

Out of scope:

- Host-local edits to `.env`, secrets, unit files, timers, or other production state.
- A Python QiWe sender, deprecated synchronous upload shortcut, or local-image-path
  send.
- Committing the real QiWe group id or secret-bearing runtime value.

## Preconditions

1. The release checkout contains
   `workflows/xiaoman-daily-case-report/daily_case_report.py`.
2. Message-store read-through works on the live host for the reviewed target group.
3. The immutable production runtime has `/usr/bin/python3`, `psycopg`, Playwright,
   Chromium, and the report script dependencies.
4. The release includes: `xiaoman-daily-case-report-auto-publish-worker.sh`,
   `xiaoman-daily-case-report-auto-publish-production-observation-smoke.sh`,
   `activate-xiaoman-daily-case-report-auto-publish-production.sh`,
   `rollback-xiaoman-daily-case-report-auto-publish-production.sh`, and the workflow
   package.
5. The QiWe image-send production adapter is enabled separately through its reviewed
   production gate.

## Rendered Units

Timer:

```ini
[Timer]
OnCalendar=*-*-* 07:45:00
AccuracySec=1min
Persistent=true
Unit=qintopia-agentos-xiaoman-daily-case-report-auto-publish.service
```

Service:

```ini
[Service]
Type=oneshot
User=ubuntu
Group=ubuntu
WorkingDirectory=/home/ubuntu/qintopia-agent-os-releases/current
EnvironmentFile=/etc/qintopia/message-sidecar.env
ExecStart=/usr/bin/env QINTOPIA_DEPLOYED_COMMIT_SHA=<release-sha> /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/xiaoman-daily-case-report-auto-publish-worker.sh
NoNewPrivileges=true
PrivateTmp=true
```

The worker script renders the JPEG, uploads it to the reviewed HTTPS media boundary,
then records the AgentOS artifact and automatic publish request. It does not call QiWe
directly.

## Production Configuration

After the release is deployed, apply the owner-approved production values through the
release-local config entrypoint. Do not edit `/etc/qintopia/message-sidecar.env` by
hand.

```bash
sudo /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/apply-xiaoman-daily-case-report-production-config.py \
  --stdin \
  --apply \
  --approval approved-production-xiaoman-daily-case-report-config-v1 \
  < xiaoman-daily-case-report-production-config.json
```

The JSON input carries only the reviewed release SHA, production database URL hash,
daily report source chat id, target group id, media upload endpoint, media public base
URL, media allowed-host list, and optional message text. The script validates
`release/current`, the existing QiWe image-send production gate, the target-group
allowlist, and the public media host boundary before writing any persistent env value.

## Activation

After production configuration succeeds:

```bash
QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_PRODUCTION_ACTIVATION=approved-production-xiaoman-daily-case-report-auto-publish \
  deploy/sidecar/scripts/activate-xiaoman-daily-case-report-auto-publish-production.sh
```

The activation script requires exactly one persistent value for the enablement flag,
production approval phrase, read-through flag, chat id, target group id, media upload
endpoint, media public base URL, and media allowed-host list.

## Observation

```bash
QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_OBSERVATION_ENABLE=1 \
QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_EXPECTED_STATE=enabled \
  deploy/sidecar/scripts/xiaoman-daily-case-report-auto-publish-production-observation-smoke.sh
```

The observation proves the timer state, daily 07:45 schedule, release-local service
command, and persistent env boundary. Retained worker and QiWe evidence after a real run
must additionally show one window created one approved generated-image artifact, one
automatic publish request, `requires_human_final_confirmation=false`, QiWe upload and
callback completion, and no duplicate send on rerun.

## Rollback

After the persistent env flag is set to
`QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED=0`:

```bash
QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_PRODUCTION_ROLLBACK=approved-production-xiaoman-daily-case-report-auto-publish-rollback \
  deploy/sidecar/scripts/rollback-xiaoman-daily-case-report-auto-publish-production.sh
```

Rollback disables only `qintopia-agentos-xiaoman-daily-case-report-auto-publish.timer`,
stops/resets only the matching service, and leaves unrelated Xiaoman/QiWe timers
unchanged.

## Acceptance

- No per-day human confirmation is required after reviewed activation.
- The report is generated and published by a reviewed release-managed systemd timer.
- External delivery uses the reviewed QiWe image-send adapter.
- One report window creates at most one successful group send.
- Deploy contract, timer observation, QiWe observation, and rollback checks pass.
