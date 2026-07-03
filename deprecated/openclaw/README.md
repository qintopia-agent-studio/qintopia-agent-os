# Deprecated: OpenClaw

OpenClaw is a legacy QiWe rollback path, not a future Agent OS integration path.

## Source

- Server source observed on 2026-07-03: `/opt/qiwe-openclaw-adapter`
- Server state observed on 2026-07-03:
  - `/opt/qiwe-openclaw-adapter` exists.
  - `qiwe-openclaw-adapter.service` is loaded, disabled, and inactive.
  - `openclaw-embedding-proxy.service` is loaded, disabled, and inactive.
  - root user `openclaw-gateway.service` is loaded, enabled, and inactive/dead.
  - no listener was observed on ports `18557` or `8787`.

## Decision

Keep OpenClaw as rollback/audit reference until the owner confirms it can be removed. Do
not restart, modify, or migrate OpenClaw unless explicitly requested.
