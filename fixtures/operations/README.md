# Operations Fixtures

Replay fixtures for operations control-plane workflows.

Fixtures here should describe state transitions and expected guardrails only. Do not add
private workbench rows, live Feishu records, internal raw chat logs, or production
database dumps.

## Files

- `control-plane-request.json`: a minimal activity promotion request replay.

## Validation

```bash
pnpm workflows:check
```
