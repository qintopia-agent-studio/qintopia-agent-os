# Huabaosi Source Snapshot

Snapshot date: 2026-07-03
Mode: read-only inventory

## Observed Runtime

- Profile path: `/home/ubuntu/.hermes/profiles/huabaosi`
- User service: `hermes-gateway-huabaosi.service`
- Key non-secret file names observed: `SOUL.md`, `config.yaml`, `profile.yaml`,
  `processes.json`, `channel_directory.json`
- Related workflow package: `workflows/activity-promotion`

## Adopted As

This package records the draft Agent boundary only. Server-side shadow/Rust material is
not promoted to approved direction. Runtime `.env`, memories, auth files, sessions,
locks, logs, caches, process state, and database files are excluded.

## Source Inputs

- Server `docs/agent-os/agents.md`
- Operations control-plane visual asset workflow docs
- Server read-only profile inventory
