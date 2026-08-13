# Xiaoman Daily Case-Report — Retired Systemd Design Draft

Updated: 2026-08-13

This document is a retained historical draft. It describes the older release-managed
systemd timer idea for `workflows/xiaoman-daily-case-report`, but the current Xiaoman
daily report direction is the wx-cli-style, image-first workflow documented in
`workflows/xiaoman-daily-case-report/README.md`.

Do not use this document as the current activation path. The current production path is
Hermes cron plus the reviewed image upload / auto-publish / QiWe send-ready chain. The
group-facing deliverable is an image; PDF, if added later, is only an archive or private
review artifact.

The current script keeps these production constraints:

- It generates storyline-first daily drafts plus a private `.draft-bundle.json`.
- It groups character sketches by stable `sender_person_id` before display name.
- It may retain only sanitized `draft_counts`, `privacy_flags`, and
  `public_output_style` as evidence.
- It must not retain rendered Markdown, quote text, relationship text, private labels,
  raw messages, person ids, chat ids, or media URIs as production evidence.
- It uses `/usr/bin/psql` as the database fallback when Python `psycopg` is unavailable.
- It uses the local Pillow renderer as the production-safe image fallback; do not
  install Playwright browsers or Python packages on the production host at runtime.

The older notes below remain for context only.

It is **not** the owner-approved activation path. Do not copy unit files to the host,
enable timers, edit production parameters, or remove legacy schedules from this
document. Production activation must land as reviewed deploy/runner code, be included in
the deploy bundle, and be guarded by deploy-contract checks before an owner executes it.

It is **not** production-completion evidence and must not be used to claim real group
delivery. The script only drafts; a human confirms before any Erhua (二花) handoff or
group send.

## Scope

In scope (this design draft):

- Specify the intended release-managed systemd timer that runs `daily_case_report.py`
  every day at 07:45.
- Preserve the human confirmation gate; the timer only prints `operator_review_message`
  and writes the PNG draft.
- Define the deploy/runner requirements that must replace any legacy /
  conversation-created cron for this report.

Out of scope (deferred, unchanged by this design draft):

- Auto-send, QiWe image delivery, Erhua (二花) group send. The production QiWe
  image-send adapter is **disabled by design**; enabling it is a separate, heavier
  cutover (owner approval phrase + allowlist + runbook) and is intentionally NOT part of
  this change.
- Host-local edits to `.env`, secrets, unit files, timers, or any other production
  state.

## Preconditions (future owner gates)

1. **PR #389 merged.** The workflow package (script + `workflow.yaml` + rolling-window
   fix) must be merged to `master` so the release checkout contains
   `workflows/xiaoman-daily-case-report/daily_case_report.py`.
2. **Live-read gate.** Confirm the message-store read-through works on the live host for
   the reviewed Xiaoman QiWe group id supplied from server-local configuration. Do not
   paste, retain, or commit the real `chat_id`. The script fails closed (non-zero exit)
   if `QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_READ_THROUGH_ENABLE=1` is unset or the
   database URL is missing.
3. **Deploy implementation gate.** A follow-up deploy/runner change must add the unit
   templates, render/install/rollback logic, and contract checks before production
   activation.
4. **Bundle/smoke gate.** The future activation must run the relevant Xiaoman production
   preflight smoke through reviewed deploy/runner code. The production baseline expects
   no conflicting conversation-created cron for this report.
5. **Review.** The activation PR is reviewed and approved. No hot-edits to production
   units.

## Systemd Unit Design

Modeled on the existing standalone unit in `deploy/runner/`
(`qintopia-agent-os-deploy-runner.{timer,service}`). The daily case report is a Python
script, not a sidecar subcommand, so it follows the standalone `deploy/runner` precedent
rather than the M9 sidecar renderer (`render-systemd-units.sh`, which only renders fixed
sidecar subcommands).

### Future reviewed parameters

- `OnCalendar` — the daily time. Default `*-*-* 07:45:00` so the rolling 24h window
  covers roughly the previous day; human review happens around 08:00.
