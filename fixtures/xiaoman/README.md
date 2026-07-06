# Xiaoman Fixtures

Replay fixtures for Xiaoman activity signal and activity table behavior.

Do not add raw Feishu table exports, live activity ids, app credentials, or production
database dumps.

## Files

- `activity-signal.json`: minimal activity signal replay.
- `duplicate-signal.json`: idempotency replay for duplicate activity signals.

## Validation

```bash
pnpm workflows:check
pnpm check:runtime
```
