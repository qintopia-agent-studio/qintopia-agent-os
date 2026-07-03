# Default Source Snapshot

Snapshot date: 2026-07-03 Mode: read-only inventory

## Observed Runtime

- Profile root: `/home/ubuntu/.hermes`
- User service: `hermes-gateway.service`
- Role source: server `docs/agent-os/agents.md`

## Adopted As

This package keeps only the dispatcher contract. It does not copy Hermes runtime state,
history, caches, credentials, logs, or profile-local generated files.

## Open Questions

- Which requests should go through the future Feishu Bot entrypoint.
- Which workflow creation actions require final human confirmation.
