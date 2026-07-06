# Production Deploy Runner

This document records the intended production deploy automation after the server moved
to `qintopia-agent-os-releases/current`.

## Current Server Evidence

Read-only verification on 2026-07-06 showed:

- production sidecar and worker services run from
  `/home/ubuntu/qintopia-agent-os-releases/current`;
- Hermes plugin symlinks for Erhua, Xiaoman, WenYuanGe, and Huabaosi point into release
  directories;
- `/home/ubuntu/qintopia-agent-os-releases/current` points to
  `16496c8d4bfb13ed26d080727a4c812f9c2e0487`;
- `/home/ubuntu/qintopia-agent-os-releases/previous` points to
  `99681909149fde4f16daa3af941a750d1f239860`;
- `/etc/qintopia/cos-artifacts.env` exists but was not readable by the `ubuntu` user;
- no deploy runner or timer existed yet.

The server has enough disk space for immutable release assembly. The release history
also contains manual assembly records where the directory name, `runtime_sha`, and
`deploy_bundle_sha` are not always the same. New automated releases must record those
fields separately and must not infer one from another.

## Target Flow

```text
GitHub Deploy Production workflow_dispatch
  -> production environment approval
  -> require workflow ref refs/heads/master
  -> validate target SHA belongs to origin/master
  -> generate a signed deploy request from the reviewed master workflow code
  -> upload request to fixed COS pending queue
  -> server systemd timer polls COS
  -> server validates request schema, HMAC signature, TTL, repository, environment, SHA, scope, and target
  -> server downloads sidecar and deploy-bundle artifacts from COS
  -> server verifies artifact-manifest.json and SHA256SUMS
  -> server assembles releases/<release-sha>
  -> server switches previous/current
  -> server restarts approved system services and Hermes user services
  -> server runs smoke
  -> server uploads deploy result JSON
  -> server archives the request and removes the pending COS object
```

GitHub Actions must not SSH to the server. The server must not pull repository source or
build Rust for routine releases.

## GitHub Controls

The workflow is `.github/workflows/deploy-production.yml` and supports only
`workflow_dispatch`.

Repository owners must configure the `production` environment with required reviewers.
The workflow should use production environment secrets for COS upload:

- `TENCENT_COS_SECRET_ID`
- `TENCENT_COS_SECRET_KEY`
- `DEPLOY_REQUEST_SIGNING_KEY`
- `DEPLOY_REQUEST_SIGNING_KEY_ID`

The workflow must run from `refs/heads/master`. It validates the requested commit is on
`origin/master`, then signs and uploads the deploy request from the reviewed workflow
code on `master`. It must not check out an older target commit and execute that older
copy of repository scripts with production secrets.

Repository variables may keep non-secret COS defaults:

- `TENCENT_COS_BUCKET`
- `TENCENT_COS_REGION`
- `TENCENT_COS_ENDPOINT`

The deploy request prefix is not configurable. It is fixed to `qintopia-agent-os` so the
GitHub workflow, JSON schema, server-side validator, and COS poller share one production
queue contract.

`DEPLOY_REQUEST_SIGNING_KEY` and `DEPLOY_REQUEST_SIGNING_KEY_ID` must also be present on
the production server, normally in `/etc/qintopia/cos-artifacts.env`. COS write
permission alone must not be enough to trigger deployment; the server rejects unsigned
requests, requests signed with a different key, and requests signed for a different key
id.

## Server Controls

The runner is root-owned because it needs to read `/etc/qintopia/cos-artifacts.env` and
restart system services. It must execute only the fixed scripts in `deploy/runner/`.

The runner must not:

- accept arbitrary shell commands from the request;
- trust COS request JSON without server-side validation;
- trust COS request JSON without HMAC signature verification;
- process expired requests;
- repeatedly process the same pending COS object after it has been archived;
- roll back before `current` has been switched;
- report rollback success when `rollback-release.sh` failed;
- deploy a SHA that was not requested explicitly;
- edit files under `.hermes` directly;
- run `git fetch`, `git checkout`, or local Rust builds for routine releases.

Hermes restart targets map to ubuntu user-level systemd services such as
`hermes-gateway-erhua.service`, not system-scope units. The smoke script must restart
and verify each requested Hermes target, or fail the deployment.

## Request And Result Records

Deploy requests live under:

```text
qintopia-agent-os/deploy-requests/production/pending/<request-id>.json
```

Deploy results live under:

```text
qintopia-agent-os/deploy-results/production/<request-id>.json
```

The request schema is `deploy/runner/deploy-request.schema.json`. The result schema is
`deploy/runner/deploy-result.schema.json`.

## First Server Installation

After this repository change is merged and the deploy bundle is published:

1. Download and verify the deploy bundle on the server through the existing COS path.
2. Assemble a new immutable release containing `deploy/runner/`.
3. Install `deploy/runner/qintopia-agent-os-deploy-runner.service` and
   `deploy/runner/qintopia-agent-os-deploy-runner.timer` as root-owned system units.
4. Run `systemctl daemon-reload`.
5. Run one dry-run request first.
6. Enable the timer only after the dry-run result is uploaded and inspected.

Do not enable production non-dry-run deployment until the dry-run proves request
polling, artifact download, manifest validation, result upload, and smoke behavior.

## Validation

```bash
pnpm deploy:runner:check
pnpm check:light
```
