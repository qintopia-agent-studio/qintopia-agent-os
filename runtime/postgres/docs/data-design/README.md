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
- `2026-06-26.003`: `2026-06-26-identity-observations-v3.md`
- `2026-06-26.004`: `2026-06-26-profile-digest-archive-v1.md`
- `2026-06-27.005`: `2026-06-27-event-signals-v2.md`
- `2026-06-29.006`: `2026-06-29-erhua-training-memory.md`
- `2026-06-30.007`: `2026-06-30-operations-control-plane.md`
- `2026-07-02.001`: `2026-07-02-operations-human-actor-guards.md`
- `2026-07-13.002`: `2026-07-13-huabaosi-image-generation.md`
- `2026-07-14.001`: `2026-07-14-xiaoman-event-signal-mutations.md`
