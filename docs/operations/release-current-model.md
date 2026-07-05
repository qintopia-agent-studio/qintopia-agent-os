# Release/Current Model

This document defines the target release model after M9-F removes the remaining
references to `/home/ubuntu/qintopia-msg-sidecar`.

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

## Target Directory Shape

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

`/home/ubuntu/qintopia-agent-os-artifacts/<sha>` is only a transition download cache. It
can be kept for rollback evidence, but services should eventually point at
`/home/ubuntu/qintopia-agent-os-releases/current`.

## Release Payloads

The first M9-F release assembles two verified COS inputs: the sidecar runtime artifact
and the deploy bundle. The resulting release directory should contain:

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

1. Confirm the target SHA has successful `check` and `sidecar-artifact` CI jobs.
2. Download the target SHA artifact from COS into a staging or cache directory.
3. Verify `artifact-manifest.json`, `SHA256SUMS`, and binary self-checks.
4. Assemble `/home/ubuntu/qintopia-agent-os-releases/<approved-sha>`.
5. Validate the release directory without changing `current`.
6. Record the current symlink target as the rollback SHA.
7. Update `previous` to the old `current` target.
8. Atomically switch `current` to `<approved-sha>`.
9. Restart only owner-approved services or Hermes profile processes.
10. Record the release SHA, previous SHA, checks, and rollback command in git.

The release assembly step should be idempotent: if the release directory already exists,
the operator must verify its manifest and checksum instead of overwriting it blindly.

## Runtime References

Systemd services should use:

```text
WorkingDirectory=/home/ubuntu/qintopia-agent-os-releases/current
Environment=QINTOPIA_SIDECAR_MIGRATIONS_DIR=/home/ubuntu/qintopia-agent-os-releases/current/runtime/postgres/migrations
Environment=QINTOPIA_DEPLOYED_COMMIT_SHA=<approved-sha>
ExecStart=/home/ubuntu/qintopia-agent-os-releases/current/sidecar/qintopia-message-sidecar <subcommand>
```

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
