# Xiaoman Fixtures

Replay fixtures for Xiaoman activity signal and activity table behavior.

Do not add raw Feishu table exports, live activity ids, app credentials, or production
database dumps.

## Files

- `activity-signal.json`: minimal activity signal replay.
- `duplicate-signal.json`: idempotency replay for duplicate activity signals.
- `in-event-signal.json`: in-event evidence-only route replay.
- `post-event-signal.json`: post-event evidence and recap-brief route replay.
- `missing-fields-signal.json`: review-needed replay when required signal fields are
  absent.

Each signal fixture's `expected` block is part of the replay contract. It must include
the operation status, phase-derived capability routing, idempotency key, review-needed
flag, missing required fields, and external-send boundary expected from
`xiaoman-activity signal-ingest`.

## Validation

```bash
pnpm workflows:check
pnpm check:runtime
```
