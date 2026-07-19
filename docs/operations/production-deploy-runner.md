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

## v0.2.2 History Correction (2026-07-12)

- Owner approval: approved by owner in Codex task `019f4b0b-1f6a-7260-9e88-8da61ca605ea`
  on `2026-07-12`, to preserve release-history continuity.
- Source correction: restore original published Release tag `v0.2.2` to
  `d083e5ccfce2d07048e07c0ceb8c052671f65911` (historical continuity correction).
- Evidence: maintain `run` `28919370259` as successful `Deploy Production` evidence in
  record.
- Boundary exception: this is a narrow temporary exception to the manifest rule
  (`manifest tracks existing published Releases`) and keeps
  `.release-please-manifest.json` at `v0.2.2` so `v0.2.3` can be generated in the next
  release cycle.
- Post-conditions: do not rebuild deleted legacy GitHub Releases, do not reuse version
  numbers, and end this exception after publishing `v0.2.3`.
- Rollback policy correction: before `v0.2.3` publishes, rollback `release_tag` default
  in the workflow must be `v0.2.1`, and the workflow/checker must explicitly reject
  `v0.2.2`.

## v0.2.3 Rollback Candidate Audit (2026-07-12)

- Release state: `v0.2.3` is published, and its tag, `master`, and GitHub Release target
  all resolve to `1b988be2744aa148200ede8cca9de468a42807fa`.
- Deployment evidence: `Deploy Production` run `29184865975` succeeded.
- Evidence basis also includes verified release metadata and current COS inventory
  checks.
- Verified release candidates/evidence set currently records:
  - `v0.2.3` (`1b988be2744aa148200ede8cca9de468a42807fa`) is the published current
    release, with successful deployment evidence.
  - `v0.2.2` (`d083e5c`) has evidence records and local visibility context but does not
    satisfy current GitHub rollback selection criteria.
  - `v0.2.1` deploy run `28918954440` is failed deploy evidence.
  - `v0.2.0` (`b24c3f7`) has published Release and paired COS artifacts for workflow
    rollback path (`sidecar-runtime` + `deploy-bundle`).
- Audit result after `v0.2.3`: GitHub rollback path currently accepts only `v0.2.0`
  candidates that are published, non-prerelease, and have both required COS asset types.
- Documentation boundary: these checks are current-evidence-based; future additions
  require manifest/release proof plus evidence replay before widening workflow options.

The server has enough disk space for immutable release assembly. The release history
also contains manual assembly records where the directory name, `runtime_sha`, and
`deploy_bundle_sha` are not always the same. New automated releases must record those
fields separately and must not infer one from another.

## Target Flow

```text
Release Please release PR merged
  -> Release Please updates CHANGELOG.md and the release manifest
  -> Release Please creates a draft GitHub Release
Owner manually publishes the draft GitHub Release
  -> validate release tag resolves to a commit on origin/master
  -> build sidecar and deploy-bundle artifacts
  -> production environment approval
  -> upload sidecar and deploy-bundle artifacts to COS
  -> generate a signed deploy request from the reviewed master workflow code
  -> upload request JSON and a fixed current.json pointer to COS
  -> server systemd timer fetches current.json and the referenced request
  -> server validates request schema, HMAC signature, TTL, repository, environment, SHA, scope, and target
  -> server downloads sidecar and deploy-bundle artifacts from COS
  -> server verifies artifact-manifest.json and SHA256SUMS
  -> server assembles releases/<release-sha>
  -> server switches previous/current
  -> server restarts approved system services and Hermes user services
  -> server runs smoke
  -> server uploads deploy result JSON
  -> server archives the local request state
```

The fetched `artifact-manifest.json`, `SHA256SUMS`, and packaged archives are immutable
non-secret release metadata and must be installed mode `0444`. The sidecar binary must
remain mode `0755`. This lets the unprivileged release-local observation and preflight
paths verify the exact production feature set without running as root or weakening the
immutable release boundary.

Promotion must validate a newly assembled tree before it can become current. Every entry
must be owned by the effective deploy-runner UID, non-symlink entries must not be group-
or world-writable, the sidecar binary must be `0755`, and packaged manifests, checksum
files, and archives must be `0444`. Directories must remain group/world readable and
traversable for unprivileged release-local observation, and special file types are
forbidden. Release and staging roots are created explicitly as `0755` so this contract
is independent of ambient `umask`.

