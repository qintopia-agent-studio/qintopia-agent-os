# Xiaoman Weekly Preview — Production Activation Cutover Runbook

Updated: 2026-08-07

This runbook is the **owner-approved activation path** for promoting
`workflows/xiaoman-weekly-preview` from a merged, `status: active` workflow package into
a live, release-managed Monday timer. It replaces the legacy conversation-created
natural-language Monday cron task.

It is **not** production-completion evidence and must not be used to claim real group
delivery. The script only drafts; a human confirms before any Erhua handoff or send.

## Scope

In scope (this cutover):

- Register a release-managed systemd timer that runs `weekly_preview.py` every Monday.
- Keep the human confirmation gate; the timer only prints `operator_review_message`.
- Remove the old natural-language Monday task from the server `jobs.json`.

Out of scope (deferred, unchanged by this runbook):

- Auto-send, QiWe delivery, feedback forms, material recap, poster generation.
- Edits to `.env`, secrets, or any other production timer.

## Preconditions (owner gates, must pass before install)

1. **Live-read gate.** Confirm the Xiaoman read-through can read both `activity_plan`
   and `activity_occurrence` for the target week on the live host. The script fails
   closed (non-zero exit) if read-through is not enabled.
2. **Bundle/smoke gate.** Run the aggregate Xiaoman production preflight smoke and the
   `xiaoman-legacy-cron-observation-smoke.sh` on the target host. The production
   baseline expects the legacy runtime cron file to be **empty**; this cutover must not
   leave a conversation-created Monday cron behind.
3. **Review.** This cutover PR is reviewed and approved. No hot-edits to production
   units.

## Systemd unit design (draft — owner validates parameters)

Modeled on the existing standalone unit in `deploy/runner/`
(`qintopia-agent-os-deploy-runner.{timer,service}`). The weekly preview is a Python
script, not a sidecar subcommand, so it follows the standalone `deploy/runner` precedent
rather than the M9 sidecar renderer (`render-systemd-units.sh`, which only renders fixed
sidecar subcommands).

### Owner decisions (do NOT commit; set on the host)

- `OnCalendar` — the exact Monday time. Example: `OnCalendar=Mon *-*-* 09:30:00`.
  Confirm it falls after the Sunday 20:00 plan-sheet confirmation.
- Secrets env file — the Feishu base token / table id / view id file. Reference it via
  `EnvironmentFile=`; never inline secrets. Path is host-specific.
- `WorkingDirectory` / release path — point at the immutable release checkout
  (`/home/ubuntu/qintopia-agent-os-releases/current/...`); do not use a build or
  standalone checkout path.
- `User` / `Group` — match the other Xiaoman runtime units on the host.

### `qintopia-xiaoman-weekly-preview.timer` (draft)

```ini
[Unit]
Description=Run Xiaoman weekly activity preview (deterministic draft)

[Timer]
# OWNER DECISION: exact Monday time, after Sunday plan confirmation.
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
# OWNER DECISION: user/group matching other Xiaoman runtime units.
User=ubuntu
Group=ubuntu
# OWNER DECISION: immutable release checkout.
WorkingDirectory=/home/ubuntu/qintopia-agent-os-releases/current
# OWNER DECISION: secrets env file (Feishu token/table id). Never inline.
EnvironmentFile=/etc/qintopia/xiaoman-activity-readthrough.env
Environment=QINTOPIA_PROFILE_ID=xiaoman
Environment=QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE=1
Environment=QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE=1
# OWNER DECISION: python interpreter + immutable release script path.
ExecStart=/usr/bin/env python3 /home/ubuntu/qintopia-agent-os-releases/current/workflows/xiaoman-weekly-preview/weekly_preview.py
NoNewPrivileges=true
PrivateTmp=true
```

## Install procedure (owner-executed on the host)

1. Copy the reviewed `qintopia-xiaoman-weekly-preview.{timer,service}` into
   `/etc/systemd/system`.
2. `sudo systemctl daemon-reload`.
3. `sudo systemctl enable --now qintopia-xiaoman-weekly-preview.timer`.
4. Remove the old natural-language Monday task from the server cron:

   ```bash
   jq 'del(.[] | select(.name == "<legacy-monday-preview-task-name>"))' \
     /home/ubuntu/.hermes/profiles/xiaoman/cron/jobs.json > /tmp/jobs.json && \
     mv /tmp/jobs.json /home/ubuntu/.hermes/profiles/xiaoman/cron/jobs.json
   ```

   Then re-run `deploy/sidecar/scripts/xiaoman-legacy-cron-observation-smoke.sh`; it
   must pass (legacy cron must be empty/absent of the Monday preview task).

## Observation (after install)

- `systemctl list-timers qintopia-xiaoman-weekly-preview.timer` shows the next Monday.
- Forced run for validation (no production send occurs):

  ```bash
  sudo systemctl start qintopia-xiaoman-weekly-preview.service
  journalctl -u qintopia-xiaoman-weekly-preview.service -n 50
  ```

- Confirm the output prints `operator_review_message`, never sends, and exits 0 on an
  empty week with "下周暂无已确认活动，暂不生成预告".

## Rollback

1. `sudo systemctl disable --now qintopia-xiaoman-weekly-preview.timer`.
2. Remove the unit files from `/etc/systemd/system`; `sudo systemctl daemon-reload`.
3. If the legacy Monday cron was removed, restore it only if the owner approves
   reverting to the conversation-created task (temporary operations convenience only).
4. Re-run `xiaoman-legacy-cron-observation-smoke.sh` to confirm the expected state.

## Acceptance

- `external_send_executed` is always `false`; `requires_human_confirmation` is always
  `true`.
- The legacy natural-language Monday cron task is gone from `jobs.json`.
- The weekly preview runs as a release-managed systemd timer, not a conversation-created
  cron.
- The aggregate Xiaoman production preflight smoke and legacy-cron smoke both pass.
