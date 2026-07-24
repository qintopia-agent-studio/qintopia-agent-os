# Release/Current Model

This document defines the active release/current production model for Agent OS runtime
payloads. M9-F removed the remaining active sidecar worker and Hermes MCP references to
`/home/ubuntu/qintopia-msg-sidecar`; that old checkout is now archive evidence, not a
runtime source.

## Direction

The server is a deployment target, not a build or development workspace. Routine Agent
OS releases use reviewed artifacts from COS:

```text
GitHub CI
  -> build from approved master commit
  -> upload artifact to Tencent COS
  -> server downloads artifact from COS
  -> server verifies manifest and checksums
  -> server assembles immutable release directory
  -> current symlink switches to the new release
  -> approved services or Hermes processes restart
```

Routine releases must not depend on server-side `git fetch`, `git checkout`, local Rust
builds, `scp` source overwrites, or direct edits under `.hermes`.

## Directory Shape

```text
/home/ubuntu/qintopia-agent-os-releases/
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
```

`/home/ubuntu/qintopia-agent-os-artifacts/<sha>` is only an artifact cache and audit
evidence. Services should use `/home/ubuntu/qintopia-agent-os-releases/current`.

## Release Payloads

The M9-F release established the release assembly pattern from two verified COS inputs:
the sidecar runtime artifact and the deploy bundle. A release directory should contain:

```text
sidecar/
  qintopia-message-sidecar
  SHA256SUMS
runtime/postgres/migrations/
deploy/
docs/
manifest.json
```

Later payloads should follow the same release directory contract:

| Payload                         | Runtime effect                                                          |
| ------------------------------- | ----------------------------------------------------------------------- |
| `sidecar-runtime`               | Updates sidecar binary, worker commands, and migration source           |
| `hermes-profile-bundle-<agent>` | Updates reviewed non-secret profile files such as `SOUL.md` or config   |
| `skill-bundle-<skill>`          | Updates reviewed Hermes plugins or skill packages such as `skills/qiwe` |
| `workflow-bundle-<workflow>`    | Updates reviewed scheduled or cross-Agent workflow scripts              |
| `mcp-bundle-<adapter>`          | Updates reviewed MCP wrappers, command configs, and adapters            |

Hermes itself is not rebuilt by this repository. Hermes remains the runtime. This
repository controls the versioned files, plugins, scripts, MCP commands, and backend
services that Hermes mounts or executes.

## Promotion Sequence

Before merging the Release Please PR or publishing its draft Release, complete
[`release-acceptance-checklist.md`](release-acceptance-checklist.md). The checklist is
the guardrail for exact-head Release Please validation, tag freshness, deploy-bundle
payload coverage, post-deploy release-current convergence, and Xiaoman completion claim
boundaries.

1. Confirm the target SHA has successful `check` and `sidecar-artifact` CI jobs.
2. Download the target SHA artifact from COS into a staging or cache directory.
3. Verify `artifact-manifest.json`, `SHA256SUMS`, and binary self-checks.
4. Assemble `/home/ubuntu/qintopia-agent-os-releases/<approved-sha>`.
5. Validate the release directory without changing `current`.
6. Record the current symlink target as the rollback SHA.
7. Update `previous` to the old `current` target.
8. Atomically switch `current` to `<approved-sha>`.
9. Render and install the fixed systemd unit allowlist from the immutable release, then
   enable internal AgentOS worker timers.
10. Restart only owner-approved services or Hermes profile processes.
11. Record the release SHA, previous SHA, checks, and rollback command in git.

The Huabaosi image-generation worker is an external provider boundary, not an internal
timer. A release may install its preflight, worker, and timer units, but the ordinary
installer must leave that timer disabled. After the owner manually publishes the
Release, production configuration must bind the enablement to that exact release SHA and
database URL hash. Run the release-local one-shot production canary first with the timer
inactive; it binds one pending brief, the fixed `trainer` reviewer, one new request, one
pending Feishu-backed JPEG, and authenticated same-byte revalidation to the immutable
release evidence. It does not approve the generated image or call QiWe. Enable ongoing
scheduling through the separate activation script only after that canary is reviewed.

