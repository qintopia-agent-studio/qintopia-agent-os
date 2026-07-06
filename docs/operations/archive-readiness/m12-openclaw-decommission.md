# M12-B OpenClaw Decommission

Updated: 2026-07-06

M12-B removed the remaining OpenClaw rollback path from the production server after the
owner confirmed OpenClaw is no longer used. This batch archived residual files first and
then removed active nginx references to the old `18557` gateway route.

This did not touch WorkTool or the current WorkTool-bound Xiaoqin runtime.

## Scope

Archive directory:

```text
/home/ubuntu/qintopia-agent-os-backups/m12-openclaw-20260706T013020Z
```

Archived items:

| Source path or file                                                         | Result    | Notes                                                                               |
| --------------------------------------------------------------------------- | --------- | ----------------------------------------------------------------------------------- |
| `/opt/qiwe-openclaw-adapter`                                                | archived  | legacy QiWe OpenClaw adapter source                                                 |
| `/etc/qiwe-openclaw-adapter.env`                                            | archived  | secret-bearing env file; contents not logged                                        |
| `/var/lib/qiwe-openclaw-adapter`                                            | archived  | old dedupe state                                                                    |
| `/tmp/openclaw`                                                             | archived  | old OpenClaw log directory                                                          |
| `/etc/systemd/system/qiwe-openclaw-adapter.service`                         | archived  | disabled and inactive before archive                                                |
| `/etc/systemd/system/openclaw-embedding-proxy.service`                      | archived  | disabled and inactive before archive                                                |
| `/etc/systemd/system/oclak-ep.service`                                      | archived  | disabled and inactive before archive                                                |
| `/etc/systemd/system/qintopia-embedding-worker.service`                     | archived  | old disabled Python worker, not current `qintopia-message-embedding-worker.service` |
| `/root/.config/systemd/user/openclaw-gateway.service`                       | archived  | root user OpenClaw gateway unit                                                     |
| `/root/.config/systemd/user/openclaw-gateway.service.bak`                   | archived  | old root user backup unit                                                           |
| `/root/.config/systemd/user/openclaw-gateway.service.d`                     | archived  | old root user override directory                                                    |
| `/etc/nginx/sites-enabled/qintopia.cn.bak-silaoshi-webhook-20260628-165858` | archived  | accidentally enabled backup server config                                           |
| `/etc/nginx/sites-available/qintopia.cn`                                    | backed up | active config backed up before route edit                                           |

## Pre-Cleanup Evidence

Read-only preflight found:

- no OpenClaw process
- no listener on ports `18557` or `8787`
- `/opt/qiwe-openclaw-adapter` still existed
- `/tmp/openclaw` still existed
- `qiwe-openclaw-adapter.service`, `openclaw-embedding-proxy.service`,
  `oclak-ep.service`, and `qintopia-embedding-worker.service` were disabled and inactive
- root user `openclaw-gateway.service` files still existed
- active nginx config still routed `/wecom/agent` and `/plugins/wecom/agent/*` to
  `127.0.0.1:18557`
- `/etc/nginx/sites-enabled/qintopia.cn.bak-silaoshi-webhook-20260628-165858` was a
  regular file under `sites-enabled`, so nginx included it as an active duplicate server
  config

## Changes

- Archived the OpenClaw directories, env, state, logs, and disabled unit files.
- Ran `systemctl daemon-reload` and root user daemon reload after unit archival.
- Removed the four legacy `18557` location blocks from the active
  `/etc/nginx/sites-available/qintopia.cn` config:
  - `location = /wecom/agent`
  - `location ^~ /wecom/agent/`
  - `location = /plugins/wecom/agent/default`
  - `location ^~ /plugins/wecom/agent/`
- Archived the accidentally enabled backup file from `/etc/nginx/sites-enabled`.
- Validated nginx with `nginx -t` before reloading nginx.

## Post-Cleanup Validation

Post-cleanup checks passed:

- OpenClaw paths were removed from their original locations.
- OpenClaw system units returned `not-found` and `inactive`.
- No OpenClaw process was running.
- No listener existed on ports `18557` or `8787`.
- Active nginx config no longer referenced `18557`, `8787`, `openclaw`, `/wecom/agent`,
  or `/plugins/wecom/agent/*`.
- Active nginx config still routed `/qiwe/webhook` to `127.0.0.1:18661/qiwe/webhook`.
- `curl http://127.0.0.1:18661/health` returned HTTP `200`.
- `nginx.service` remained active.
- All nine current `qintopia-message-*` and `qintopia-agentos-*` system services
  remained active.
- Seven Hermes gateway user services remained active.
- `qintopia-message-sidecar check` passed against NATS JetStream and Postgres.

## Rollback

Rollback should restore only the specific missed dependency. Do not restore OpenClaw as
a general rollback path unless the owner explicitly approves that direction.

If the nginx route removal must be reverted:

```bash
sudo cp \
  /home/ubuntu/qintopia-agent-os-backups/m12-openclaw-20260706T013020Z/nginx/qintopia.cn.pre-m12-openclaw \
  /etc/nginx/sites-available/qintopia.cn
sudo nginx -t
sudo systemctl reload nginx
```

If a unit or directory is unexpectedly needed, move that exact archived file or
directory back from the archive path, run `systemctl daemon-reload`, and record the
missed dependency before any further cleanup.

## Remaining Cleanup

OpenClaw cleanup is complete for the known server paths and active nginx references.
Permanent deletion of the archive is not approved in this batch.

The remaining M12 cleanup scope is WorkTool plus the current WorkTool-bound Xiaoqin
runtime.
