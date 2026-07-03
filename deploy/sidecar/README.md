# Deploy: Sidecar

`deploy/sidecar` is the rollout, smoke, and rollback contract for the Agent OS sidecar
service family.

## Current Source

- Local source: `../qintopia-message-sidecar/scripts/server-deploy.sh`
- Runbook: `../qintopia-message-sidecar/docs/operations/server-deployment.md`
- Adopted reference: `eda2652f21999e4f32699463413372accbd3b76e`
- Server deployment path observed on 2026-07-03: `/home/ubuntu/qintopia-msg-sidecar`

## Deployment Rule

Server deployment must use git and an approved commit SHA. Do not edit files directly on
the server and do not use `scp` overwrites as a normal release path.

## Current Server Caveat

The server checkout observed on 2026-07-03 is
`codex/huabaosi-localization-shadow@b16c247a19ec751c08de75ae2d312f35b765f317`, not local
`main@eda2652f21999e4f32699463413372accbd3b76e`. Treat that branch as review-pool until
the owner confirms whether Huabaosi shadow work belongs in the roadmap.

## Validation

Before any cutover from this monorepo, the deploy package needs:

- exact target branch and commit SHA
- dry-run or prepare output
- package tests and smokes
- service health checks
- rollback command and owner record
