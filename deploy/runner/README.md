# Production Deploy Runner

`deploy/runner` defines the stable production deployment control plane for Qintopia
Agent OS.

The runner exists so collaborators can deploy an approved `master` SHA without direct
server access. GitHub Actions creates an HMAC-signed, schema-validated deploy request in
COS. The server-side runner pulls that request, verifies the signature and artifacts,
promotes a release, and writes a deploy result.

## Direction

```text
GitHub Release published
  -> validate release tag is on origin/master
  -> build sidecar/deploy-bundle artifacts
  -> production environment approval
  -> upload sidecar/deploy-bundle artifacts to COS
  -> generate a signed request from the reviewed master workflow code
  -> upload deploy request JSON and current.json pointer to fixed COS prefix qintopia-agent-os
  -> server deploy runner reads current.json and the referenced request
  -> validate request schema, signature, TTL, repository, environment, SHA, scope, and restart target
  -> download sidecar and deploy-bundle artifacts from COS
  -> verify manifests and SHA256SUMS
  -> assemble /home/ubuntu/qintopia-agent-os-releases/<release-sha>
  -> switch previous/current symlinks
  -> restart approved system and Hermes user-service targets
  -> smoke
  -> write deploy result JSON to COS
  -> GitHub Actions optionally waits for the COS result JSON and fails the run on failed deploy
  -> archive local request state for idempotency
```

No GitHub Action should SSH to production. No routine release should run `git fetch`,
build Rust, copy source with `scp`, or edit `.hermes` live state.

`workflow_dispatch` remains available as an emergency or diagnostic path, but normal
operators should publish a GitHub Release instead of manually running Actions.
Publishing a non-prerelease GitHub Release is the production release entrypoint; the
workflow still uses the GitHub `production` environment approval gate before it can
write the signed deploy request.

GitHub Release assets are not part of the production deploy path. COS is the artifact
registry consumed by the server; the Release page is the operator-facing version record.

Release tags must point to the current `master` HEAD. To deploy through the normal path:
create or select a tag for the current `master`, draft the GitHub Release, then click
Publish release. Do not publish a Release for an older commit as a shortcut; use
rollback instead.

## Request Contract

Deploy requests must match `deploy-request.schema.json`.

Important fields:

- `commit_sha`: reviewed `master` commit requested by the operator.
- `runtime_sha`: sidecar runtime artifact SHA in COS.
- `deploy_bundle_sha`: deploy bundle artifact SHA in COS.
- `release_sha`: immutable release directory name. For normal releases this should match
  `deploy_bundle_sha` when only operator/plugin files changed, or the target commit SHA
  when runtime and deploy bundle were built together.
- `release_scope`: one or more of `sidecar-runtime`, `deploy-bundle`, and
  `hermes-plugins`.
- `restart_targets`: fixed restart groups. The runner must not accept arbitrary service
  names.
- `dry_run`: validate and assemble without switching `current` or restarting services.
- `signature`: HMAC-SHA256 signature over the unsigned request body. The GitHub
  `production` environment and the server must share `DEPLOY_REQUEST_SIGNING_KEY` and
  `DEPLOY_REQUEST_SIGNING_KEY_ID`.

The COS request prefix is intentionally fixed to `qintopia-agent-os`. Bucket, region,
and endpoint can vary by environment; the production queue path cannot.

COS write access alone is not sufficient to trigger deployment. The server rejects
unsigned requests and requests signed with the wrong key.

Rollback is attempted only after `current` has been switched and the post-promotion
smoke path fails. Artifact download, request validation, or staging failures must not
move a healthy `current` symlink back to `previous`.

Rollback result records must distinguish rollback success from rollback failure. A
failed rollback is recorded as deployment `failed` with `rollback.status: failed`, not
as `rolled_back`.

## Restart Target Resolution

Release deploys should not restart every Agent by default. GitHub resolves restart
targets from the final deployed Release diff:

