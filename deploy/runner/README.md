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
  -> render and install reviewed systemd units from the immutable release
  -> enable internal AgentOS worker timers
  -> restart approved system and Hermes user-service targets
  -> smoke
  -> write deploy result JSON to COS
  -> GitHub Actions optionally waits for the COS result JSON and fails the run on failed deploy
  -> archive local request state for idempotency
```

No GitHub Action should SSH to production. No routine release should run `git fetch`,
build Rust, copy source with `scp`, or edit `.hermes` live state.

After promotion, the root-owned runner renders systemd units from the release-local
bundle, installs a fixed allowlist under `/etc/systemd/system`, and enables only
internal AgentOS worker timers. Those timers may write AgentOS/Postgres state, but they
do not enable Feishu writeback, QiWe sends, or external adapters.

The root runner extracts both COS archives with `tar --no-same-owner`. Build artifacts
may contain the GitHub runner's numeric UID and GID; preserving those identities would
make the immutable release tree owned by an unrelated server account and invalidate
release-local ownership checks. The later `cp -a` assembly step may preserve modes and
the already normalized root ownership only.

The deploy artifact may carry the observation-only Xiaoman profile bundle and parity
smoke. The runner does not render it, read its server-local values, create profile
symlinks, or restart Xiaoman on the bundle's behalf. Activation requires a later
reviewed runner change with first-cutover rollback evidence.

The bundled one-time values migration command also remains manual. The runner must not
invoke it or create `/etc/qintopia/xiaoman-profile-bundle-values.json` during promotion.

A Release containing only observation bundle inputs still follows the workflow's minimum
internal system-service restart because a deploy artifact is promoted. The observation
bundle paths themselves are no-restart paths for the Xiaoman gateway.

`workflow_dispatch` remains available as an emergency or diagnostic path, but normal
operators should publish a GitHub Release instead of manually running deploy Actions.

The explicit `legacy_runner_bootstrap` mode exists only when the currently deployed
runner rejects a newly reviewed Huabaosi feature contract before it can install the new
deploy bundle. It binds the runtime and commit to the latest trusted successful deploy
result, requires a distinct immutable release SHA, accepts only
`release_scope=deploy-bundle` and `restart_targets=qintopia-system-services`, and keeps
rollback enabled. The normal artifact path continues to reject the legacy Huabaosi
feature set. Run bootstrap as a dry-run first; a successful live bootstrap installs the
reviewed runner while retaining the currently deployed runtime, after which the target
Release must pass its own dry-run and full deployment. Publishing a non-prerelease
GitHub Release is the production release entrypoint; the workflow still uses the GitHub
`production` environment approval gate before it can write the signed deploy request.

For `workflow_dispatch`, both reviewed production sidecar artifacts must already exist
in COS before request upload. The primary profile remains `huabaosi-production`; the
workflow fetches Huabaosi, the QiWe companion, and the deploy bundle before it writes a
request, so a missing or mismatched companion fails in GitHub Actions rather than only
on the server runner.

Rollback uses the separate `Rollback Production` workflow. It exposes a selectable
published Release tag list for operators, resolves the chosen tag to a commit SHA, and
then submits the same signed deploy request contract. The operator also selects the
reviewed rollback `runtime_artifact_profile`, which must match the validated COS sidecar
artifact before the request is written. GitHub Actions `choice` inputs are static YAML
options, so adding a new rollback candidate requires updating the workflow option list
in git.

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
- `runtime_artifact_profile`: primary production sidecar artifact profile, fixed to
  `huabaosi-production`. The release manifest separately records
  `companion_runtime_artifact_profiles=["qiwe-production"]`.
- `deploy_bundle_sha`: deploy bundle artifact SHA in COS.
- `release_sha`: immutable release directory name. For normal releases this should match
  `deploy_bundle_sha` when only operator/plugin files changed, or the target commit SHA
  when runtime and deploy bundle were built together.
- `release_scope`: one or more of `sidecar-runtime`, `deploy-bundle`, and
  `hermes-plugins`. The fixed `hermes-profile-erhua` scope is exclusive and requires
  exactly the `hermes-erhua` restart target. Production action scopes such as
  `production-hermes-cron-apply`, `production-activation`, `production-observation`,
  `production-legacy-cron-retirement`, and `production-runtime-one-shot` are also
  exclusive and require exactly the `qintopia-system-services` restart target.
- `production-hermes-cron-apply`: exclusive scope for owner-approved repository-to-live
  Hermes cron writes. It accepts only fixed reviewed recurring-task targets and
  `mode=install|enable`, calls release-local apply scripts with fixed approval strings,
  and records sanitized target/mode/status evidence only.
- `production-runtime-one-shot`: exclusive scope for owner-approved immediate production
  runs after the matching timer is already enabled. It accepts exactly one fixed target,
  either `erhua-morning-brief` or `xiaoman-daily-case-report-auto-publish-backfill`,
  requires target-specific approval metadata, records sanitized evidence only, and does
  not promote releases, retire cron files, write persistent config, or enable/disable
  timers.
- `restart_targets`: fixed restart groups. The runner must not accept arbitrary service
  names.
- `dry_run`: validate and assemble without switching `current` or restarting services.
- `profile_dry_run_request_id`: required only for non-dry-run `hermes-profile-erhua`;
  names the reviewed dry run and must still be fresh.
- `rollback_on_smoke_failure`: must be `true` for `hermes-profile-erhua`.
- `signature`: HMAC-SHA256 signature over the unsigned request body. The GitHub
  `production` environment and the server must share `DEPLOY_REQUEST_SIGNING_KEY` and
  `DEPLOY_REQUEST_SIGNING_KEY_ID`.

The COS request prefix is intentionally fixed to `qintopia-agent-os`. Bucket, region,
and endpoint can vary by environment; the production queue path cannot.

COS write access alone is not sufficient to trigger deployment. The server rejects
unsigned requests and requests signed with the wrong key.

For ordinary releases, rollback is attempted only after `current` has been switched and
the post-promotion smoke path fails. Artifact download, request validation, or staging
failures must not move a healthy `current` symlink back to `previous`. Erhua profile
requests never switch release links; their rollback restores only the backed-up profile
files before restarting Erhua.

Rollback result records must distinguish rollback success from rollback failure. A
failed rollback is recorded as deployment `failed` with `rollback.status: failed`, not
as `rolled_back`.

When promotion has already started and a later step fails, the result must keep a
bounded diagnostic in the `deploy-runner` check detail: the fixed failure stage, exit
status, whether `current` had been promoted, and whether an Erhua profile activation was
attempted. Do not upload raw server logs, environment files, journal output, secrets, or
external adapter payloads as deploy result diagnostics.

Erhua profile activation additionally requires a matching reviewed dry-run, backs up and
restores the runtime-local config and `.env`, verifies hashes, modes, and ownership,
restarts only Erhua, requires affirmative resolution through Hermes's own provider
resolver, and records non-sending provider smoke evidence. It validates against the
active release and never moves `current` or `previous`. The initial rollout is
deliberately two-stage because the old runner must first be upgraded through an existing
scope. See the Erhua Livecool profile overlay runbook under
`docs/operations/profile-bundles/`.

## Restart Target Resolution

Release deploys should not restart every Agent by default. GitHub resolves restart
targets from the final deployed Release diff:

```text
each target's latest server deploy result status succeeded..current Release tag
  -> deploy/restart-target-rules.yaml
  -> tools/deploy/resolve-restart-targets.mjs
  -> deploy request restart_targets
