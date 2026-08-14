# Xiaoman Daily Case-Report Auto Publish Cutover Runbook

Updated: 2026-08-11

> **Status: rollback-only.** The daily 08:00 case-report auto-publish lives in a Xiaoman
> Hermes cron job; use
> `docs/operations/xiaoman-daily-case-report-hermes-cron-runbook.md` for cutover. This
> systemd runbook is kept only as the rollback target after the Hermes job is disabled.

This document records the retired systemd cutover shape for
`workflows/xiaoman-daily-case-report`. Use it only to restore the old systemd publisher
during rollback.

Do not copy unit files to the host, create cron jobs, edit production parameters by
hand, or send the image from a local path. Production activation uses only the
release-local reviewed scripts named below after the release has been deployed.

The target behavior is **no per-day human confirmation**. Once the reviewed production
boundary is activated, the daily run creates the report artifact and publishes it
through the governed QiWe image-send adapter.

## Scope

In scope:

- Restore the daily report flow every day at 08:00 from the retired release-managed
  systemd timer only after disabling the Hermes cron job.
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
3. The immutable production runtime has `/usr/bin/python3`, the reviewed `/usr/bin/psql`
   fallback, system Pillow renderer, and the report script dependencies. Do not install
   Python packages or browser binaries on the server during rollback.
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
OnCalendar=*-*-* 08:00:00
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

The worker script renders the JPEG, uploads it to the reviewed Feishu-backed primary
storage boundary, then records the AgentOS artifact and automatic publish request. It
does not call QiWe directly.

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
`storage_backend: "feishu-base"`, daily report source chat id, target group id, and
optional message text. The script validates `release/current`, the existing QiWe
image-send production gate, the target-group allowlist, and the reviewed Huabaosi Feishu
primary-storage boundary before writing any persistent env value. Do not invent or
provide HTTPS media upload endpoint values for the current production path.

## Activation

After production configuration succeeds:

```bash
QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_PRODUCTION_ACTIVATION=approved-production-xiaoman-daily-case-report-auto-publish \
  deploy/sidecar/scripts/activate-xiaoman-daily-case-report-auto-publish-production.sh
```

The activation script requires exactly one persistent value for the enablement flag,
production approval phrase, read-through flag, storage backend, chat id, and target
group id. For the current production path, the storage backend must be `feishu-base`.

## Observation

```bash
QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_OBSERVATION_ENABLE=1 \
QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_EXPECTED_STATE=enabled \
  deploy/sidecar/scripts/xiaoman-daily-case-report-auto-publish-production-observation-smoke.sh
```

The observation proves the timer state, daily 08:00 schedule, release-local service
command, and persistent env boundary. Retained worker and QiWe evidence after a real run
must additionally show one window created one approved generated-image artifact, one
automatic publish request, `requires_human_final_confirmation=false`, QiWe upload and
callback completion, and no duplicate send on rerun.

## Backfill

To publish a missed calendar day during rollback or after Hermes cutover, use the
reviewed backfill entrypoint. It calls the fixed release-local worker directly for that
one date, temporarily exports the worker enablement and date override in the process
environment, and leaves the retired systemd timer untouched. For example, on 2026-08-09,
yesterday is 2026-08-08:

```bash
QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_BACKFILL=approved-production-xiaoman-daily-case-report-auto-publish-backfill \
QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_BACKFILL_RELEASE_SHA=<published-production-release-sha> \
QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_BACKFILL_DATE=2026-08-08 \
  deploy/sidecar/scripts/xiaoman-daily-case-report-auto-publish-backfill.sh
```

The backfill script validates the fixed production env, reviewed release SHA, and target
group boundary, then injects the one-day `--date` override only into the worker process.
It does not create cron entries, copy units, start the retired timer/service, call QiWe
directly, or accept a local image path.

## Rollback

Use the production configuration entrypoint with `desired_state: "disabled"` to clear
the managed daily report keys and leave only the persistent disabled flag. Then stop the
timer through the reviewed rollback script.

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
- The forward path is Hermes cron. This systemd path may generate and publish only
  during a reviewed rollback window after the Hermes job is disabled.
- External delivery uses the reviewed QiWe image-send adapter.
- One report window creates at most one successful group send.
- Deploy contract, timer observation, QiWe observation, and rollback checks pass.
