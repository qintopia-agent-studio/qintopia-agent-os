# QiWe Fixtures

Replay fixtures for QiWe group/private/mention/send guard behavior.

Fixtures must be synthetic or sanitized. Do not include raw private messages, live
member ids, access tokens, room ids, or production logs.

## Files

- `group-mention.json`: group mention should be eligible for guarded processing.
- `group-no-mention.json`: group text without mention should not trigger Erhua.
- `private-message.json`: private message remains outside group reply behavior.

## Validation

```bash
pnpm test:qiwe
```
