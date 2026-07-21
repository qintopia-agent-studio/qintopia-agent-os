# Xiaoman Source Snapshot

Snapshot date: 2026-07-15 Mode: read-only inventory

## Observed Runtime

- Profile path: `/home/ubuntu/.hermes/profiles/xiaoman`
- User service: `hermes-gateway-xiaoman.service`
- Candidate file names observed: `SOUL.md`, `config.yaml`, `profile.yaml`,
  `webhook_subscriptions.json`, `channel_directory.json`, `cron/jobs.json`
- Related package already adopted: `workflows/activity-promotion`

The observation-only profile bundle records source hashes for all six candidates. Only
`SOUL.md` and `profile.yaml` are template inputs. `config.yaml` contains secret-shaped
provider fields, webhook subscriptions contain live secrets, channel directories contain
real conversation identifiers, and cron state contains a generated timestamp. Those four
files remain runtime-only.

An in-memory read-only parity check on 2026-07-15 extracted the four declared values
from the live `SOUL.md` without printing them, rendered the reviewed template locally,
and reproduced source SHA-256
`4b54c777e09102385665554829df7b1665bde57d28b4c5bc5ce34fd1d052801e`. The checked-in
`profile.yaml` template independently matches its recorded production source hash. This
is source-template evidence, not a release artifact smoke or cutover approval.

## Adopted As

This package keeps the Agent contract and source boundary. It excludes `.env`, live
config values, webhook/channel/cron runtime state, backups, memory files, sessions,
locks, auth files, and database files.

## Source Inputs

- Server `docs/agent-os/agents.md`
- Runtime sidecar `xiaoman_activity` contract and acceptance smokes
- Server read-only profile inventory
