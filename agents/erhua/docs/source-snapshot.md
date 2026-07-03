# Erhua Source Snapshot

Snapshot date: 2026-07-03
Mode: read-only inventory

## Observed Runtime

- Profile path: `/home/ubuntu/.hermes/profiles/erhua`
- User service: `hermes-gateway-erhua.service`
- Key non-secret file names observed: `SOUL.md`, `config.yaml`,
  `activity-feishu-mapping.json`, `activity-reminder-policy.json`,
  `channel_directory.json`
- Related plugin source already adopted separately: `skills/qiwe`

## Adopted As

This package records the Agent contract and runtime boundary only. It does not copy
`.env`, memory files, identity caches, gateway state, auth files, locks, logs, sessions,
or database files.

## Source Inputs

- Local Agent OS contract docs under `../qintopia-agent-os/docs/agent-os/`
- Sidecar Erhua trainer-memory implementation and data-design notes
- Server read-only profile inventory
