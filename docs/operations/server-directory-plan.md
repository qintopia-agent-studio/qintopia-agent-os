# Server Directory Plan

Updated: 2026-07-04

This document records the intended server filesystem shape for Qintopia Agent OS. It
separates the current transitional state from the target release model so future
operators do not reintroduce ad hoc server checkouts, manual builds, or direct profile
edits.

## Direction

The server should be a deployment target, not a development workspace.

The long-term deployment model is:

- GitHub CI builds reviewed artifacts from an approved `master` commit SHA.
- The server downloads and verifies artifacts with GitHub App credentials.
- Each approved release is stored under an immutable SHA directory.
- Runtime services point at a stable `current` symlink.
- Hermes live state remains under `.hermes`; only reviewed, versioned files are mounted
  or linked from the release.
- Rollback switches `current` back to the previous release and restarts approved
  services.

## Target Shape

```text
/home/ubuntu/
  .hermes/
    hermes-agent/
    profiles/
      erhua/
      xiaoman/
      wenyuange/
      silaoshi/
      guanerye/
      huabaosi/
    logs/
    cache/
    sessions/
    state/

  qintopia-agent-os-monorepo/
  qintopia-agent-os-releases/
    <approved-sha>/
      manifest.json
      sidecar/
        qintopia-message-sidecar
        SHA256SUMS
      runtime/
        postgres/
          migrations/
      agents/
      skills/
      workflows/
      mcp/
      deploy/
    current -> <approved-sha>
    previous -> <previous-approved-sha>

  qintopia-agent-os-backups/
    <date-or-window>/

/etc/qintopia/
  message-sidecar.env
  github-app/
    qintopia-agent-os-deployer.pem
```

`qintopia-agent-os-artifacts/<sha>` is the current transition path for CI sidecar
artifacts. It should either become a download cache or be replaced by
`qintopia-agent-os-releases/<sha>` after the release/current model is implemented.

## Current Transitional Directories

| Path                                       | Classification       | Current use                                                   | Target disposition                                            |
| ------------------------------------------ | -------------------- | ------------------------------------------------------------- | ------------------------------------------------------------- |
| `/home/ubuntu/qintopia-agent-os-monorepo`  | current architecture | server checkout for runbooks, scripts, migrations, and docs   | keep as deploy checkout                                       |
| `/home/ubuntu/qintopia-agent-os-artifacts` | transition artifact  | verified CI sidecar binaries by SHA                           | replace or fold into `qintopia-agent-os-releases`             |
| `/home/ubuntu/qintopia-agent-os-backups`   | current architecture | systemd and rollback backups created during M9                | keep with retention policy                                    |
| `/etc/qintopia`                            | current architecture | production env/config and GitHub App key material             | keep; never copy into git                                     |
| `/home/ubuntu/.hermes`                     | Hermes live runtime  | Hermes core, profiles, logs, sessions, cache, skills, scripts | keep; move versioned files to release-managed links over time |

## Legacy Or Mixed-State Directories

These paths came from the previous mixed deployment model. Do not delete them until all
runtime references are removed and an owner-approved archive window is active.

| Path                                          | Why it still matters                                 | Required migration before cleanup                               |
| --------------------------------------------- | ---------------------------------------------------- | --------------------------------------------------------------- |
| `/home/ubuntu/qintopia-msg-sidecar`           | still used by legacy AgentOS workers and MCP context | M9-F repoint workers and Hermes MCP command to approved release |
| `/home/ubuntu/qintopia-agent-os`              | server-side Rust/docs exploration                    | archive or delete after owner review; do not use as source      |
| `/home/ubuntu/qintopia-hermes-runtime`        | old Hermes runtime/template attempt, dirty git state | review for unique evidence, then archive or delete              |
| `/home/ubuntu/qintopia-message-sidecar-build` | old build workspace                                  | archive or delete after confirming no service reference         |
| `/home/ubuntu/qintopia-artifacts`             | old artifact/output directory                        | archive or delete after content review                          |
| `/home/ubuntu/qintopia-migration`             | older migration evidence                             | archive evidence or delete after owner review                   |
| `/home/ubuntu/qintopia-worklog-guard-*`       | historical worklog guard run directories             | archive or delete after confirming no timer/process reference   |
| `/home/ubuntu/worktool-gateway`               | deprecated WorkTool runtime                          | cleanup under WorkTool decommission plan                        |
| `/home/ubuntu/worktool-gateway-old`           | deprecated WorkTool backup                           | cleanup under WorkTool decommission plan                        |
| `/home/ubuntu/.hermes/profiles/xiaoqin`       | deprecated Xiaoqin WorkTool profile                  | archive or delete after service/config recheck                  |
| `/opt/qiwe-openclaw-adapter`                  | deprecated OpenClaw adapter                          | cleanup with disabled unit and nginx route reconciliation       |

M9-F must remove these active service references before
`/home/ubuntu/qintopia-msg-sidecar` becomes eligible for archive:

- `qintopia-agentos-member-profile-worker.service`
- `qintopia-agentos-graph-projection-worker.service`
- `qintopia-agentos-raw-archive-worker.service`
- `qintopia-agentos-event-signal-worker.service`
- `qintopia-agentos-daily-digest-worker.service`
- `qintopia-agentos-daily-digest-publisher.service`

## Hermes Runtime Boundary

Hermes itself is not rebuilt by the Agent OS CI pipeline. Hermes remains the runtime
process manager for profile-bound Agents. Agent OS updates are mounted into Hermes
through reviewed profile files, plugins, scripts, MCP commands, and backend services.

Do not replace a whole profile directory from CI. Preserve live state:

- `.env`
- sessions
- logs
- cache
- state databases
- auth files
- runtime-generated memory

Versioned files should move to the release model:

- `SOUL.md`
- `config.yaml`
- `mcp.json` or equivalent MCP command config
- reviewed profile scripts
- profile-local plugins
- channel policies and non-secret mapping files

For example, the target Erhua shape is:

```text
/home/ubuntu/.hermes/profiles/erhua/
  .env
  sessions/
  logs/
  cache/
  SOUL.md -> /home/ubuntu/qintopia-agent-os-releases/current/agents/erhua/SOUL.md
  config.yaml -> /home/ubuntu/qintopia-agent-os-releases/current/agents/erhua/config.yaml
  plugins/qiwe-platform -> /home/ubuntu/qintopia-agent-os-releases/current/skills/qiwe
```

## Cleanup Rule

Before removing any legacy directory:

1. Confirm no process references it.
2. Confirm no systemd unit or timer references it.
3. Confirm no nginx route or cron job references it.
4. Confirm rollback no longer needs it.
5. Archive first under an owner-approved dated path.
6. Record the archive path and validation output in git.

Permanent deletion happens only after the owner confirms the archive is no longer needed
for rollback or audit.
