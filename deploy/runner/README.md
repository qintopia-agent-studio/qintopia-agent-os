# Production Deploy Runner

## Hermes Core Readiness

`check-hermes-core-readiness.sh` is a read-only server-side preflight for the Hermes
core checkout. It verifies the official upstream remote, a clean `main` worktree,
minimum root-disk headroom, the Hermes interpreter, and the seven gateway services
without printing configuration, secrets, message content, or worktree paths.

Run it from a reviewed release on the server before the one-time Hermes core detachment
and before later direct core updates:

```bash
deploy/runner/check-hermes-core-readiness.sh
```

It must report `hermes_core_readiness=ready` before the steady-state update sequence:

```bash
cd /home/ubuntu/.hermes/hermes-agent
hermes update --check
hermes update --plan
hermes update --yes
```

Do not pass `--backup` by default. Current Hermes releases create a quick state snapshot
for every profile automatically; `--backup` additionally compresses the full
`HERMES_HOME` and requires separately reviewed disk or external backup capacity. A
successful command must also be followed by update-receipt, plugin-compatibility, and
seven-profile smoke validation as specified in
`docs/plans/active/hermes-core-detachment.md`.

The preflight intentionally fails while the current server checkout contains Qintopia
core patches or insufficient disk space. It is a gate, not a cleanup command. Core
detachment and the first clean-core cutover remain a reviewed maintenance deployment;
subsequent Qintopia changes continue through `release/current`.

## Hermes WeCom Configuration Readiness

`check-hermes-wecom-readiness.sh` is the read-only companion gate for preserving the
current seven-profile WeCom configuration contract. With no arguments it checks the
fixed production Hermes home. For an isolated migrated configuration, pass an explicit
absolute staging home:

```bash
deploy/runner/check-hermes-wecom-readiness.sh
deploy/runner/check-hermes-wecom-readiness.sh --staging-home /absolute/staging/hermes
```

The checker requires the reviewed enabled state for `default`, `guanerye`, `huabaosi`,
`silaoshi`, and `xiaoman`, and the reviewed disabled state for `erhua` and `wenyuange`.
For enabled profiles it verifies that the WeCom bot and secret bindings exist either in
the parsed channel configuration or as keys in the profile `.env`. It never sources the
environment file and never prints values, paths, targets, or message content. Symlinked,
oversized, malformed, ambiguous, or duplicate-key inputs fail closed.

This preflight proves configuration structure, reviewed enablement, and required-key
presence only. It does not prove network connectivity, actual WeCom delivery, complete
field-level parity, or equivalence of the old local patch. Those remain separate staging
replay and canary gates in `docs/plans/active/hermes-core-detachment.md`.

`check-hermes-wecom-parity.sh` is the next gate after the target Hermes version has
migrated a copied profile home under the fixed staging root. It compares each complete
WeCom configuration mapping and every `WECOM_*` environment binding against the current
production baseline. Moving the mapping between `channel.wecom` and `platforms.wecom` is
accepted; any key, value, target binding, retry setting, media option, or enabled state
drift fails closed.

```bash
deploy/runner/check-hermes-wecom-parity.sh \
  --candidate-home /home/ubuntu/.local/state/qintopia-agentos/hermes-core-staging/<release>/home
```

The comparison reads values only in local process memory and emits profile names,
booleans, match status, and fixed error codes. It never prints config values, env
values, paths, ids, or messages. This proves exact retained configuration parity; it
does not prove that the target WeCom plugin interprets every retained field identically,
so replay and canary remain mandatory.

## Hermes Core Artifact Verification

`verify-hermes-core-artifact.mjs` is the HC-1 fail-closed verifier for a staged clean
Hermes core artifact. It requires the official repository plus exact tag, commit, source
archive digest, derived artifact identity, and manifest digest. It validates the
manifest, build receipt, upstream-update receipt, and sanitized validation summary, then
checks the complete `core/` inventory and `SHA256SUMS`.

The artifact tree must be immutable and owned consistently by the release owner. Under
the HC-2 production model that abstract manifest owner maps to `root:root`; gateway
service users receive read-only access and cannot stage or replace releases. Symlinks,
hardlinks, special files, missing or extra files, unsafe paths, unexpected modes,
oversized inputs, failed receipts, timestamp drift, and any cross-document identity
mismatch are rejected. Failure output contains only a fixed error code; it does not
print artifact paths or document contents.

HC-1 does not authorize production promotion. Until HC-4 binds the expected digests to
the signed `hermes-core-release` request, caller-supplied expected values are not an
authenticity boundary. HC-2 and HC-3 must also complete immutable staging, lineage,
transactional service switching, and rollback before this verifier can enter the live
runner path.

