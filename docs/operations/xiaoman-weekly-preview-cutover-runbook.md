# Xiaoman Weekly Preview — Systemd Design Draft

Updated: 2026-08-07

This document is a **non-executable design draft** for a future reviewed deploy/runner
implementation. It is not an owner-approved production activation path and must not be
used to copy unit files into `/etc/systemd/system`, run `systemctl enable`, or mutate
server cron state by hand.

Production activation may happen only after the unit templates, installer, rollback,
bundle inclusion, and deploy-contract checks land under `deploy/` through reviewed code.

It is **not** production-completion evidence and must not be used to claim real group
delivery. The script only drafts; a human confirms before any Erhua handoff or send.

## Scope

In scope (this design draft):

- Capture the intended release-managed systemd timer shape for review.
- Capture the human confirmation gate; the timer would only print
  `operator_review_message`.
- Record the legacy natural-language Monday task as a future deploy/runner cleanup
  requirement.

Out of scope (deferred, unchanged by this runbook):

- Auto-send, QiWe delivery, feedback forms, material recap, poster generation.
- Edits to `.env`, secrets, cron files, `/etc/systemd/system`, or any production timer.
- Any operator-executed install, rollback, `daemon-reload`, or `systemctl enable`.

## Preconditions (future deploy/runner gates)

1. **Live-read gate.** Confirm the Xiaoman read-through can read both `activity_plan`
   and `activity_occurrence` for the target week on the live host. The script fails
   closed (non-zero exit) if read-through is not enabled.
2. **Deploy contract gate.** Add the unit templates and install/rollback logic to
   `deploy/`, include them in `tools/deploy/build-deploy-bundle.mjs`, and guard them in
   `tools/deploy/check-deploy-contracts.mjs` before any production activation.
3. **Bundle/smoke gate.** The future deploy path must run the aggregate Xiaoman
   production preflight smoke and the `xiaoman-legacy-cron-observation-smoke.sh` on the
   target host. The production baseline expects the legacy runtime cron file to be
   **empty**; activation must not leave a conversation-created Monday cron behind.
4. **Review.** The deploy/runner implementation PR is reviewed and approved. No
   hot-edits to production units.

## Systemd unit design (draft — owner validates parameters)

Modeled on the existing standalone unit in `deploy/runner/`
(`qintopia-agent-os-deploy-runner.{timer,service}`). The weekly preview is a Python
script, not a sidecar subcommand, so it follows the standalone `deploy/runner` precedent
rather than the M9 sidecar renderer (`render-systemd-units.sh`, which only renders fixed
sidecar subcommands).

### Future reviewed parameters (not host-local edits)

- `OnCalendar` — the exact Monday time. Example: `OnCalendar=Mon *-*-* 09:30:00`.
  Confirm it falls after the Sunday 20:00 plan-sheet confirmation. Store the selected
  value in reviewed deploy/runner configuration, not by editing a live unit.
- Secrets env file — the Feishu base token / table id / view id file. Reference it via
  `EnvironmentFile=`; never inline secrets. The reviewed installer must own the allowed
  path.
- `WorkingDirectory` / release path — point at the immutable release checkout
  (`/home/ubuntu/qintopia-agent-os-releases/current/...`); do not use a build or
  standalone checkout path.
- `User` / `Group` — match the other Xiaoman runtime units on the host.
- Runtime interpreter — use an absolute reviewed interpreter path from the production
  runtime package or venv. Never use `/usr/bin/env python3` or any PATH-dependent
  command in the rendered unit.

### `qintopia-xiaoman-weekly-preview.timer` (draft)

```ini
[Unit]
Description=Run Xiaoman weekly activity preview (deterministic draft)

[Timer]
# FUTURE DEPLOY CODE: exact Monday time, after Sunday plan confirmation.
OnCalendar=Mon *-*-* 09:30:00
Persistent=true
Unit=qintopia-xiaoman-weekly-preview.service

[Install]
WantedBy=timers.target
```

### `qintopia-xiaoman-weekly-preview.service` (draft)

```ini
[Unit]
Description=Xiaoman Weekly Activity Preview (deterministic draft)
After=network-online.target
Wants=network-online.target

[Service]
Type=oneshot
# FUTURE DEPLOY CODE: user/group matching other Xiaoman runtime units.
User=ubuntu
Group=ubuntu
# FUTURE DEPLOY CODE: immutable release checkout.
WorkingDirectory=/home/ubuntu/qintopia-agent-os-releases/current
# FUTURE DEPLOY CODE: secrets env file allowlist. Never inline.
EnvironmentFile=/etc/qintopia/xiaoman-activity-readthrough.env
Environment=QINTOPIA_PROFILE_ID=xiaoman
Environment=QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE=1
Environment=QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE=1
# FUTURE DEPLOY CODE: absolute reviewed interpreter + immutable release script path.
ExecStart=<reviewed-absolute-python-interpreter> /home/ubuntu/qintopia-agent-os-releases/current/workflows/xiaoman-weekly-preview/weekly_preview.py
NoNewPrivileges=true
PrivateTmp=true
```

## Future Deployment Requirements

The future executable path must be reviewed code under `deploy/`; this design draft is
not enough. That implementation must:

- render or install the fixed timer/service from repository-owned templates;
- include those templates and scripts in the deploy bundle;
- validate the bundle and unit contract in repository checks;
- use a fixed systemd boundary and reviewed activation/rollback commands;
- render `ExecStart` with an absolute reviewed interpreter and reject `/usr/bin/env`,
  `python3`, shell wrappers, or any PATH-dependent interpreter lookup;
- preflight the interpreter identity and import/runtime dependency set before enabling
  the timer, then retain only sanitized interpreter path/version and dependency status;
- remove or disable the legacy natural-language Monday task only through reviewed
  deploy/runner logic, not by manual `jq`/`mv` edits on the server;
- retain sanitized evidence for timer state, service command, and legacy-cron absence.

## Observation (future deploy/runner evidence)

- `systemctl list-timers qintopia-xiaoman-weekly-preview.timer` shows the next Monday.
- The preflight proves the rendered service uses the reviewed absolute interpreter, not
  a PATH-resolved Python.
- Confirm the output prints `operator_review_message`, never sends, and exits 0 on an
  empty week with "下周暂无已确认活动，暂不生成预告".

## Rollback

Rollback must also be implemented under `deploy/`. It must stop/disable only the
reviewed weekly-preview timer, preserve unrelated production timers, and restore legacy
cron state only through an explicit owner-approved reviewed path. Do not remove unit
files from `/etc/systemd/system` by hand.

## Acceptance

- `external_send_executed` is always `false`; `requires_human_confirmation` is always
  `true`.
- A future reviewed deploy/runner activation removes the legacy natural-language Monday
  cron task from `jobs.json`.
- A future reviewed deploy/runner activation runs the weekly preview as a
  release-managed systemd timer, not a conversation-created cron.
- The aggregate Xiaoman production preflight smoke and legacy-cron smoke both pass.
