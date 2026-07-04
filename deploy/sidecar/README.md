# Deploy: Sidecar

`deploy/sidecar` is the rollout, smoke, and rollback contract for the Agent OS sidecar
service family.

## Current Status

`scripts/server-deploy.sh` is a legacy source snapshot adopted from
`qintopia-message-sidecar@eda2652`. It preserves the current standalone sidecar deploy
knowledge, systemd units, smokes, and rollback hints. It is not the monorepo-native
production deployment entrypoint.

M9-D cut over only the approved active sidecar service family to the monorepo checkout
and verified CI artifact. M9-F must still repoint the remaining active
`qintopia-agentos-*` workers and Hermes `mcp-context` command references away from
`/home/ubuntu/qintopia-msg-sidecar`.

The current M9 runtime shape is still a transition model. The next target is M10:
immutable release directories under `/home/ubuntu/qintopia-agent-os-releases/<sha>` with
stable `current` and `previous` symlinks.

## Current Source

- Local source: `../qintopia-message-sidecar/scripts/server-deploy.sh`
- Legacy runbook snapshot:
  `../qintopia-message-sidecar/docs/operations/server-deployment.md`
- Adopted reference: `eda2652f21999e4f32699463413372accbd3b76e`
- Server deployment path observed on 2026-07-03: `/home/ubuntu/qintopia-msg-sidecar`

## Deployment Rule

Server deployment must use git and an approved commit SHA. Do not edit files directly on
the server and do not use `scp` overwrites as a normal release path.

The global M9 execution contract is
`../../docs/operations/m9-server-cutover-runbook.md`. The target server filesystem model
is `../../docs/operations/server-directory-plan.md`. The sidecar-specific historical
cutover notes are in `docs/monorepo-cutover-plan.md`.

The monorepo-native systemd target shape is documented in
`docs/systemd-cutover-plan.md`. Render and validate the unit review files without
touching the server:

```bash
pnpm deploy:systemd:check
```

M9-F legacy-reference removal is documented in `docs/m9f-legacy-reference-removal.md`.
Validate its repository-side readiness checks without touching the server:

```bash
pnpm deploy:m9f:check
```

To produce review files under `dist/` for a candidate SHA:

```bash
QINTOPIA_M9_TARGET_SHA="<approved-target-sha>" \
deploy/sidecar/scripts/render-systemd-units.sh
```

## Server Caveat

The server is intentionally mixed until M9-F and M10 complete. Three
`qintopia-message-*` services run from the approved monorepo artifact, while remaining
legacy workers and Hermes MCP context commands still reference the old standalone
checkout. Treat Huabaosi shadow/Rust material as review-pool until the owner explicitly
approves it as product direction.

## Validation

Before any cutover from this monorepo, the deploy package needs:

- exact target branch and commit SHA
- successful CI workflow run for the target SHA, with the `sidecar-artifact` artifact
  uploaded
- server-side manifest and `SHA256SUMS` verification of the downloaded artifact
- rendered systemd unit review output
- package tests and smokes
- service health checks
- rollback command and owner record