## Hermes Core Release Dry-Run

`plan-hermes-core-release.sh` is the HC-2 read-only manager boundary. It accepts only
artifact identity and expected lineage values, fixes the core root to
`/var/lib/qintopia-hermes-core`, acquires the fixed root-owned manager lock, clears the
environment, and invokes the dependency-free planner. It rejects caller-supplied root,
service, command, or apply options.

The active release tuple is represented by one immutable generation below
`lineage/generations/`; `lineage/active` is the sole future transaction pointer. Stable
top-level `current`, `previous`, and `rollback-reserve` symlinks resolve through that
generation. The dry-run validates the complete tuple, protected artifacts, candidate in
`incoming/<commit>`, ownership, modes, lock inode, layout, and minimum 5 GiB free space.
Success always reports `pointer_changes=0` and `service_changes=0`.

`stage-hermes-core-release.sh` is the separate HC-2 candidate writer. It reads only the
identity-selected artifact below the fixed root-owned ingress
`/var/lib/qintopia-agent-os-deploy/hermes-core-ingress`, verifies it before copying,
copies into a private random directory with exclusive file creation, verifies the copy,
then atomically renames it to `incoming/<commit>` and fsyncs the directory. A failed
partial copy is moved to the fixed root-only quarantine; existing releases and pointers
are never removed or changed. A quarantine failure and a post-rename fsync failure use
distinct fail-closed error codes so an operator or later recovery routine cannot mistake
an uncertain committed candidate for a clean retry.

`fetch-hermes-core-artifact.sh` is the HC-2 ingress writer. It accepts only pinned
public artifact identity fields, downloads from a fixed COS key, validates the archive
and identity digests, and uses a bounded streaming extractor that rejects traversal,
duplicate members, links, special files, truncation, and size/count exhaustion. It
commits only to the fixed root-owned ingress while holding the fixed `flock` boundary.

`bootstrap-hermes-core-root.sh` is the one-time HC-2 bootstrap wrapper. Under a fixed
external lock and cleared environment it installs two different verified clean releases,
creates an immutable generation, and commits the initial
`(current, previous, rollback-reserve)` tuple through one `lineage/active` rename. Its
repository fixture covers interrupted release, generation, active-pointer, directory
`fsync`, quarantine, and retry recovery paths. The low-level generation commit is not a
standalone production entry point; HC-3 will call it only inside the seven-service
transaction.

Consumers must resolve `lineage/active` once and read all three role links from that
same generation. The stable top-level links are compatibility conveniences, not a safe
way to assemble a transaction snapshot through three separate reads.

HC-2 completion is repository-local only. No real clean artifact has been downloaded,
the production core root has not been bootstrapped, and no production runner request,
signature scope, service, or systemd boundary has been changed. HC-3 through HC-5 still
gate production use. The current checkout, local WeCom patches, seven-profile WeCom
configuration and enabled states, and Erhua's separate `qiwe-platform` remain unchanged.

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
  -> disable and verify the Space dispatcher/worker before ordinary release promotion
  -> download sidecar and deploy-bundle artifacts from COS
  -> verify manifests and SHA256SUMS
  -> assemble /home/ubuntu/qintopia-agent-os-releases/<release-sha>
  -> switch previous/current symlinks
  -> render and install reviewed systemd units from the immutable release
  -> enable internal AgentOS worker timers
  -> restart approved system and Hermes user-service targets
  -> smoke
  -> write deploy result JSON to COS
  -> GitHub Actions waits for the signed COS result JSON and fails the run on failed deploy
  -> archive local request state for idempotency
