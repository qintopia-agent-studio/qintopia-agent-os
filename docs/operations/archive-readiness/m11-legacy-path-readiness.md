# M11 Legacy Path Archive Readiness

Updated: 2026-07-05

M11 only marks legacy paths as archive-ready or not-ready. It does not move, delete, or
archive anything. M12 is the first phase allowed to perform cleanup, and only after
owner approval for a specific batch.

## Checks Used

Read-only checks were run against the server:

- process command lines
- system systemd unit files
- user systemd unit files
- enabled/active unit state
- cron files
- nginx config
- Hermes profile `config.yaml`, `SOUL.md`, and `profile.yaml`
- path existence and size summary

Exact path matching was used for `/home/ubuntu/qintopia-agent-os` so valid
`/home/ubuntu/qintopia-agent-os-releases/current` references do not count as legacy
references.

## Archive-Ready Candidates

These paths exist and had no process, systemd, user systemd, cron, nginx, or Hermes
profile config references in the M11 read-only scan.

| Path                                          | Size | M11 mark      | Notes                                         |
| --------------------------------------------- | ---- | ------------- | --------------------------------------------- |
| `/home/ubuntu/qintopia-msg-sidecar`           | 39M  | archive-ready | M9-F removed active worker and MCP references |
| `/home/ubuntu/qintopia-agent-os`              | 6.9M | archive-ready | Server-side exploration; not source of truth  |
| `/home/ubuntu/qintopia-hermes-runtime`        | 284K | archive-ready | Old Hermes runtime/template attempt           |
| `/home/ubuntu/qintopia-message-sidecar-build` | 152K | archive-ready | Old build workspace                           |
| `/home/ubuntu/qintopia-artifacts`             | 336K | archive-ready | Old artifact/output directory                 |
| `/home/ubuntu/qintopia-migration`             | 281M | archive-ready | Older migration evidence                      |
| `/home/ubuntu/qintopia-worklog-guard-*`       | 160K | archive-ready | Historical worklog guard run directories      |
| `/home/ubuntu/worktool-gateway-old`           | 104K | archive-ready | Deprecated WorkTool backup                    |

Archive-ready means eligible for an owner-approved M12 archive batch. It does not mean
delete now.

## Not Ready For Archive

These paths have disabled unit-file or profile references that should be handled as a
single decommission batch before archival.

| Path                                    | M11 mark                    | Blocking evidence                                                   |
| --------------------------------------- | --------------------------- | ------------------------------------------------------------------- |
| `/home/ubuntu/worktool-gateway`         | decommission-batch-required | Referenced by disabled user unit `worktool-gateway.service`         |
| `/home/ubuntu/.hermes/profiles/xiaoqin` | decommission-batch-required | Referenced by disabled Xiaoqin WorkTool user unit and profile files |
| `/opt/qiwe-openclaw-adapter`            | decommission-batch-required | Referenced by disabled system unit `qiwe-openclaw-adapter.service`  |

Related disabled OpenClaw units observed:

- `qiwe-openclaw-adapter.service`: inactive, disabled
- `openclaw-embedding-proxy.service`: inactive, disabled
- `oclak-ep.service`: inactive, disabled
- `qintopia-embedding-worker.service`: inactive, disabled

Related disabled WorkTool/Xiaoqin user units observed:

- `worktool-gateway.service`: inactive, disabled
- `hermes-gateway-xiaoqin-worktool.service`: inactive, disabled

## Current Release State During M11

```text
current  -> /home/ubuntu/qintopia-agent-os-releases/16496c8d4bfb13ed26d080727a4c812f9c2e0487
previous -> /home/ubuntu/qintopia-agent-os-releases/99681909149fde4f16daa3af941a750d1f239860
```

Huabaosi `qintopia-base-read` resolves to:

```text
/home/ubuntu/qintopia-agent-os-releases/16496c8d4bfb13ed26d080727a4c812f9c2e0487/skills/feishu-base
```

## M12 Gate

Before M12 cleanup starts, choose an explicit batch:

1. Low-risk archive-ready legacy directories.
2. WorkTool/Xiaoqin disabled-unit decommission batch.
3. OpenClaw disabled-unit and route decommission batch.

Each M12 batch must record:

- archive path
- commands run
- owner approval
- pre/post service state
- rollback command
- final docs/changelog update
