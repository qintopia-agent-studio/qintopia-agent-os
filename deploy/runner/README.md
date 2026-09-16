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

## Agent operating contracts

Read the relevant topic when changing this capability. These documents retain the full
constraints behind the scoped AGENTS.md summaries.

- [Release and deployment contract](docs/agent-contract.md)
