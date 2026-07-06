# Inventory Tools

`tools/inventory` is the package boundary for read-only local and server inventory
collection.

Inventory tools must:

- run read-only by default;
- report paths, owners, process/service references, and cleanup candidates;
- avoid copying secrets, live `.env` files, private logs, sessions, caches, auth files,
  raw chat logs, or runtime databases into git;
- separate evidence collection from deletion or server mutation.

## Validation

```bash
pnpm tools:inventory:check
```
