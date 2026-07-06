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

The owner confirmed OpenClaw is no longer used. M12-B archived the remaining server
paths, disabled unit files, root user unit residue, env/state files, and active nginx
routes to `127.0.0.1:18557`.

Archive record:

- `docs/operations/archive-readiness/m12-openclaw-decommission.md`

Do not restart, modify, or migrate OpenClaw. Treat the archive as rollback/audit
material only until the owner approves permanent deletion.
