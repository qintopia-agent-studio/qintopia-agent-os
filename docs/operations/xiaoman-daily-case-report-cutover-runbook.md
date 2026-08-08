# Xiaoman Daily Case-Report — Production Activation Cutover Runbook

Updated: 2026-08-08

This runbook is the **owner-approved activation path** for promoting
`workflows/xiaoman-daily-case-report` from a merged, `status: draft` workflow package
into a live, release-managed **daily** timer. It replaces any ad-hoc /
conversation-created scheduling and produces a human-reviewed draft PNG every morning.

It is **not** production-completion evidence and must not be used to claim real group
delivery. The script only drafts; a human confirms before any Erhua (二花) handoff or
group send.

## Scope

In scope (this cutover):

- Register a release-managed systemd timer that runs `daily_case_report.py` every day at
  07:45.
- Keep the human confirmation gate; the timer only prints `operator_review_message` and
  writes the PNG draft.
- Remove any legacy / conversation-created cron for this report (if present).

Out of scope (deferred, unchanged by this runbook):

- Auto-send, QiWe image delivery, Erhua (二花) group send. The production QiWe
  image-send adapter is **disabled by design**; enabling it is a separate, heavier
  cutover (owner approval phrase + allowlist + runbook) and is intentionally NOT part of
  this change.
- Edits to `.env`, secrets, or any other production timer.

## Preconditions (owner gates, must pass before install)

1. **PR #389 merged.** The workflow package (script + `workflow.yaml` + rolling-window
   fix) must be merged to `master` so the release checkout contains
   `workflows/xiaoman-daily-case-report/daily_case_report.py`.
2. **Live-read gate.** Confirm the message-store read-through works on the live host for
   the target group (`chat_id=10859791146538059`, 秦托邦的小伙伴（新）). The script
   fails closed (non-zero exit) if
   `QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_READ_THROUGH_ENABLE=1` is unset or the database
   URL is missing.
3. **Bundle/smoke gate.** Run the relevant Xiaoman production preflight smoke on the
   target host. The production baseline expects no conflicting conversation-created cron
   for this report.
4. **Review.** This cutover PR is reviewed and approved. No hot-edits to production
   units.

## Systemd unit design (draft — owner validates parameters)

Modeled on the existing standalone unit in `deploy/runner/`
(`qintopia-agent-os-deploy-runner.{timer,service}`). The daily case report is a Python
script, not a sidecar subcommand, so it follows the standalone `deploy/runner` precedent
rather than the M9 sidecar renderer (`render-systemd-units.sh`, which only renders fixed
sidecar subcommands).

### Owner decisions (do NOT commit; set on the host)

- `OnCalendar` — the daily time. Default `*-*-* 07:45:00` (generate ~07:45 so the
  rolling 24h window covers roughly the previous day; human reviews and sends by
  ~08:00).
- Secrets env file — the message-store database URL file. Reference it via
  `EnvironmentFile=`; never inline secrets. Path is host-specific.
- `WorkingDirectory` / release path — point at the immutable release checkout
  (`/home/ubuntu/qintopia-agent-os-releases/current/...`); do not use a build or
  standalone checkout path.
- `User` / `Group` — match the other Xiaoman runtime units on the host.
- Python interpreter — the host's managed `python3` at the immutable release path.

### `qintopia-xiaoman-daily-case-report.timer` (draft)

```ini
[Unit]
Description=Run Xiaoman daily community case-file report (deterministic draft)

[Timer]
# OWNER DECISION: daily generation time. 07:45 covers the previous day via rolling window.
OnCalendar=*-*-* 07:45:00
Persistent=true
Unit=qintopia-xiaoman-daily-case-report.service

[Install]
WantedBy=timers.target
```

### `qintopia-xiaoman-daily-case-report.service` (draft)

```ini
[Unit]
Description=Xiaoman Daily Community Case-File Report (deterministic draft)
After=network-online.target
Wants=network-online.target

[Service]
Type=oneshot
# OWNER DECISION: user/group matching other Xiaoman runtime units.
User=ubuntu
Group=ubuntu
# OWNER DECISION: immutable release checkout.
WorkingDirectory=/home/ubuntu/qintopia-agent-os-releases/current
# OWNER DECISION: secrets env file (message-store DB URL). Never inline.
EnvironmentFile=/etc/qintopia/xiaoman-daily-case-report.env
Environment=QINTOPIA_PROFILE_ID=xiaoman
Environment=QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_READ_THROUGH_ENABLE=1
# OWNER DECISION: python interpreter + immutable release script path.
ExecStart=/usr/bin/env python3 /home/ubuntu/qintopia-agent-os-releases/current/workflows/xiaoman-daily-case-report/daily_case_report.py --render png --output-dir /var/lib/qintopia-xiaoman-daily-case-report
NoNewPrivileges=true
PrivateTmp=true
```

> The timer runs **without** `--date`, so `_report_date` uses the rolling 24h window
> ending at run time (per the fix in PR #389). To backfill a specific calendar day, run
> the script manually with `--date YYYY-MM-DD`.

## Install procedure (owner-executed on the host)

1. Copy the reviewed `qintopia-xiaoman-daily-case-report.{timer,service}` into
   `/etc/systemd/system`.
2. `sudo systemctl daemon-reload`.
3. `sudo systemctl enable --now qintopia-xiaoman-daily-case-report.timer`.
4. If a legacy conversation-created cron for this report exists, remove it (mirror the
   weekly-preview cleanup), then re-run the relevant observation smoke to confirm it is
   gone.

## Observation (after install)

- `systemctl list-timers qintopia-xiaoman-daily-case-report.timer` shows the next daily
  07:45.
- Forced run for validation (no production send occurs):

  ```bash
  sudo systemctl start qintopia-xiaoman-daily-case-report.service
  journalctl -u qintopia-xiaoman-daily-case-report.service -n 50
  ```

- Confirm the output prints `operator_review_message` with the PNG path and the line
  "本报告仅生成草稿，未发送到任何群聊。确认无误后请回复「发」再执行外发。", never sends,
  and exits 0.
- Confirm the PNG draft is written to the configured `--output-dir`.

## Rollback

1. `sudo systemctl disable --now qintopia-xiaoman-daily-case-report.timer`.
2. Remove the unit files from `/etc/systemd/system`; `sudo systemctl daemon-reload`.
3. Re-run the observation smoke to confirm the expected idle state.

## Acceptance

- `external_send_executed` is always `false`; `requires_human_confirmation` is always
  `true`.
- The report is generated as a release-managed systemd timer, not a conversation-created
  cron.
- No production QiWe image-send adapter is enabled by this change.
- The relevant Xiaoman production preflight smoke passes.
