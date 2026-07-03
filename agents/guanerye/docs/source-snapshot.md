# Guanerye Source Snapshot

Snapshot date: 2026-07-03
Mode: read-only inventory

## Observed Runtime

- Profile path: `/home/ubuntu/.hermes/profiles/guanerye`
- User service: `hermes-gateway-guanerye.service`
- Key non-secret file names observed: `SOUL.md`, `config.yaml`, `profile.yaml`,
  `channel_directory.json`
- Script name observed for later classification: `send_promo_reminder.sh`

## Adopted As

This package records the Agent boundary only. It excludes runtime `.env`, auth files,
sessions, locks, cron state, caches, and database files.

## Source Inputs

- `../qintopia-agent-os/docs/agent-os/guanerye-engineering-automation-boundary.md`
- Server `docs/agent-os/agents.md`
- Server read-only profile inventory