```text
latest successful deployed Release tag..current Release tag
  -> deploy/restart-target-rules.yaml
  -> tools/deploy/resolve-restart-targets.mjs
  -> deploy request restart_targets
```

If no successful deployed Release can be identified from workflow history, the workflow
falls back to the previous published Release tag. This keeps first-run and
history-pruned cases deployable, while avoiding missed restarts after a published
Release deploy failed.

PR checks may show a restart impact preview, but the Release workflow must recompute the
target list from the final tags. PR output is advisory only.

The resolver must fail closed for production-adjacent files that are not covered by a
rule. This prevents a new Agent, skill, workflow, MCP adapter, runtime template, or
deploy script from shipping without an explicit restart decision.

`RELEASE_DEPLOY_RESTART_TARGETS_OVERRIDE` may replace the resolved list only for
operator emergencies. Overrides must still use the deploy request schema allowlist and
must be visible in the workflow summary.

New Agents must declare their deploy target in `agents/<agent>/agent.yaml`, then add the
matching deploy schema, smoke script, and restart-rule entries in the same PR. A new
profile package without a restart target is incomplete.

## Server Requirements

The target server currently has:

- `/home/ubuntu/qintopia-agent-os-releases/current`
- `/home/ubuntu/qintopia-agent-os-releases/previous`
- systemd services running from `release/current`
- Hermes plugin symlinks pointing into release directories
- `/etc/qintopia/cos-artifacts.env`

The COS env file was observed as `root:ubuntu 0600`, so the production runner should run
as a root-owned system service and execute only the fixed runner scripts. If a dedicated
runner user is introduced later, change the env file to a dedicated group-readable mode
and document that separately.

Required server environment:

- `TENCENT_COS_BUCKET`
- `TENCENT_COS_REGION`
- `DEPLOY_REQUEST_SIGNING_KEY`
- `DEPLOY_REQUEST_SIGNING_KEY_ID`
- `TENCENT_COS_SECRET_ID` and `TENCENT_COS_SECRET_KEY`, or CVM role settings

## Manifest Normalization

The current production release history contains manual assembly records where the
release directory name, `runtime_sha`, and `deploy_bundle_sha` may differ. New automated
releases must write a normalized `manifest.json` with separate fields:

- `release_sha`
- `runtime_sha`
- `deploy_bundle_sha`
- `previous_sha`
- `assembled_at`
- `request_id`
- `release_scope`
- `restart_targets`

The runner must not infer one SHA from another.

## Server Units

Install `qintopia-agent-os-deploy-runner.service` and
`qintopia-agent-os-deploy-runner.timer` only after a reviewed deploy bundle has been
published and verified on the server. The timer runs
`deploy/runner/poll-deploy-requests.sh`, which pulls `current.json` from COS, downloads
the referenced request if it has not already been consumed locally, and then invokes
`qintopia-agent-os-deploy-runner`.

Missing `current.json`, pointers with an existing COS result, and locally consumed
pointers are normal idle timer states and must exit successfully. COS network,
authentication, or permission failures remain hard failures.

Do not point the timer at a writable server checkout.

## GitHub Result Visibility

`deploy/runner/wait-deploy-result.sh` lets the production workflow wait for the
server-written result in COS after the request is uploaded. Enable it only after the
server timer is installed and active by setting repository or environment variable
`WAIT_FOR_SERVER_DEPLOY_RESULT=true`.

Recommended production values:

- `WAIT_FOR_SERVER_DEPLOY_RESULT=true`
- `DEPLOY_RESULT_TIMEOUT_SECONDS=900`
- `DEPLOY_RESULT_POLL_SECONDS=15`

Until the timer is active, keep `WAIT_FOR_SERVER_DEPLOY_RESULT=false`; otherwise the
workflow will correctly time out because no server process is consuming deploy requests.

## Validation

```bash
pnpm deploy:runner:check
pnpm check:light
```