The normal `release.published` deploy path remains the ordinary Huabaosi production
artifact only. It builds/uploads `qintopia-message-sidecar-linux-x86_64-gnu`, records
`runtime_artifact_profile=huabaosi-production`, and must not auto-switch to the
independent QiWe production artifact. A later QiWe enablement deploy is a separate
owner-approved `workflow_dispatch` follow-up that first publishes
`qintopia-message-sidecar-qiwe-production-linux-x86_64-gnu` to COS, then dispatches
`Deploy Production` with `runtime_artifact_profile=qiwe-production`.

The release assembly step should be idempotent: if the release directory already exists,
the operator must verify its manifest and checksum instead of overwriting it blindly.

## Completion Claims

Publishing a Release is not itself proof that a product workflow is usable end to end.
For Xiaoman, classify the Release before merge as `infrastructure`, `activation-ready`,
or `production-complete` using
[`docs/plans/active/xiaoman-production-completion-gate.md`](../plans/active/xiaoman-production-completion-gate.md).

An infrastructure Release may be published to ship staging artifacts, provisioners,
deployment fixes, smoke checks, or guarded activation scripts. Its release notes and PR
body must not claim Xiaoman production completion while Huabaosi staging evidence, QiWe
staging upload/callback/send evidence, QiWe production enablement, production activation
evidence, or one real end-to-end activity is still missing.

## Runtime References

Systemd services should use:

```text
WorkingDirectory=/home/ubuntu/qintopia-agent-os-releases/current
Environment=QINTOPIA_SIDECAR_MIGRATIONS_DIR=/home/ubuntu/qintopia-agent-os-releases/current/runtime/postgres/migrations
Environment=QINTOPIA_DEPLOYED_COMMIT_SHA=<approved-sha>
ExecStart=/home/ubuntu/qintopia-agent-os-releases/current/sidecar/qintopia-message-sidecar <subcommand>
```

After each promotion, render and reinstall these units from the immutable release so
`QINTOPIA_DEPLOYED_COMMIT_SHA` matches the release behind `current`. Do not leave stale
deployment metadata in a unit merely because its path uses the `current` symlink.

Huabaosi image-generation services additionally render both
`QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_RELEASE_SHA` and
`QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA` from the same approved release SHA.
Feishu-mirror-only services render only the Feishu binding. The read-only Huabaosi image
observation derives all three release-bound values from the verified `current` target
before invoking the immutable sidecar, while retaining the reviewed persistent image
approval, database hash, timeout, and media bound used by the production preflight.
Persistent environment files remain the reviewed source for secrets, allowlists,
enablement, and approval values; they are not the deployment source of truth for the
active release identity.

Hermes profile directories remain live runtime state. Preserve:

- `.env`
- sessions
- logs
- cache
- state databases
- auth files
- runtime-generated memory

Reviewed profile files should be linked or mounted from `current`, for example:

```text
/home/ubuntu/.hermes/profiles/erhua/SOUL.md
  -> /home/ubuntu/qintopia-agent-os-releases/current/agents/erhua/SOUL.md

/home/ubuntu/.hermes/profiles/erhua/plugins/qiwe-platform
  -> /home/ubuntu/qintopia-agent-os-releases/current/skills/qiwe
```

Do not replace a whole Hermes profile directory from CI.

## Rollback

Rollback should be symlink-based:

1. Confirm `previous` points to the intended rollback SHA.
2. Switch `current` back to `previous`.
3. Restart only the services or Hermes profile processes that were changed.
4. Run the same health checks used after promotion.
5. Record the rollback evidence in git.

Rollback must not rebuild on the server or copy individual source files with `scp`.

## Server Git Boundary

Server-side GitHub access is allowed only for deploy runner bootstrap, deploy runner
upgrades, or diagnostics. It is not part of the normal runtime release path.

If the deploy runner itself changes, treat that as a separate approved change before
artifact promotion. Record the deploy runner SHA separately from the runtime release
SHA.
