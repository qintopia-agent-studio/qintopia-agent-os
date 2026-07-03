# Deprecated: OpenClaw

OpenClaw is a legacy QiWe rollback path, not a future Agent OS integration path.

## Source

- Server source observed on 2026-07-03: `/opt/qiwe-openclaw-adapter`
- Server state observed on 2026-07-03: directory exists, no matching systemd service or
  timer found by read-only scan

## Decision

Keep OpenClaw as rollback/audit reference until the owner confirms it can be removed. Do
not restart, modify, or migrate OpenClaw unless explicitly requested.
