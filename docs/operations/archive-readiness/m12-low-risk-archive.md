# M12 Low-Risk Legacy Archive

Updated: 2026-07-06

M12 first batch archived the low-risk legacy directories that M11 had already marked as
archive-ready. This batch moved files into a dated backup directory only. It did not
delete files permanently and did not touch WorkTool, the current WorkTool-bound Xiaoqin
runtime, or OpenClaw decommission paths.

## Scope

Owner approval: confirmed in chat before execution.

Archive directory:

```text
/home/ubuntu/qintopia-agent-os-backups/m12-low-risk-20260706T011023Z
```

Archived paths:

| Source path                                                     | Size before archive | Result   |
| --------------------------------------------------------------- | ------------------- | -------- |
| `/home/ubuntu/qintopia-msg-sidecar`                             | 39M                 | archived |
| `/home/ubuntu/qintopia-agent-os`                                | 6.9M                | archived |
| `/home/ubuntu/qintopia-hermes-runtime`                          | 284K                | archived |
| `/home/ubuntu/qintopia-message-sidecar-build`                   | 152K                | archived |
| `/home/ubuntu/qintopia-artifacts`                               | 336K                | archived |
| `/home/ubuntu/qintopia-migration`                               | 281M                | archived |
| `/home/ubuntu/worktool-gateway-old`                             | 104K                | archived |
| `/home/ubuntu/qintopia-worklog-guard-20260625-xiaoman-dispatch` | 36K                 | archived |
| `/home/ubuntu/qintopia-worklog-guard-20260626`                  | 32K                 | archived |
| `/home/ubuntu/qintopia-worklog-guard-20260627`                  | 28K                 | archived |
| `/home/ubuntu/qintopia-worklog-guard-20260628`                  | 32K                 | archived |
| `/home/ubuntu/qintopia-worklog-guard-20260703`                  | 32K                 | archived |
| `/home/ubuntu/qintopia-worklog-guard-20260705`                  | 32K                 | archived |

Archive size after move: 327M.

## Pre-Archive Gate

The M12 preflight repeated the M11 read-only reference checks immediately before moving
files:

- no process references for the source paths
- no systemd system unit references
- no systemd user unit references
- no cron references
- no nginx references
- no Hermes profile references in `config.yaml`, `SOUL.md`, `profile.yaml`, or
  `mcp.json`

The `/home/ubuntu/qintopia-agent-os` check used exact path matching so
`/home/ubuntu/qintopia-agent-os-releases/current` did not count as a legacy source
checkout reference.

## Post-Archive Validation

Release symlinks were unchanged:

```text
current  -> /home/ubuntu/qintopia-agent-os-releases/16496c8d4bfb13ed26d080727a4c812f9c2e0487
previous -> /home/ubuntu/qintopia-agent-os-releases/99681909149fde4f16daa3af941a750d1f239860
```

Archived source paths no longer existed under their original locations.

The not-in-this-batch paths remained present for later decommission work:

- `/home/ubuntu/worktool-gateway`
- `/home/ubuntu/.hermes/profiles/xiaoqin`
- `/opt/qiwe-openclaw-adapter`

Active system services after archive:

- `qintopia-message-sidecar.service`
- `qintopia-message-embedding-worker.service`
- `qintopia-message-identity-worker.service`
- `qintopia-agentos-member-profile-worker.service`
- `qintopia-agentos-graph-projection-worker.service`
- `qintopia-agentos-raw-archive-worker.service`
- `qintopia-agentos-event-signal-worker.service`
- `qintopia-agentos-daily-digest-worker.service`
- `qintopia-agentos-daily-digest-publisher.service`

Active Hermes user services after archive:

- `hermes-gateway.service`
- `hermes-gateway-erhua.service`
- `hermes-gateway-xiaoman.service`
- `hermes-gateway-wenyuange.service`
- `hermes-gateway-silaoshi.service`
- `hermes-gateway-huabaosi.service`
- `hermes-gateway-guanerye.service`

Sidecar health check passed:

```json
{
  "nats_url": "nats://127.0.0.1:4222",
  "ok": true,
  "postgres_checked": true,
  "stream": "QINTOPIA_QIWE_MESSAGES"
}
```

## Rollback

If a later issue proves that a path in this batch is still needed, restore only that
path from the archive directory. Do not restore the whole batch unless the failure mode
requires it.

Example:

```bash
sudo mv \
  /home/ubuntu/qintopia-agent-os-backups/m12-low-risk-20260706T011023Z/qintopia-msg-sidecar \
  /home/ubuntu/qintopia-msg-sidecar
```

After rollback, re-run the affected service or process check and record the reason in
this document before doing any further cleanup.

## Remaining Cleanup

These paths are intentionally not archived in this batch:

| Path                                    | Reason                                                        | Next action                                 |
| --------------------------------------- | ------------------------------------------------------------- | ------------------------------------------- |
| `/home/ubuntu/worktool-gateway`         | disabled unit references still exist                          | WorkTool decommission batch                 |
| `/home/ubuntu/.hermes/profiles/xiaoqin` | current WorkTool-bound Xiaoqin runtime and disabled unit refs | WorkTool-bound Xiaoqin runtime decommission |
| `/opt/qiwe-openclaw-adapter`            | disabled OpenClaw unit and route references                   | OpenClaw decommission and nginx recheck     |

Permanent deletion of this M12 archive is not approved. Keep it until the owner approves
post-archive retention cleanup.
