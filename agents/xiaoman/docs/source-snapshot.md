# Xiaoman Source Snapshot

Snapshot date: 2026-07-03 Mode: read-only inventory

## Observed Runtime

- Profile path: `/home/ubuntu/.hermes/profiles/xiaoman`
- User service: `hermes-gateway-xiaoman.service`
- Key non-secret file names observed: `SOUL.md`, `config.yaml`, `profile.yaml`,
  `webhook_subscriptions.json`, `channel_directory.json`
- Related package already adopted: `workflows/activity-promotion`

## Adopted As

This package keeps the Agent contract and source boundary. It excludes `.env`, backups,
memory files, webhook runtime state, sessions, locks, auth files, and database files.

## Source Inputs

- Server `docs/agent-os/agents.md`
- Runtime sidecar `xiaoman_activity` contract and acceptance smokes
- Server read-only profile inventory