```

The resolver must use the server deploy result emitted by `wait-deploy-result.sh`; a
successful GitHub workflow alone may be a dry-run, and a successful deploy may restart
only some targets. If no target-specific successful deployed Release can be identified
from workflow logs, the workflow falls back to the previous published Release tag for
that target. This keeps first-run and history-pruned cases deployable, while avoiding
missed restarts after a published Release deploy failed, only dry-ran, or skipped a
target.

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
- root `python3` with PyYAML
- runner `ReadWritePaths` access to `/home/ubuntu/.hermes/profiles/erhua`
- runner `ReadWritePaths` access to the fixed
  `/home/ubuntu/.hermes/profiles/erhua/scripts` directory for reviewed profile-local
  Erhua Hermes wrapper installs only
- runner `ReadWritePaths` access to the fixed
  `/home/ubuntu/.hermes/profiles/xiaoman/cron` directory for reviewed Xiaoman legacy
  cron retirement and Hermes cron apply only
- runner `ReadWritePaths` access to the fixed
  `/home/ubuntu/.hermes/profiles/xiaoman/scripts` directory for reviewed profile-local
  Xiaoman Hermes wrapper installs only
- runner `ReadWritePaths` access to `/home/ubuntu/.hermes/scripts` only for reviewed
  legacy/global Hermes helper compatibility; no-agent cron wrappers install into the
  profile-local `scripts` directory
- runner `ReadWritePaths` access to `/home/ubuntu/.config/systemd/user` for the reviewed
  Hermes cron snapshot user timer install only
- runner `ReadWritePaths` access to the fixed
  `/home/ubuntu/.local/state/qintopia-agentos/hermes-cron-snapshot` repo for reviewed
  Hermes cron apply snapshot sync only

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
- `companion_runtime_artifact_profiles`

The runner must not infer one SHA from another.

### Same-SHA Follow-up Requests

A follow-up deployment for an existing immutable `release_sha` must reuse the existing
manifest's exact `runtime_sha`, `deploy_bundle_sha`, `commit_sha`, `release_scope`, and
`restart_targets`. The primary runtime profile remains `huabaosi-production`. A legacy
Huabaosi-only release may add the complete missing `sidecar-profiles/qiwe-production`
tree after exact inventory verification; the primary sidecar payload cannot be replaced.
Any partial companion or other identity mismatch fails before promotion.

Before dispatching a same-SHA follow-up, read only the sanitized manifest fields from
the promoted release evidence or the prior successful deploy result. Do not guess the
restart targets from the current diff or from a later runbook summary. A mismatch fails
before promotion, does not switch `current`, and does not require rollback. Successful
deploy results retain the same reviewed `commit_sha`, `runtime_sha`,
`deploy_bundle_sha`, `release_scope`, and `restart_targets` identity, plus the active
runtime profile, so this comparison can be made without re-reading mutable operator
notes.

The `v0.2.29` runner wrote release manifests without `runtime_artifact_profile`. A
same-SHA follow-up assembled by `v0.2.30+` must adopt the missing reviewed profile from
the immutable `sidecar/artifact-manifest.json`, persist it into the existing release
manifest, and only then compare the identity. This compatibility path and companion
installation both fail closed for any other manifest drift or unavailable artifact.

The existing-release path also repairs metadata left by a previous runner only after the
exact manifest identity matches, the complete release tree matches freshly fetched and
verified artifacts, and all three packaged `SHA256SUMS` files pass. It then makes the
release tree root-owned and copies only modes from the fresh staging tree. Any missing,
extra, changed, symlink-drifted, or unsupported path fails before metadata mutation.

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

When the server poller rejects a malformed request before the runner can write a normal
result, it uploads a bounded fallback `status=failed` result with
`checks=[{"name":"deploy-request-validation","status":"failed"}]`. That fallback may
normalize invalid SHA/profile/scope/target fields; `wait-deploy-result.sh` must accept
only that exact validation-failure shape while still rejecting ordinary result identity
mismatches.

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