```

No GitHub Action should SSH to production. No routine release should run `git fetch`,
build Rust, copy source with `scp`, or edit `.hermes` live state.

After promotion, the root-owned runner renders systemd units from the release-local
bundle, installs a fixed allowlist under `/etc/systemd/system`, and enables only
internal AgentOS worker timers. Those timers may write AgentOS/Postgres state, but they
do not enable Feishu writeback, QiWe sends, or external adapters.

The generic Space automation dispatcher timer and Space automation execution service are
part of the fixed install allowlist. Release installation disables and stops both and
verifies their inactive state, even if an older Release had enabled them. They use the
signed fixed `space-automation-runtime` activation and observation targets only after
the relevant capabilities, Space policy, persistent runtime approval, database hash, and
callback authentication are in place. The matching fixed runtime one-shot target
`space-automation-runtime-rollback` stops both units and verifies persistent
disablement. None of these targets accepts unit, command, path, URL, credential, or
environment inputs.

For a non-dry-run ordinary Release, the runner also disables and verifies this runtime
before `promote-release.sh` can switch `current`. A failed pre-promotion shutdown fails
the request without moving release links. The installer repeats the shutdown after unit
installation as defense in depth; dedicated production action and Erhua profile scopes
do not pass through this ordinary Release step. After a successful shutdown, any later
promotion failure leaves the Space runtime disabled; it must not be implicitly
reactivated outside the fixed owner-approved activation request.

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
feature set while the bootstrap path accepts the exact deployed Huabaosi three-feature
manifest only when it is bound to the latest successful runtime SHA. Run bootstrap as a
dry-run first; a successful live bootstrap installs the reviewed runner while retaining
the currently deployed runtime, after which the target Release must pass its own dry-run
and full deployment. Publishing a non-prerelease GitHub Release is the production
release entrypoint; the workflow still uses the GitHub `production` environment approval
gate before it can write the signed deploy request.

For `workflow_dispatch`, both reviewed production sidecar artifacts must already exist
in COS before request upload. The primary profile remains `huabaosi-production`; the
workflow fetches Huabaosi, the QiWe companion, and the deploy bundle before it writes a
request, so a missing or mismatched companion fails in GitHub Actions rather than only
on the server runner.

Rollback uses the separate `Rollback Production` workflow. The operator supplies the
expected current published Release tag and its expected exact previous Release tag. The
workflow resolves both to commit SHAs, verifies that both are published non-prerelease
semantic versions reachable from `origin/master`, and signs the expected-current to
expected-previous lineage into the deploy request. Under the production lock, the server
requires those SHAs to match its `current` and `previous` symlinks and the persisted
`current/manifest.json.previous_sha`; an arbitrary older published ancestor is rejected.
Before request creation, the workflow read-validates the Huabaosi primary runtime, QiWe
companion runtime, and deploy bundle for that exact SHA in COS. The rollback runtime
profile remains fixed to `huabaosi-production`; it is not an operator-selectable global
profile switch.

GitHub Release assets are not part of the production deploy path. COS is the artifact
registry consumed by the server; the Release page is the operator-facing version record.

Release tags must point to the current `master` HEAD. To deploy through the normal path:
create or select a tag for the current `master`, draft the GitHub Release, then click
Publish release. Do not publish a Release for an older commit as a shortcut; use
rollback instead.

## Request Contract

Deploy requests must match `deploy-request.schema.json`. Compute `expires_at` from the
same `created_at` timestamp, not a second clock read: the server strictly limits request
lifetime to 60 minutes, including millisecond precision. The clock-advance regression
runs in `pnpm deploy:runner:check`.

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
- `production-runtime-one-shot`: exclusive scope for one owner-approved fixed action. It
  accepts one allowlisted business run, reviewed payload apply, snapshot install, or
  Qiwe webhook ingress apply/rollback target. It also accepts the fixed Space automation
  runtime rollback target, which disables the generic dispatcher and execution worker
  before verifying persistent disablement. Ingress actions are release-bound and may
  atomically replace only `/etc/nginx/snippets/qintopia-qiwe-webhook.conf`; they run
  `nginx -t`, reload, authenticated positive/negative smokes, and automatic restore. The
  runner records sanitized evidence only and never accepts callback paths, tokens, nginx
  paths, provider commands, or service names from the request.
- `restart_targets`: fixed restart groups. The runner must not accept arbitrary service
  names.
- `dry_run`: validate and assemble without switching `current` or restarting services.
- `release_rollback`: optional exact rollback lineage containing `expected_current_sha`
  and `expected_previous_sha`. It is allowed only for an ordinary Release promotion, the
  requested artifact/release SHAs must equal the expected previous SHA, and the server
  must prove the same persisted current-to-previous edge.
- `profile_dry_run_request_id`: required only for non-dry-run `hermes-profile-erhua`;
  names the reviewed dry run and must still be fresh.
- `rollback_on_smoke_failure`: must be `true` for ordinary Release promotion and
  `hermes-profile-erhua`; ordinary promotion failure recovery cannot be disabled by a
  request field.
- `created_at` and `expires_at`: the runner accepts at most 60 minutes of request TTL,
  rejects a request more than 15 minutes old, and permits at most five minutes of future
  clock skew.
- `signature`: HMAC-SHA256 signature over the unsigned request body. The GitHub
  `production` environment and the server must share `DEPLOY_REQUEST_SIGNING_KEY` and
  `DEPLOY_REQUEST_SIGNING_KEY_ID`. `signature.signed_at` must stay within five minutes
  of `created_at` and satisfies the same age and future-skew limits.

The COS request prefix is intentionally fixed to `qintopia-agent-os`. Bucket, region,
and endpoint can vary by environment; the production queue path cannot.

COS write access alone is not sufficient to trigger deployment. The server rejects
unsigned requests and requests signed with the wrong key.

For ordinary releases, the runner snapshots the original `current` and `previous` under
the deployment lock. Any failure after `current` changes, including a promoter error
after the link switch, systemd installation failure, or smoke failure, automatically
restores the original `current`, its managed units, and the original `previous` pointer,
including restoring an originally absent `previous` pointer to absence. Artifact
download, request validation, lineage validation, or staging failures before a link
change must not move a healthy `current`. Erhua profile requests never switch release
links; their rollback restores only the backed-up profile files before restarting Erhua.

Server-local release rollback requires the caller's exact expected current and previous
SHAs and revalidates both symlinks plus the current manifest lineage before mutation. It
restores the previous release's complete managed systemd unit set through that immutable
release's installer. It strictly compares the current and previous installer manifests,
stops and disables candidate-only units, removes only those validated unit filenames,
and reloads systemd. Repointing `current` alone is not a complete rollback.

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
- runner `ProtectSystem=strict` and `ProtectHome=read-only`; verify the effective
  properties after installing or upgrading the root-owned unit
- runner `ReadWritePaths` access to `/etc/systemd/system` only for the reviewed fixed
  root-unit manifest and to `/etc/nginx/snippets` only for the reviewed Qiwe webhook
  include; do not grant `/etc` or `/etc/qintopia`
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
server-written result in COS after the request is uploaded. Production activation,
runtime observation, and runtime one-shot workflows always wait for this result. Do not
dispatch those workflows until the server timer is active and the deployed runner can
sign results; an unconsumed request correctly times out instead of reporting success.

Ordinary production deploy, rollback, Hermes cron apply, and legacy cron retirement
retain the transition flag `WAIT_FOR_SERVER_DEPLOY_RESULT`. Set it to `true` only after
the same server prerequisite is observed. A workflow that does not wait proves request
publication only, not server execution.

Every normal result and bounded poller validation-failure result carries an HMAC-SHA256
signature with issuer `qintopia-deploy-runner`. The wait script verifies the configured
key id, binds `signed_at` to `finished_at`, and authenticates the complete result before
accepting status or request identity. The result currently uses the same shared HMAC key
as request signing; this excludes a COS-only writer, but it is not an asymmetric server
identity.

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
optional-wait workflows will correctly time out because no server process is consuming
deploy requests. Do not run the three unconditional-wait workflows in that state.

## Validation

```bash
pnpm deploy:runner:check
pnpm check:light
```

## Operating rules

Paths below are repository-relative; Sidecar subsections use `runtime/sidecar/`.

### Commands

- COSCLI installer defaults must use a versioned official GitHub release asset and the
  matching asset digest, not the floating `coscli-linux-amd64` alias. The alias can move
  before Tencent download docs update their SHA table and break production release
  deploys at install-time checksum verification.
- Deploy-runner production one-shots run from the root service boundary. If a one-shot
  needs ubuntu user systemd, use fixed `/usr/sbin/runuser -u ubuntu` with
  `XDG_RUNTIME_DIR=/run/user/<ubuntu-uid>` and the matching user bus address; direct
  root `systemctl --user` cannot prove the ubuntu user timer boundary.
- After ordinary release promotion, deploy-runner must execute
  `deploy/runner/install-release-systemd-units.sh` and `deploy/runner/smoke-release.sh`
  from the just-promoted release directory, not from the root runner's already-installed
  `RUNNER_DIR`. The root runner may be older than the release it is promoting, so using
  stale runner-local install or smoke logic can block self-bootstrap fixes before they
  reach production.
- Release Please PR manual CI validation:
  `gh workflow run ci.yml --ref <release-please-head-branch> -f release_please_pr_number=<pr-number>`
- Release Please PR required PR-Agent check validation:
  `gh workflow run pr-agent.yml --ref <release-please-head-branch> -f release_please_pr_number=<pr-number>`
- Staging runtime values metadata observation smoke:
  `QINTOPIA_STAGING_RUNTIME_VALUES_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/staging-runtime-values-observation-smoke.sh`
- AgentOS downstream evidence/visual timers observation smoke:
  `QINTOPIA_OPERATIONS_DOWNSTREAM_TIMERS_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/operations-downstream-timers-observation-smoke.sh`

### Core Rules

- When adding a new workflow package, update `registry/workflows.yaml`,
  `tools/workflows/check-workflows.mjs`, and `deploy/restart-target-rules.yaml` in the
  same PR. CI treats unmatched `workflows/**` files as production-adjacent.
- CI must install the fixed cargo-nextest and cargo-llvm-cov versions from checksum-
  verified prebuilt releases through `taiki-e/install-action` with Cargo fallback
  disabled. Do not restore per-run `cargo install`; it reintroduces crates.io index
  failures before tests start. Prepare a non-empty diagnostic artifact before tool
  download so an installation failure cannot be obscured by a second missing-artifact
  error.
- Local PR validation is also risk-tiered. Use `pnpm check:pr:auto` before opening an
  ordinary PR; it always runs the quick tier and escalates to heavy Rust checks for
  sidecar, Postgres, deploy script, and CI workflow changes. Use `pnpm check:pr:heavy`
  when you want the full local quick + Rust + disposable PostgreSQL mirror and local
  `qintopia_test` is ready on `127.0.0.1:5432`.
- Document first for new features, behavior changes, migrations, runtime changes, or
  production-adjacent work.
- Do not manually edit root `CHANGELOG.md` in ordinary feature or fix PRs. Release
  Please owns routine release changelog updates from merged Conventional Commits.
- Merging a Release Please PR prepares a version and draft GitHub Release. Manual owner
  publication remains the default production boundary.
- Low-risk classification is a safety boundary for the conversational programming-
  extension runner, not approval to merge or publish. Every generated candidate PR,
  Release Please PR, and draft Release must be reviewed and advanced explicitly by the
  owner.
- If any earlier Release Please version or draft GitHub Release in the current release
  sequence was not published, do not publish the newest version. Stop, reconcile or
  delete the unpublished drafts as an explicit release decision, then regenerate and
  validate a fresh Release Please PR instead of skipping ahead.
- Before publishing a draft GitHub Release, confirm its tag points to current
  `origin/master`. If `master` advanced after the draft was prepared, do not publish or
  retry the stale tag; validate and publish the next Release Please PR instead.
- Production release deploy resolution must not scan unbounded historical Actions logs.
  Keep `deploy-production.yml` release-run lookup bounded to recent completed release
  deploy runs and keep `tools/deploy/collect-release-deploy-results.mjs` enforcing a
  `--max-release-runs` cap before it calls `gh run view --log`.
- Do not merge a Release Please PR unless the draft GitHub Release will be published or
  intentionally deleted in the same release decision. The repository release manifest
  must track the latest published Release tag; deleted draft-only releases must not
  remain as the Release Please baseline.
- A Release Please PR created or updated with `GITHUB_TOKEN` may have no automatic PR
  checks because GitHub suppresses recursive workflow triggers. Before merging such a
  PR, run the manual CI validation command on its exact head branch and require the
  workflow `changes`, `check`, `Rust quality baseline`, and `PostgreSQL integration`
  jobs plus the PR-attached `Release Please validation` commit status to pass. Run the
  manual PR-Agent validation on that same exact head when the ruleset-required
  `PR-Agent review assistant` check was suppressed. Both dispatches must fail if the PR
  is not open, does not target `master`, is not bot-authored, or the checked-out SHA
  differs from the PR head. The authenticated PR-Agent dispatch must skip external AI
  review and must not edit or comment on the generated Release PR.
- Do not hot-edit production servers.
- Existing `/etc/qintopia/message-sidecar.env` owner/mode drift must be repaired only by
  reviewed deploy/runner code. The release systemd installer normalizes it to
  `root:ubuntu 0640`; do not run ad-hoc production `chown`/`chmod`.
- Any script expected to exist under `/home/ubuntu/qintopia-agent-os-releases/current`
  after deployment must be included in `tools/deploy/build-deploy-bundle.mjs` and
  guarded by `tools/deploy/check-deploy-contracts.mjs`; adding a repo file alone does
  not put it on the production release root.
- Production COS fetch must leave `artifact-manifest.json`, `SHA256SUMS`, and packaged
  archives mode `0444`, while the sidecar binary remains `0755`. These files are
  immutable non-secret release evidence needed by unprivileged release-local
  observation; mode `0640` can make a valid release unverifiable after root-owned
  promotion.
- Production COS archive extraction runs under the root deploy runner and must use
  `tar --no-same-owner` for both sidecar and deploy-bundle payloads. Never preserve
  GitHub runner numeric owners from an artifact archive or propagate them into the
  immutable production release with `cp -a`; the promoted release tree must remain owned
  by the deploy runner.
- Staging sidecar provisioning runs as the `ubuntu` operator, not root. It must create
  the fixed staging release root, release directory, and sidecar directory with explicit
  mode `0755` independent of ambient `umask`, then freeze the immutable release and
  sidecar directories to `0555`. Failed attempts may remove only paths they created;
  they must not reuse or delete an existing release directory.
- CI must execute non-ignored sidecar tests with all Cargo features so staging-only
  adapter tests actually run. This is test coverage only: ignored PostgreSQL tests
  remain in the disposable integration job. Production artifacts must still use only
  reviewed production features; an all-features CI build must never be promoted or
  treated as a production artifact.
- Heavy PR checks are risk-tiered. Keep `check` meaningful for ordinary PRs, but run
  `rust-quality-baseline` and `postgres-integration` only for sidecar, Postgres, deploy
  sidecar script, or CI workflow changes. Explicit manual dispatches and authenticated
  Release Please validation force the full light, runtime, Rust, and PostgreSQL tiers.
  Do not weaken production deploy or published Release gates; those remain the full
  safety boundary.
- `staging-runtime-prerequisite-observation-smoke.sh` is a read-only observation gate
  for fixed staging env and immutable release prerequisites. It must never read env
  contents, execute the sidecar, connect to Postgres, call external services, install
  units, enable timers, or report secret-bearing values. Its path checks must lstat
  every parent component and reject symlinks, non-directories, group/world-writable
  parents, unexpected parent owners, and a sidecar binary the running user cannot
  execute; tests for these checks must use repository-local temporary roots, not `/tmp`.
- Server Hermes patches under `docs/operations/review-pool/hermes/` are non-deployable
  migration evidence. Do not add them to release bundles or apply them to production;
  migrate each accepted behavior into an owned package with focused tests and a separate
  cutover PR.
- The disposable operations apply smoke may exercise the Huabaosi retry state only when
  both `huabaosi-staging-adapter` and `postgres-integration-tests` are compiled,
  `QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE=1`, the database is exactly `qintopia_test` on
  a literal loopback IP with its approved URL hash, and every provider/media endpoint
  and allowlist host is a literal loopback IP. This exception must never accept an
  external host or production database.
- The first release containing a deploy-runner behavior change is processed by the
  previous runner. Use a reviewed follow-up `workflow_dispatch` request for the same
  published SHA to activate the new runner behavior; do not bootstrap it with server
  edits.
- If the previous runner rejects the new Huabaosi artifact feature contract before
  promotion, the default-disabled `legacy_runner_bootstrap` workflow mode is the only
  allowed bridge. It must bind the legacy runtime to the latest trusted successful
  deploy result, accept only the exact deployed Huabaosi three-feature artifact, use a
  distinct transition release SHA, and restrict scope/restarts to `deploy-bundle` and
  `qintopia-system-services`. Normal fetches must continue to require the current
  three-feature artifact. Run a dry-run before any live bootstrap.
- Deploy result diagnostics may include only bounded non-secret runner facts such as the
  fixed failure stage, numeric exit status, promotion state, and profile activation
  attempt state. Do not upload raw server logs, journal output, env files, secrets,
  external adapter payloads, or command output into COS deploy result JSON.
- If the server poller rejects a malformed deploy request before the runner starts, it
  must still upload a bounded `status=failed` result with
  `checks=[{"name":"deploy-request-validation","status":"failed"}]`. That fallback may
  normalize invalid SHA/profile/scope/restart fields; `wait-deploy-result.sh` must
  accept only this exact validation-failure shape and keep strict identity matching for
  every other deploy result.

### Sidecar Rules

- Treat `deploy/sidecar/docs/server-deployment.md` as historical rollback evidence, not
  the current deployment path.
- Group-message send-readiness and policy-denial transitions must release the complete
  claim tuple (`claimed_by`, `locked_at`, and `claim_expires_at`) and require exactly
  one work-item update before appending the corresponding audit event.
- The complete sidecar suite needs a 32 MiB test-thread stack. `pnpm test:sidecar` and
  CI set `RUST_MIN_STACK=33554432`; this is test-only and must not be copied into the
  production sidecar service environment.
- Huabaosi live provider/media execution must compile with exactly one non-default live
  feature: `huabaosi-staging-adapter` or `huabaosi-production-adapter`. A build with
  neither or both must reject apply before Postgres. Staging keeps the exact owner
  phrase and reviewed staging database hash. Production must verify the exact production
  approval phrase, deployed release SHA binding, database URL hash binding, and adapter
  policy before Postgres or external I/O; shell scripts cannot be the only enforcement
  point.
- Production mirror observation must discover
  `release/current/sidecar/qintopia-message-sidecar` or accept `QINTOPIA_SIDECAR_BIN`
  only when it resolves to that same immutable binary with the approved production
  features; source-tree `cargo run` fallback is forbidden. Its shell may parse only the
  mirror enable flag; a direct child launcher may pass only that parsed flag and the
  non-secret release SHA to the immutable binary without sourcing shell, importing
  secrets into the shell, or writing a secret-bearing temporary file. It may run only
  the non-secret mirror observation preflight, not full configuration preflight or
  worker dry-run. Non-allowlisted env values must be ignored before mirror-flag value
  validation. Activation must fail before preflight or timer mutation until persistent
  mirror enablement is present exactly once and exactly `1`; timer rollback must stop
  external work immediately and fail closed until it is present exactly once and exactly
  `0` in the reviewed environment file.
- A staging-feature callback apply must validate explicit enablement, API/media/group
  allowlists, and webhook readiness before reading stdin. Upload apply must validate the
  same adapter configuration before connecting to Postgres.
- CI must execute the non-ignored sidecar suite with all features so staging-only
  adapter tests run, then run warning-denied Clippy once with no default features and
  once with all features. The all-feature test/build is CI-only and cannot stand in for
  the production feature set; ignored PostgreSQL tests stay in their disposable
  integration job.
- External adapter modules must use `bounded_http`; do not add another raw socket HTTP
  implementation. Test-only loopback HTTP is allowed, while production clients require
  HTTPS and the reviewed endpoint/host allowlists.
- Production timer activation should use the `Activate Production Timers` GitHub
  workflow after the reviewed release containing the runner support is deployed. It
  creates a signed `production-activation` deploy-runner request and accepts only these
  fixed targets: `erhua-morning-brief`, `space-automation-runtime`,
  `xiaoman-weekly-recruitment`, `xiaoman-weekly-plan-confirmation`,
  `xiaoman-weekly-preview`, and `xiaoman-daily-case-report-auto-publish`. The activation
  request does not retire legacy cron files, write persistent production config, or
  promise automatic rollback; each selected target requires its owner-approved
  production config to have been applied first. Legacy cron retirement must be handled
  through the explicit `production-legacy-cron-retirement` request and evidenced before
  activation retries. The `xiaoman-weekly-recruitment` and
  `xiaoman-weekly-plan-confirmation` target additions are a 2026-08-09 owner-approved
  fixed-boundary expansion for the Xiaoman weekly minimum loop; they may enable only
  their own release-managed systemd timers and must not send, publish, write Feishu,
  call Erhua, or call QiWe. `space-automation-runtime` must be the only target in its
  activation request. It may enable only the generic dispatcher timer and Space
  execution worker after the fixed production approval, database hash, Qiwe host,
  companion artifact, authenticated ingress, and exact-unit observation checks pass. A
  release installation disables and stops both units; every new release therefore
  requires a fresh explicit activation.
- Production runtime observation should use the `Observe Production Runtime` GitHub
  workflow after the reviewed release containing the runner support is deployed. It
  creates a signed `production-observation` deploy-runner request and accepts only these
  fixed targets: `qiwe-image-send`, `space-automation-runtime`,
  `xiaoman-daily-case-report-auto-publish`, `hermes-cron-snapshot`,
  `hermes-cron-live-parity`, and the worker-run evidence targets
  `erhua-morning-brief-worker-run`, `xiaoman-daily-case-report-worker-run`,
  `xiaoman-weekly-recruitment-worker-run`,
  `xiaoman-weekly-plan-confirmation-worker-run`, and
  `xiaoman-weekly-preview-worker-run`. `hermes-cron-snapshot` reports only safe
  server-local snapshot unit/repo facts, and `hermes-cron-live-parity` reports only
  reviewed/live/enabled counts after comparing the reviewed registry to live
  declarations including `deliver` and `origin` boundaries. Do not hard-code the
  reviewed job count in live-parity observation; the registry may grow while the safe
  output remains bounded counts only. Migrated worker-run targets prove the reviewed
  Hermes cron wrapper wrote a latest `<timestamp> <task> run=ok` sentinel for the
  expected Asia/Shanghai schedule date and the worker exited successfully; stale
  sentinels from an older scheduled date must fail as `scheduled_run_missing`, not be
  classified as the current worker result. Erhua morning brief observation must also
  parse the worker's sanitized summary from that latest log segment and verify the text
  artifact was created plus the optional auto-publish summary reports
  `external_send_executed=true`; never fall back to sentinel-only success for Erhua
  sends. Weekly targets also validate the worker's `latest-summary.json` draft
  invariants. When the fixed Hermes cron log is absent or contains no reviewed sentinel
  for the task, the observation passes with `<key>_worker_run_result=not_started`;
  before the first scheduled trigger this means the Hermes job has not fired yet, not a
  regression, while `not_started` after the scheduled time means the Hermes job did not
  reach the reviewed wrapper and needs reviewed investigation. Observation is read-only:
  it may run only fixed release-local observation scripts, must not enable or disable
  timers, write persistent config, retire legacy cron files, call QiWe/Feishu/Postgres
  mutation commands, or run activation/rollback scripts, and must not print live
  `jobs.json`, group ids, prompts, env values, snapshot contents, or raw script output.
  `space-automation-runtime` additionally verifies the exact release-bound unit bytes,
  enabled/active state, scheduled timer value, and that the live worker PID resolves to
  the current immutable Qiwe companion binary; it never reports process arguments or
  environment values.
- Production immediate worker/backfill runs should use the
  `Run Production Runtime One-Shot` GitHub workflow after the reviewed release
  containing the runner support is deployed. Erhua one-shots still require the
  corresponding release-managed timer to be enabled; Xiaoman daily case-report backfill
  is the reviewed Hermes-cutover exception and must not require the retired systemd
  timer to be enabled. Control targets enforce their fixed target-specific
  preconditions. It creates a signed `production-runtime-one-shot` deploy-runner request
  and accepts exactly one fixed target per request: `erhua-morning-brief` with
  `approved-production-erhua-morning-brief-one-shot`, or
  `xiaoman-daily-case-report-auto-publish-backfill` with
  `approved-production-xiaoman-daily-case-report-auto-publish-backfill` and `YYYY-MM-DD`
  backfill date, or `xiaoman-daily-case-report-approval-repair` /
  `xiaoman-daily-case-report-read-through-repair` /
  `xiaoman-daily-case-report-chat-id-repair` with
  `approved-production-xiaoman-daily-case-report-config-v1`, or
  `qiwe-image-send-intro-text-enable` with
  `approved-production-qiwe-image-send-intro-text-v1`, or
  `xiaoman-creative-profile-candidates-apply` with
  `approved-production-xiaoman-creative-profile-candidates` and the 64-hex SHA-256 of
  the fixed server-local reviewed payload, or `hermes-cron-snapshot-install` with
  `approved-production-hermes-cron-snapshot` and empty `backfill_date` when
  `hermes-cron-snapshot` observation reports
  `hermes_cron_snapshot_observation_error=unit_missing`, or `qiwe-webhook-ingress-apply`
  with `approved-production-qiwe-webhook-ingress-apply`, or
  `qiwe-webhook-ingress-rollback` with
  `approved-production-qiwe-webhook-ingress-rollback`, or
  `space-automation-runtime-rollback` with
  `approved-production-space-automation-runtime-rollback`. The snapshot target installs
  only the fixed server-local snapshot timer and baseline snapshot repo, then should be
  followed by observation with `hermes-cron-snapshot,hermes-cron-live-parity`. This path
  may create real production publish/send side effects through the reviewed worker
  boundaries, but it must not write persistent config, enable/disable business worker
  timers, retire cron files, accept multiple targets, or record raw worker output, live
  cron JSON, group ids, prompts, database URLs, tokens, person ids, reviewed profile
  payload content, Feishu payloads, QiWe payloads, message content, snapshot contents,
  or journal logs. Runtime one-shot entrypoints must emit a bounded
  `qintopia_runtime_one_shot_safe_failure=` marker for every pre-worker failure as well
  as worker failures; otherwise deploy results collapse to bare `exit 1` and production
  troubleshooting loses the reviewed boundary.
- As of `v0.2.30`, an existing release first assembled by `v0.2.29` may have a
  `manifest.json` that omits `runtime_artifact_profile` even though the immutable
  sidecar artifact manifest already records the reviewed profile. The same-SHA repair
  path must adopt that profile from `sidecar/artifact-manifest.json`, then persist it
  back into the release manifest before exact identity comparison. Do not hot-edit the
  server manifest by hand.
- As of 2026-07-15, the corrected `v0.2.10` same-SHA follow-up deploy installed the new
  systemd units. A same-SHA request for an existing release must reuse the immutable
  manifest's exact runtime, runtime artifact profile, bundle, commit, scope, and
  restart-target fields. The only content exception is installing a complete missing
  QiWe companion into a legacy Huabaosi-only release without changing the primary
  binary. Narrowing `restart_targets` is rejected before promotion and does not trigger
  rollback. Content, path, type, or symlink drift must fail before mutation. After the
  bounded metadata or companion repair allowed above, the existing tree must satisfy the
  same deploy-runner owner, non-writable, directory accessibility, regular/symlink type,
  sidecar `0755`, and metadata `0444` checks as a new staging tree. Same-SHA reuse must
  preserve a distinct `previous` target. Production release and staging roots must be
  created explicitly as `0755` so the validation contract does not depend on ambient
  `umask`.
