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
- GitHub CI uploads reviewed artifacts to Tencent COS for server-side distribution.
- The server downloads artifacts from COS and verifies manifest plus checksums before
  repointing services.
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
  cos-artifacts.env
```

`qintopia-agent-os-artifacts/<sha>` is the current transition path for CI sidecar
artifacts after they are pulled from COS. It should either become a download cache or be
replaced by `qintopia-agent-os-releases/<sha>` after the release/current model is
implemented.

## Current Transitional Directories

| Path                                       | Classification       | Current use                                                    | Target disposition                                            |
| ------------------------------------------ | -------------------- | -------------------------------------------------------------- | ------------------------------------------------------------- |
| `/home/ubuntu/qintopia-agent-os-monorepo`  | current architecture | server checkout for runbooks, scripts, migrations, and docs    | keep as deploy checkout                                       |
| `/home/ubuntu/qintopia-agent-os-artifacts` | transition artifact  | verified CI sidecar binaries by SHA                            | replace or fold into `qintopia-agent-os-releases`             |
| `/home/ubuntu/qintopia-agent-os-backups`   | current architecture | systemd and rollback backups created during M9                 | keep with retention policy                                    |
| `/etc/qintopia`                            | current architecture | production env/config, COS config, and GitHub App key material | keep; never copy into git                                     |
| `/home/ubuntu/.hermes`                     | Hermes live runtime  | Hermes core, profiles, logs, sessions, cache, skills, scripts  | keep; move versioned files to release-managed links over time |

## Legacy Or Mixed-State Directories

These paths came from the previous mixed deployment model. Do not delete them until all
runtime references are removed and an owner-approved archive window is active.

| Path                                          | Current status                                               | Required migration before cleanup                       |
| --------------------------------------------- | ------------------------------------------------------------ | ------------------------------------------------------- |
| `/home/ubuntu/qintopia-msg-sidecar`           | archived in M12 low-risk batch                               | keep archive until retention deletion is owner-approved |
| `/home/ubuntu/qintopia-agent-os`              | archived in M12 low-risk batch                               | keep archive until retention deletion is owner-approved |
| `/home/ubuntu/qintopia-hermes-runtime`        | archived in M12 low-risk batch                               | keep archive until retention deletion is owner-approved |
| `/home/ubuntu/qintopia-message-sidecar-build` | archived in M12 low-risk batch                               | keep archive until retention deletion is owner-approved |
| `/home/ubuntu/qintopia-artifacts`             | archived in M12 low-risk batch                               | keep archive until retention deletion is owner-approved |
| `/home/ubuntu/qintopia-migration`             | archived in M12 low-risk batch                               | keep archive until retention deletion is owner-approved |
| `/home/ubuntu/qintopia-worklog-guard-*`       | archived in M12 low-risk batch                               | keep archive until retention deletion is owner-approved |
| `/home/ubuntu/worktool-gateway`               | deprecated runtime; still separate batch                     | cleanup under WorkTool decommission plan                |
| `/home/ubuntu/worktool-gateway-old`           | archived in M12 low-risk batch                               | keep archive until retention deletion is owner-approved |
| `/home/ubuntu/.hermes/profiles/xiaoqin`       | current WorkTool-bound Xiaoqin runtime; still separate batch | archive current runtime after service/config recheck    |
| `/opt/qiwe-openclaw-adapter`                  | archived in M12-B OpenClaw batch                             | keep archive until retention deletion is owner-approved |

M12 first low-risk archive path:

```text
/home/ubuntu/qintopia-agent-os-backups/m12-low-risk-20260706T011023Z
```

M12-B OpenClaw archive path:

```text
/home/ubuntu/qintopia-agent-os-backups/m12-openclaw-20260706T013020Z
```

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

## Release Promotion Contract

Routine release promotion should not require the server to pull repository code. The
server receives reviewed payloads from COS, verifies them, assembles an immutable
release directory, then repoints `current`.

Minimum promotion sequence:

1. Download the approved SHA artifact from COS into a staging or cache directory.
2. Verify `artifact-manifest.json`, `SHA256SUMS`, and binary self-checks.
3. Assemble `/home/ubuntu/qintopia-agent-os-releases/<approved-sha>`.
4. Validate the release directory without changing `current`.
5. Update `previous` to the old `current` target.
6. Atomically switch `current` to `<approved-sha>`.
7. Restart only the approved services or Hermes profile processes.
8. Record checks, release SHA, previous SHA, and rollback command in git.

Repository fetches on the server are reserved for deploy runner bootstrap or approved
runner upgrades, not normal Agent OS runtime releases.

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
