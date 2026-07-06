# M12-C WorkTool And Xiaoqin WorkTool Runtime Decommission

Updated: 2026-07-06

M12-C archived the remaining WorkTool runtime and the current WorkTool-bound Xiaoqin
Hermes profile. This closes the known legacy cleanup scope for the monorepo migration.

This does not decide future Xiaoqin product direction. A future Xiaoqin Agent remains
possible only through a new non-WorkTool integration and a reviewed Agent package
contract.

## Scope

Archive directory:

```text
/home/ubuntu/qintopia-agent-os-backups/m12-worktool-xiaoqin-20260706T014342Z
```

Archived items:

| Source path or file                                                         | Size | Result   |
| --------------------------------------------------------------------------- | ---- | -------- |
| `/home/ubuntu/worktool-gateway`                                             | 112K | archived |
| `/home/ubuntu/.hermes/profiles/xiaoqin`                                     | 374M | archived |
| `/home/ubuntu/.config/systemd/user/worktool-gateway.service`                | 4K   | archived |
| `/home/ubuntu/.config/systemd/user/hermes-gateway-xiaoqin-worktool.service` | 4K   | archived |

Archive size after move: 374M.

The archived Xiaoqin profile contained live runtime state such as `.env`, memories,
sessions, logs, cache, `state.db`, and auth/runtime files. Those files were not copied
into git and must stay in the private server archive unless the owner explicitly
approves a narrowly scoped extraction.

## Pre-Cleanup Evidence

Read-only preflight found:

- `/home/ubuntu/worktool-gateway` still existed.
- `/home/ubuntu/.hermes/profiles/xiaoqin` still existed.
- `/home/ubuntu/.hermes/profiles/xiaoqin/plugins/worktool-platform` still existed.
- `worktool-gateway.service` was loaded, disabled, and inactive.
- `hermes-gateway-xiaoqin-worktool.service` was loaded, disabled, and inactive.
- `hermes-gateway-xiaoqin.service` was not found.
- no WorkTool or Xiaoqin process was running.
- no listener existed on port `8787`.
- active nginx config did not reference WorkTool, Xiaoqin, or `8787`.
- cron did not reference WorkTool, Xiaoqin, or `8787`.
- active Hermes profiles did not reference WorkTool, Xiaoqin, or `8787`.

## Changes

- Moved `/home/ubuntu/worktool-gateway` into the archive.
- Moved the current WorkTool-bound `/home/ubuntu/.hermes/profiles/xiaoqin` profile into
  the archive.
- Moved the disabled ubuntu user unit files into the archive:
  - `worktool-gateway.service`
  - `hermes-gateway-xiaoqin-worktool.service`
- Ran `systemctl --user daemon-reload` and reset failed user units.

No nginx change was needed in this batch because M12-B had already removed the legacy
OpenClaw/WorkTool route references.

## Post-Cleanup Validation

Post-cleanup checks passed:

- original WorkTool and Xiaoqin WorkTool runtime paths no longer existed.
- `worktool-gateway.service`, `hermes-gateway-xiaoqin-worktool.service`, and
  `hermes-gateway-xiaoqin.service` returned `not-found` and `inactive`.
- no WorkTool or Xiaoqin process was running.
- no listener existed on port `8787`.
- `nginx -t` passed.
- active nginx config did not reference WorkTool, Xiaoqin, or `8787`.
- cron did not reference WorkTool, Xiaoqin, or `8787`.
- active Hermes profile configs did not reference WorkTool, Xiaoqin, or `8787`.
- `nginx.service` remained active.
- all nine current `qintopia-message-*` and `qintopia-agentos-*` system services
  remained active.
- seven Hermes gateway user services remained active.
- `qintopia-message-sidecar check` passed against NATS JetStream and Postgres.

## Rollback

Rollback should restore only the exact missed dependency. Do not restore WorkTool as a
future Agent OS channel, and do not restore the archived WorkTool-bound Xiaoqin runtime
as the future Xiaoqin implementation.

Example rollback for the old WorkTool gateway:

```bash
sudo mv \
  /home/ubuntu/qintopia-agent-os-backups/m12-worktool-xiaoqin-20260706T014342Z/home-ubuntu/worktool-gateway \
  /home/ubuntu/worktool-gateway
mv \
  /home/ubuntu/qintopia-agent-os-backups/m12-worktool-xiaoqin-20260706T014342Z/systemd-user/worktool-gateway.service \
  /home/ubuntu/.config/systemd/user/worktool-gateway.service
systemctl --user daemon-reload
```

Example rollback for the old Xiaoqin WorkTool runtime:

```bash
sudo mv \
  /home/ubuntu/qintopia-agent-os-backups/m12-worktool-xiaoqin-20260706T014342Z/hermes-profiles/xiaoqin \
  /home/ubuntu/.hermes/profiles/xiaoqin
mv \
  /home/ubuntu/qintopia-agent-os-backups/m12-worktool-xiaoqin-20260706T014342Z/systemd-user/hermes-gateway-xiaoqin-worktool.service \
  /home/ubuntu/.config/systemd/user/hermes-gateway-xiaoqin-worktool.service
systemctl --user daemon-reload
```

After any rollback, re-run process, port, user-unit, nginx, cron, Hermes profile, and
sidecar checks before making additional cleanup changes.

## Remaining Cleanup

Known legacy server cleanup for the monorepo migration is complete:

- M12 low-risk paths archived.
- M12-B OpenClaw paths and nginx routes archived or removed.
- M12-C WorkTool and current WorkTool-bound Xiaoqin runtime archived.

Permanent deletion of M12 archives is not approved. Keep the archives until the owner
approves retention cleanup.