An existing same-SHA release may repair owner and modes only after exact manifest
identity, complete content/path/type/symlink equality with the freshly verified tree,
and both packaged checksum files pass. The repaired tree must then pass the same strict
validation as a new tree. Content or path drift fails before metadata mutation, and an
idempotent request must not replace a distinct `previous` target with `current`.

GitHub Actions must not SSH to the server. The server must not pull repository source or
build Rust for routine releases.

## GitHub Controls

Release preparation is handled by `.github/workflows/release-please.yml`. Release Please
opens or updates a release PR from merged Conventional Commits, updates `CHANGELOG.md`
and `.release-please-manifest.json`, and creates a draft GitHub Release after the
release PR is merged. Draft releases do not trigger production deployment. Because Agent
OS release mechanics are production-adjacent operator behavior, Release Please includes
`ci:` and `build:` commits in release notes. A deployment workflow or COS artifact
change must not disappear from the release PR just because it does not change end-user
application code.

The production workflow is `.github/workflows/deploy-production.yml`. Its primary
trigger is `release.published`: manually publishing a normal GitHub Release is the
production release entrypoint. The same workflow keeps `workflow_dispatch` only as an
emergency or diagnostic path for explicitly named SHAs.

Merging the Release Please PR prepares a version but does not approve production
deployment. Publishing the draft GitHub Release is the owner-approved production
approval event for this repository. The `production` environment scopes COS and
request-signing secrets to the deploy job, but Qintopia does not currently require a
second GitHub environment review gate after Release publication. If required reviewers
are added later, treat that as an extra gate on top of the Release approval, not as a
replacement for Release-based version control. The workflow should use production
environment secrets for COS upload and request signing:

- `TENCENT_COS_SECRET_ID`
- `TENCENT_COS_SECRET_KEY`
- `DEPLOY_REQUEST_SIGNING_KEY`
- `DEPLOY_REQUEST_SIGNING_KEY_ID`

Release merge and publication are manual-only operator actions. Do not enable or use
auto-merge for Release Please PRs, and do not automatically publish draft Releases from
automation or programming-agent flows. Automation may prepare the PR, draft Release, and
validation evidence; the owner still performs the merge and publish decisions manually.

For Release-triggered production deployment, the Release tag must point to the current
`origin/master` HEAD. Pre-releases are rejected. The workflow checks out the reviewed
`master` workflow code, builds artifacts for the release commit, uploads server-consumed
artifacts to COS, then signs and uploads a deploy request. It must not check out an
older target commit and execute that older copy of repository scripts with production
secrets.

Before publishing a draft Release, compare its tag target with current `origin/master`.
If `master` has advanced since the draft was prepared, do not publish or retry that
stale tag; let Release Please create the next release PR and publish the new current
HEAD tag after validation.

GitHub Release assets are intentionally not uploaded by this production workflow. COS is
the server-consumed artifact registry, and the GitHub Release page is only the operator
version record. This keeps GitHub attachment failures from turning a successful COS
upload and signed deploy request into a false production deploy failure.

Manual `workflow_dispatch` remains allowed only from `refs/heads/master`; it validates
the requested commit belongs to `origin/master`. Operators should prefer publishing a
GitHub Release over using manual workflow inputs.

Repository variables may keep non-secret COS defaults:

- `TENCENT_COS_BUCKET`
- `TENCENT_COS_REGION`
- `TENCENT_COS_ENDPOINT`
- `RELEASE_DEPLOY_SCOPE`
- `RELEASE_DEPLOY_DRY_RUN`
- `RELEASE_DEPLOY_RESTART_TARGETS_OVERRIDE`

`RELEASE_DEPLOY_DRY_RUN` controls what happens after a Release is published. Keep it
`true` until the deploy runner is installed and the first dry-run result is inspected.
After that, setting it to `false` makes publishing a normal GitHub Release generate a
real production deploy request.

