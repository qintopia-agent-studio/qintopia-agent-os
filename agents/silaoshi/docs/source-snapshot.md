# Silaoshi Source Snapshot

Snapshot date: 2026-07-03
Mode: read-only inventory

## Observed Runtime

- Profile path: `/home/ubuntu/.hermes/profiles/silaoshi`
- User service: `hermes-gateway-silaoshi.service`
- Key non-secret file names observed: `SOUL.md`, `config.yaml`, `profile.yaml`,
  `webhook_subscriptions.json`, `channel_directory.json`
- Script names observed for later classification include daily brief, daily check,
  weekly report, resident onboarding, and holiday announcement scripts.

## Adopted As

This package keeps only the Agent contract. It excludes runtime `.env`, memories,
reports, auth files, sessions, locks, logs, cron state, and database files.

## Source Inputs

- `../qintopia-agent-os/docs/agent-os/silaoshi-community-operations-boundary.md`
- Server `docs/agent-os/agents.md`
- Server read-only profile inventory