- Secrets env file — the message-store database URL file. Reference it via
  `EnvironmentFile=`; never inline secrets. The reviewed deploy/runner path owns the
  host-specific location.
- `WorkingDirectory` / release path — point at the immutable release checkout
  (`/home/ubuntu/qintopia-agent-os-releases/current/...`); do not use a build or
  standalone checkout path.
- `User` / `Group` — match the other Xiaoman runtime units on the host.
- Python interpreter — a reviewed absolute interpreter path from the production
  uv-managed environment. The service must reject `/usr/bin/env`, bare `python3`, shell
  wrappers, or any PATH-dependent interpreter lookup.

### `qintopia-xiaoman-daily-case-report.timer` (draft)

```ini
[Unit]
Description=Retired Xiaoman daily report systemd draft (do not activate)

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
Description=Retired Xiaoman Daily Report Systemd Draft (do not activate)
After=network-online.target
Wants=network-online.target

[Service]
Type=oneshot
# REVIEWED PARAMETER: user/group matching other Xiaoman runtime units.
User=ubuntu
Group=ubuntu
# REVIEWED PARAMETER: immutable release checkout.
WorkingDirectory=/home/ubuntu/qintopia-agent-os-releases/current
# REVIEWED PARAMETER: secrets env file (message-store DB URL). Never inline.
EnvironmentFile=/etc/qintopia/xiaoman-daily-case-report.env
Environment=QINTOPIA_PROFILE_ID=xiaoman
Environment=QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_READ_THROUGH_ENABLE=1
# REVIEWED PARAMETER: absolute production venv interpreter + immutable release script path.
ExecStart=<reviewed-absolute-python-interpreter> /home/ubuntu/qintopia-agent-os-releases/current/workflows/xiaoman-daily-case-report/daily_case_report.py --render png --output-dir /var/lib/qintopia-xiaoman-daily-case-report
NoNewPrivileges=true
PrivateTmp=true
```

> The timer runs **without** `--date`, so `_report_date` uses the rolling 24h window
> ending at run time (per the fix in PR #389). Any backfill path must be added through a
> separate reviewed operational procedure.

## Future Deployment Requirements

Before this timer can be activated, a follow-up reviewed deploy/runner change must:

- Add the unit templates and render/install/rollback logic under `deploy/`.
- Include the activation assets in `tools/deploy/build-deploy-bundle.mjs`.
- Guard the release-root paths, unit identities, and rollback assets in
  `tools/deploy/check-deploy-contracts.mjs`.
- Use the fixed systemd boundary already required by production deploy scripts.
- Render `ExecStart=` with an absolute, reviewed production interpreter path from the
  immutable release environment if this retired path is ever audited again. The current
  production-safe renderer is Pillow; do not require Playwright or Chromium on the
  production host.
- Remove or disable any legacy conversation-created schedule only inside reviewed
  deploy/runner logic, with rollback that restores the previous reviewed state.
- Emit sanitized activation evidence only; do not print secrets, member-level raw chat
  logs, or HTML contents.

## Future Observation Evidence

The future deploy/runner observation must prove:

- The reviewed timer is enabled only by the activation runner, and its next run is the
  expected daily 07:45 schedule.
- A validation run generates a PNG draft, prints `operator_review_message`, does not
  send to QiWe or Erhua, and exits 0.
- The script executes from the immutable release checkout using the reviewed absolute
  interpreter path, not a PATH-resolved interpreter.
- The configured `--output-dir` receives the PNG draft with production-appropriate file
  permissions.
- Any legacy conversation-created schedule is absent after activation.

## Future Rollback Requirements

Rollback must be reviewed deploy/runner code that disables the timer, restores or
removes only reviewed activation assets, reloads systemd through the fixed production
boundary, and emits sanitized idle-state evidence. It must not rely on owner hand-edits
or host-local unit deletion outside the reviewed runner.

## Future Activation Acceptance

- `external_send_executed` is always `false`; `requires_human_confirmation` is always
  `true`.
- The report is generated as a reviewed release-managed systemd timer, not a
  conversation-created cron.
- No production QiWe image-send adapter is enabled by this change.
- The relevant Xiaoman production preflight and deploy-contract checks pass.
