# Data Design

This directory records versioned database design decisions for the Qintopia message and
Agent OS data layer.

## Rules

- Every schema migration must have a matching design note in this directory.
- Every schema migration must add one entry to `CHANGELOG.md`.
- Every applied migration should insert or update a row in
  `qintopia_agent_os.schema_change_log`.
- Design notes should explain purpose, scope, compatibility, table ownership, privacy
  boundaries, and follow-up work.
- Database credentials and environment-specific URLs must not be committed.

## Versions

- Change history: `CHANGELOG.md`
- `2026-06-18.001`: `2026-06-18-message-capture-v1.md`
- `2026-06-24.002`: `2026-06-24-agent-os-data-layer-v2.md`
