# Security Checks

`tools/security/check-secrets.mjs` is the repository secret and runtime-state gate.

It scans the git-visible working tree and fails on:

- real `.env` files and secret key material
- private key or credential file extensions
- live runtime state such as sessions, caches, logs, SQLite databases, request dumps,
  pid files, and Hermes gateway state files
- high-confidence long credential assignments

Examples and placeholders are allowed when they are clearly marked as examples, fake
values, test values, placeholders, or environment variable references.

Run it with:

```bash
pnpm secrets:check
```