Release-triggered deploys derive `restart_targets` from each restart target's latest
server deploy result status of `succeeded`, not from the global `current` symlink alone.
A successful GitHub workflow is not sufficient because it may represent a dry-run, and a
successful deploy may have restarted only a subset of targets. If workflow logs cannot
prove a target-specific deployed baseline, the workflow falls back to the previous
published Release tag for that target. The mapping lives in
`deploy/restart-target-rules.yaml` and is evaluated by
`tools/deploy/collect-release-deploy-results.mjs`,
`tools/deploy/resolve-release-restart-targets.mjs`, and
`tools/deploy/resolve-restart-targets.mjs`. PRs only show a restart impact preview; the
production request is resolved again from the final Release deploy evidence.

`RELEASE_DEPLOY_RESTART_TARGETS_OVERRIDE` is only for emergency operator override. When
set, it must contain deploy-runner allowlist targets such as `hermes-erhua` or
`qintopia-system-services`; the workflow records the override in the job summary. Do not
use the override as normal release configuration.

## Systemd Unit Bootstrap

After each promotion, the release runner renders the reviewed systemd unit allowlist
from the immutable release, installs it, reloads systemd, and enables only AgentOS
internal workflow timers. This keeps `QINTOPIA_DEPLOYED_COMMIT_SHA`, `WorkingDirectory`,
and sidecar binary paths aligned with `current`.

The first release that introduces this runner behavior needs one follow-up approved
`workflow_dispatch` request for the same published SHA after the release has become
`current`: that first promotion is still processed by the prior runner, while the second
request is processed by the new runner and installs the units. Do not bootstrap this by
editing `/etc/systemd/system` or release files on the server.

Unknown production-adjacent paths fail closed. If a PR adds a new Agent, skill,
workflow, runtime, MCP adapter, or deploy path without a restart rule, CI must fail
until the package contract and restart target rule are added.

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
- repeatedly process the same current pointer after it has been archived locally;
- roll back before `current` has been switched;
- report rollback success when `rollback-release.sh` failed;
- deploy a SHA that was not requested explicitly;
- edit files under `.hermes` directly;
- run `git fetch`, `git checkout`, or local Rust builds for routine releases.

Hermes restart targets map to ubuntu user-level systemd services such as
`hermes-gateway-erhua.service`, not system-scope units. The smoke script must restart
and verify each requested Hermes target, or fail the deployment.

Each Agent package must declare its runtime target in `agents/<agent>/agent.yaml`:

```yaml
runtime:
  restart_target: hermes-erhua
  systemd_user_service: hermes-gateway-erhua.service
```

Adding a new Agent requires adding the Agent package, the runtime target declaration,
the deploy request schema allowlist entry, the smoke restart case, the restart rule, and
contract tests in the same PR. A profile directory without a deployable restart contract
is not production-ready.

## Request And Result Records

Deploy requests live under:

```text
qintopia-agent-os/deploy-requests/production/requests/<request-id>.json
```

The latest deploy request pointer lives under:

```text
qintopia-agent-os/deploy-requests/production/current.json
```

Deploy results live under:

```text
qintopia-agent-os/deploy-results/production/<request-id>.json
```

The request schema is `deploy/runner/deploy-request.schema.json`. The result schema is
`deploy/runner/deploy-result.schema.json`.

The server runner intentionally does not list `deploy-requests/production/pending/`.
Tencent COS is object storage, not a queue, and ListBucket/prefix listing can be slower
or less reliable than reading a known object. GitHub writes `current.json`; the server
only needs `GetObject` for that fixed pointer and the referenced request. For Tencent
Cloud CVM/Lighthouse instances in the same region as the bucket, prefer the default
regional COS endpoint such as
`qintopia-agent-os-artifacts-1305166808.cos.ap-shanghai.myqcloud.com` rather than the
global acceleration endpoint. Same-region default COS access is expected to use Tencent
Cloud internal networking when DNS resolves to internal addresses.

The poller is idempotent for systemd timer health:

- if `current.json` does not exist yet, the poller exits successfully as idle;
- if `current.json` points to a request whose result already exists in COS, the poller
  exits successfully as idle even if local state was cleaned or migrated;
- if `current.json` still points to a locally processed request, the poller exits
  successfully as idle;
- if `current.json` still points to a locally failed request, the poller exits
  successfully as idle until GitHub uploads a new pointer.

Network, authentication, or permission failures while downloading the pointer still
return non-zero so COS outages do not look like normal idle time.

## First Server Installation

After this repository change is merged and a GitHub Release has published a deploy
bundle:

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
