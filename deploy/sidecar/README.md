# Deploy: Sidecar

`deploy/sidecar` is the rollout, smoke, and rollback contract for the Agent OS sidecar
service family.

## Current Status

`scripts/server-deploy.sh` is a legacy source snapshot adopted from
`qintopia-message-sidecar@eda2652`. It preserves the current standalone sidecar deploy
knowledge, systemd units, smokes, and rollback hints. It is not yet the monorepo-native
production deployment entrypoint.

Use the script for reference and local smoke compatibility only until M9 server cutover
approves a monorepo deployment path.

## Current Source

- Local source: `../qintopia-message-sidecar/scripts/server-deploy.sh`
- Runbook: `../qintopia-message-sidecar/docs/operations/server-deployment.md`
- Adopted reference: `eda2652f21999e4f32699463413372accbd3b76e`
- Server deployment path observed on 2026-07-03: `/home/ubuntu/qintopia-msg-sidecar`

## Deployment Rule

Server deployment must use git and an approved commit SHA. Do not edit files directly on
the server and do not use `scp` overwrites as a normal release path.

The sidecar-specific cutover plan is `docs/monorepo-cutover-plan.md`. The global M9
execution contract is `../../docs/operations/m9-server-cutover-runbook.md`.

## Current Server Caveat

The server checkout observed on 2026-07-03 is
`codex/huabaosi-localization-shadow@b16c247a19ec751c08de75ae2d312f35b765f317`, not local
`main@eda2652f21999e4f32699463413372accbd3b76e`. Treat that branch as review-pool until
the owner confirms whether Huabaosi shadow work belongs in the roadmap.

## Validation

Before any cutover from this monorepo, the deploy package needs:

- exact target branch and commit SHA
- successful CI workflow run for the target SHA, with the `sidecar-artifact` artifact
  uploaded
- server-side manifest and `SHA256SUMS` verification of the downloaded artifact
- dry-run or prepare output
- package tests and smokes
- service health checks
- rollback command and owner record
