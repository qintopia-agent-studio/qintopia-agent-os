# Production Deploy Runner

## v0.3.7 提升前失败的有限闭合

首次 O→T 接管现在必须执行已验完整 staged
bundle 内的 runner 及其同版本 helper。launcher 与 poller 都验证 manifest 文件摘要、所有权及安全目录；poller 额外要求 bundle
commit 等于新签名请求的
`deploy_bundle_sha`，不再执行 O 中的旧 runner。继承的 deploy 锁仍覆盖 journal、执行、结果上传和归档。普通发布入口不变。已领取接管请求未完成核对前，禁止替换其 staged
bundle；提升后中断恢复必须验证原执行文件摘要及其与原签名请求绑定的 bundle。

Space automation 停用只对 `failed` unit 执行
`reset-failed`，避免空闲 unit 被 systemd 卸载造成误报。disable/stop 失败、非 inactive 状态、残留 PID 或非空/未知 cgroup 仍阻止提升。没有放宽业务发送、任务排空或配置保留约束。

对本次已领取的请求，禁止使用
`retire-unstarted`。保留旧包，将新完整审查产物按原 staging 流程验摘要后放入固定 staged 目录，再执行：

```bash
sudo /var/lib/qintopia-agent-os-deploy/recovery/staged/payload/deploy/runner/recover-release-lineage.sh \
  --request-id deploy-20261002T105028Z-16e8d56b9800
```

该入口仅处理原 O/P、T 不存在、无 claim、请求已归档 failed，并且本地及 COS 签名回执完全一致的
`quiesce-space-automation-runtime`
提升前失败。它验证原请求、journal、manifest、hold、消费者退出及调用身份，以 takeover→poller→deploy 顺序持锁。

先持久化包含请求/回执/helper 摘要的闭合审计，再归档消费标记。原 journal、请求、回执和 hold 均保留，不重启业务服务、不回放请求、不变更指针或业务数据。

闭合成功后必须生成新签名请求，不能复用旧 ID；原子审计完成但标记归档中断时，可重复闭合以完成归档。新请求绑定后旧闭合入口失效。缺失回执、未知上传、仍有 claim、已创建 T、后续 journal 或不一致证据全部保持阻断，不能用删除文件解除。

新 bundle 的身份独立记录，不要求等于已失败请求使用的旧 bundle。新 live 接管必须等于它自己的 bundle 身份。接管成功后仍按既有 finalize 和完整六目标发布流程验收，岸岸排空超时暂缓，Hermes 核心升级单独执行。首次成功完整生产路径前不宣称部署修复完成。

## First-Takeover Failure Before Claim

COSCLI writes to its executable directory by default. The poller explicitly selects its
private temporary log directory, including existence probes that need error output;
changing cwd or suppressing all logs does not preserve that error contract.

The launcher records `preparing`; the child publishes its durable claim before writing
`takeover-consumed` and before invoking the runner. A claim or any uncertain execution
still uses the existing recovery path, never a replay. A launcher lock serializes
prepare, consume, retirement and finalization; child locking remains poller then deploy.

For a pre-claim failure, use only the reviewed, digest-verified staged launcher:

```bash
sudo /var/lib/qintopia-agent-os-deploy/recovery/staged/payload/deploy/runner/run-fixed-takeover-request.sh \
  retire-unstarted '<original-request-id>'
```

This requires the original signed request to have expired by more than five minutes,
unchanged O/P pointers, no T tree, no claim/journal/local result, stopped consumers and
no residual deployment processes. It verifies the exact remote result key is absent
through authenticated HTTP 404/XML `NoSuchKey` with an exact `Resource` path or `Key`.
When both identity fields exist, both must match; missing or conflicting identity
refuses retirement. Unknown or conflicting execution evidence also refuses retirement.
The private existing COS environment stays on the server.

Retirement durably records the request digest in `takeover.json`, archives a legacy
pre-claim marker, and preserves partial downloads. It leaves the timer disabled and hold
in place. Repeat retirement with the same ID to finish an interrupted archive. Then
issue a **new** signed takeover request and use ordinary `consume`; never reuse the
retired ID or manually remove the hold. Only verified successful finalization restores
the original timer state. Cancellation of a GitHub run does not withdraw a COS request.

For the v0.3.5 incident, the staged bundle is still the old immutable artifact. Do not
hot-edit or overwrite individual staged scripts. A reviewed replacement bundle and its
published digest must be acquired before using the new mode; retain the original bundle,
hold and takeover evidence. See the
[incident record](../reports/2026-10-02-v035-runner-takeover.md).

## Management UI Preparation And Stop

`agentos.qintopia.cn` resolves to the origin, but DNS is only the first external
prerequisite. The reviewed root operator prepares the dedicated account and five-key
private environment, installs the HTTP challenge site, requests the separate exact-host
certificate, verifies renewal, and installs the HTTPS proxy before approving activation.
Verify the real live tenant, UI database role's effective privileges, manager identity
and grant, binary traversal, route 503 while stopped, and the 30-second drain on the
exact R binary. Do not install a certificate or activate the service as a side effect of
release installation.

The installer always leaves `qintopia-agentos-management-ui.service` disabled and
stopped. Runner/recovery call the fixed lifecycle helper while retaining their deploy
lock; unknown stop outcomes retain the request hold and require readback of the original
operation and audit/version evidence. After R to T, leave the HTTPS route in place
returning 503. A restored R is not automatically reactivated. Removing the route and
archiving only its renewal declaration is a separate reviewed decommission step. Chrome
login, permission, save and rollback checks remain manual production acceptance.

Before first `prepare`, HTTP-site or certificate work, identify an existing reviewed
request-bound hold that remains valid for the entire maintenance action. The fixed O->T
launcher creates an unbound hold in `prepare` and automatically finalizes and removes it
on successful `consume`. The ordinary T->R success path leaves a direction journal but
no maintenance hold. Neither path currently supplies a supported first-setup window. See
the dated
[v0.3.4 preparation and patch record](../reports/2026-09-28-management-ui-deploy-wiring.md#2026-09-29-maintenance-patch-implementation-local-only)
for the exact source, isolated failure reproduction and implemented `begin`/`finish`
maintenance entry on this branch. Those modes do not exist in fixed R. Do not race a
consumer, hand-write a hold or turn a successful deployment into an artificial recovery.
Keep the UI disabled until a reviewed immutable release supplies the entry and its
signed T-to-R success is verified. Keep the old v0.3.4 draft unpublished and its tag
unchanged. After the maintenance patch is reviewed and merged through the protected
process, use the next valid Release Please version for the first usable UI. Publishing
the old R first would leave UI setup blocked until a later release.

On a later reviewed immutable release containing these modes, invoke its exact current
helper with the original successful T-to-R live request ID. `begin` takes FD 8 then FD 9
itself and requires the processed signed request/result, matching direction journal, R/T
pointers, no claim, and the private existing signing environment. Only after it returns
successfully may the separately approved `prepare`, HTTP, certificate and HTTPS steps
run. Verify the stopped HTTPS 503 route, then run `finish` with the **same** ID; it
restores the timer's original enabled/disabled state before releasing the hold. If
either command returns an uncertain result, inspect the durable maintenance record, hold
and timer, then resume only with the same ID. Do not remove the hold manually or start
the UI before `finish` succeeds. This is a future command shape, not a production action
authorized by this document:

```bash
UI_HELPER="/home/ubuntu/qintopia-agent-os-releases/<reviewed-release-sha>/deploy/runner/management-ui-lifecycle.sh"
sudo "$UI_HELPER" begin '<original-T-to-R-live-request-id>'
# Run separately approved account, database, HTTP, certificate and HTTPS checks.
sudo "$UI_HELPER" finish '<original-T-to-R-live-request-id>'
```

The runner may leave the processed request and signed result at mode 0644 within its
root-owned 0700 state directory; `begin` accepts that layout after checking ownership,
single-link regular files, signature and digest. The direction journal and maintenance
record must remain mode 0600. A failed record or directory `fsync` must leave the timer
in its original state; inspect the record before retrying the same ID.

### Management UI Preflight And Rollback Commands

Run these on the reviewed origin host with the approved immutable R release SHA, after
the signed request-bound hold exists. The operator supplies `RELEASE_SHA` from the
reviewed artifact identity; the commands do not create an account, grant a role, request
a certificate or enable the UI.

```bash
RELEASE_SHA='<reviewed-40-character-SHA>'
RELEASE_DIR="/home/ubuntu/qintopia-agent-os-releases/$RELEASE_SHA"
test "$(readlink -f /home/ubuntu/qintopia-agent-os-releases/current)" = "$RELEASE_DIR"
test -x "$RELEASE_DIR/sidecar/qintopia-message-sidecar"
namei -l "$RELEASE_DIR/sidecar/qintopia-message-sidecar"
getent passwd qintopia-management-ui
sudo -u qintopia-management-ui test -x "$RELEASE_DIR/sidecar/qintopia-message-sidecar"
sudo stat -c '%U:%G %a' /etc/qintopia/collaboration-management-ui.env
sudo systemd-analyze verify /etc/systemd/system/qintopia-agentos-management-ui.service
systemctl show qintopia-agentos-management-ui.service \
  --property=LoadState,ActiveState,UnitFileState,User,Group,Restart,SendSIGKILL,TimeoutStopUSec
sudo nginx -t
```

Require the file mode to be `root:root 600`, the unit to be disabled and inactive before
activation, and the rendered unit to have `Restart=no`, `SendSIGKILL=no`, the dedicated
user/group and a 35-second stop timeout. Check binary traversal as that service user;
the file's execute bit alone is insufficient. In the approved administrator database
session, inspect all five `qintopia_identity.management_ui_lock_%` function owners,
`SECURITY DEFINER`, fixed `search_path`, explicit UI/runtime EXECUTE and effective ACL.
Confirm `PUBLIC EXECUTE` is absent; the UI must have no Person/Gateway UPDATE, Gateway
MAINTAIN, identity-schema CREATE or owner-role SET. Run the restricted-role nonempty
transaction and concurrent Gateway insert probe from the
[lock capability design](../../runtime/postgres/docs/data-design/2026-09-28-management-ui-lock-capabilities.md)
before enabling the unit. A default ACL listing alone does not prove effective access.

After the separate certificate and renewal operation, verify the exact SAN, renewal,
route and stopped state before activation:

```bash
sudo openssl x509 -in /etc/letsencrypt/live/qintopia-management-ui/fullchain.pem \
  -noout -checkend 604800 -ext subjectAltName
sudo certbot renew --cert-name qintopia-management-ui --dry-run
sudo nginx -t
curl --noproxy '*' --silent --show-error --output /dev/null --write-out '%{http_code}\n' \
  --resolve agentos.qintopia.cn:443:127.0.0.1 https://agentos.qintopia.cn/
```

The stopped HTTPS response must be `503` with no public-site content. When a reviewed
rollback or R to T operation requires quiescence, the runner/recovery helper owns FD 9.
For an explicitly approved root maintenance stop outside that process, take FD 8 then FD
9 and use the same immutable helper:

```bash
UI_HELPER="$RELEASE_DIR/deploy/runner/management-ui-lifecycle.sh"
sudo bash -c '
  exec 8>/var/lib/qintopia-agent-os-deploy/poller.lock
  flock -n 8 || exit 75
  exec 9>/var/lib/qintopia-agent-os-deploy/deploy.lock
  flock -n 9 || exit 75
  "$1" quiesce || exit $?
  "$1" verify-closed
' bash "$UI_HELPER"
```

Keep the HTTPS site installed and returning `503` while T runs. A failed HTTPS switch
restores the prior reviewed HTTP site inside `install-https`; check `nginx -t` and the
HTTP `404` bootstrap route before deciding any next action. If quiescence or
`verify-closed` returns nonzero, retain the hold and inspect the original invocation,
operation ID and audit/version state; do not replay a possibly committed save or restart
the UI. For an interrupted pointer operation, the reviewed recovery entry is
`$RELEASE_DIR/deploy/runner/recover-release-lineage.sh --request-id <original-request-id>`
under its own signed-request and absence checks. Do not change release pointers or
delete the certificate and renewal declaration by hand.

This document records the intended production deploy automation after the server moved
to `qintopia-agent-os-releases/current`.

The approved takeover/recovery implementation contract is the dated
`docs/reports/2026-09-26-v033-runner-takeover-and-rollback.md`. Before any first live
takeover, an immutable reviewed recovery bundle and persistent service hold must be
prepared, and the fixed consumer must bind the expected signed request ID at the actual
COS pointer read. An unfinished local claim or uncertain signed result blocks new
consumption. Recovery distinguishes O→T, T→R and R→T by a durable request-bound journal
and checks the original request/result and release manifests before changing pointers.
Normal rollback requires a fresh production-signed request. Neither local validation nor
a merged implementation PR is production authorization. After a fixed takeover has
produced a signed success result, `finalize <request-id>` may repeat only the evidence
checks and timer restoration; it never replays the deploy request. Before any timer
change it takes `poller.lock` then `deploy.lock`, checks the current hold belongs to the
original takeover, and refuses any other claim or later recovery journal. The reviewed
hold drop-in remains installed. The hold file is removed only after an originally
enabled timer is again enabled and active. A later T→R or R→T hold has its own request
identity; an old `finalize` cannot remove it even if pointers returned to T/O. An
interrupted original finalization remains blocked and can resume from durable evidence.
Draft PR #726 implements the finite recovery matrix for a whole-process crash before any
signed result. The new poller records `not_started` for result upload in the claim and
original direction-bound journal before execution, with the original request and
immutable execution identity. Before any result PUT it must atomically fsync
`upload_intent` and the exact payload digest in both records. A missing stage, old
claim, partial update or any upload intent is unknown even if COS currently returns 404;
waiting and probing again cannot prove an already-sent PUT will not land.

The fixed recovery helper first retains the hold/drop-in, stops the ordinary timer,
proves the ordinary and fixed transient consumer, uploader, child processes and Anan
helper have ended through unit PID/cgroup state, and takes `poller.lock` before
`deploy.lock`. A missing transient unit needs persisted proof that this request started
in that unit; unreadable unit state remains unknown. It compares the fixed COS request
object with the original signed request. Only a result-key object request returning the
specific COS `NoSuchKey` service code establishes remote absence. Authentication,
timeout, other 404/error codes or conflicting local/COS/journal evidence retain hold.

With authoritative absence and matching new `not_started` records, the helper may use
only the direction-bound finite CAS and real rollback primitive from the reviewed table.
It records the missing original result in a separate fsynced maintenance field and never
creates a signed failed result, retries the request, or rolls back business data. A
retry can resume after a completed pointer CAS or installer phase using the persisted
phase and verified original pointers. An interrupted rollback primitive, installer or
smoke with an unknown outcome keeps the hold for manual review; restored pointers alone
do not establish that service installation and smoke completed. A trusted remote
`succeeded` result prevents reversal: only exact signed bytes and a missing local
archive may be repaired after complete manifest, pointer and request checks. Existing
different bytes stop reconciliation. COS absence beside a local signed success also
prevents reversal. The helper leaves hold installed after recovery; release of isolation
requires a separate review of the queue pointer and transaction. The owner approved a
one-file test-scope addition on 2026-09-27 for the existing Anan/Erhua smoke fixture,
bringing this Draft PR to 18 paths. That fixture must verify the fixed-release transient
unit invocation and its failure receipts while retaining the Erhua, Profile activation
and system-service sentinels. Passing simulated fixture and Ubuntu PID1 fault matrices
does not authorize a first live takeover. The fixed head still requires applicable
checks, focused review and separate production approval.

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

That primary `release.published` path builds/uploads both
`qintopia-message-sidecar-linux-x86_64-gnu` and
`qintopia-message-sidecar-qiwe-production-linux-x86_64-gnu` in the existing jobs. The
signed request fixes `runtime_artifact_profile=huabaosi-production`; promotion installs
the QiWe artifact as a companion instead of replacing the primary Huabaosi runtime.

A rollback target must have a reviewed complete primary and companion artifact set for
the target commit. Do not use `runtime_artifact_profile` as a global runtime switch.

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
For a staged O->T->R takeover, verify the effective repository and production
environment value immediately before publication: a `false` value would emit a live
full-R request while O is still the active runner. The published Release request is
separate from the later signed O->T and T->R requests and must not be replayed if the
old runner rejects its dry-run target set. After that, setting it to `false` makes
publishing a normal GitHub Release generate a real production deploy request.

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

Systemd applies `EnvironmentFile=` values after `Environment=` values. Release-owned
identity must therefore be injected as fixed `/usr/bin/env KEY=<release-value>`
assignments on `ExecStart` and `ExecStartPre`, after the persistent environment has been
loaded. Do not rely on `Environment=` for `QINTOPIA_DEPLOYED_COMMIT_SHA`, Huabaosi image
release SHA, or Huabaosi Feishu release SHA when the same key may exist in the
persistent environment file. Persistent runtime configuration is not an authoritative
release identity source.

The first release that introduces this runner behavior needs one follow-up approved
`workflow_dispatch` request for the same published SHA after the release has become
`current`: that first promotion is still processed by the prior runner, while the second
request is processed by the new runner and installs the units. Do not bootstrap this by
editing `/etc/systemd/system` or release files on the server.

If the prior runner rejects the new primary artifact contract before the Release can
become `current`, use the explicit default-disabled `legacy_runner_bootstrap` workflow
mode. It must bind the legacy runtime and commit to the latest trusted successful deploy
result, use the target Release's reviewed deploy bundle, select a distinct ancestor
commit as the transition release SHA, and allow only `deploy-bundle` plus
`qintopia-system-services`. Validate with `dry_run=true` before the separately approved
live bootstrap. Then deploy the target Release normally; do not reuse the transition
release for the new runtime or broaden its restart set.

Unknown production-adjacent paths fail closed. If a PR adds a new Agent, skill,
workflow, runtime, MCP adapter, or deploy path without a restart rule, CI must fail
until the package contract and restart target rule are added.

The deploy request prefix is not configurable. It is fixed to `qintopia-agent-os` so the
GitHub workflow, JSON schema, server-side validator, and COS poller share one production
request-pointer contract.

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
- repeatedly process a request referenced by `current.json` after its local state has
  been archived;
- roll back before `current` has been switched;
- report rollback success when `rollback-release.sh` failed;
- deploy a SHA that was not requested explicitly;
- edit files under `.hermes` outside a fixed, reviewed profile transaction with backup,
  smoke, and rollback;
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

Profile activation requests identify the exact reviewed dry-run request. Results retain
the fixed manifest identity (`commit_sha`, `runtime_sha`, `runtime_artifact_profile`,
and `deploy_bundle_sha`), release scope, restart target, smoke phase, and restore
evidence so approval and rollback can be audited independently.

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

## Runner Unit Upgrades

The installed runner unit is a static root-owned file under `/etc/systemd/system`; a
release symlink switch does not update its sandbox. Any later unit change must therefore
use an owner-approved immutable release file, back up the installed unit under the
deploy state directory, run `systemd-analyze verify` before installation, then run
`systemctl daemon-reload` and inspect the effective properties with `systemctl show`.

If verification fails, restore the backed-up unit and reload systemd. Do not edit the
installed unit in place and do not source it from an unverified working tree.

Keep `ProtectSystem=strict` and `ProtectHome=read-only`, and inspect both effective
properties after every runner-unit upgrade. The release installer requires the exact
`/etc/systemd/system` write path for its allowlisted root-unit manifest; never broaden
that exception to `/etc`. Xiaoman legacy cron retirement requires only the fixed
`/home/ubuntu/.hermes/profiles/xiaoman/cron` directory in `ReadWritePaths`; reviewed
Hermes cron apply additionally needs the fixed profile-local wrapper directories
`/home/ubuntu/.hermes/profiles/xiaoman/scripts` and
`/home/ubuntu/.hermes/profiles/erhua/scripts` plus the governed Erhua profile and the
`/home/ubuntu/.local/state/qintopia-agentos/hermes-cron-snapshot` snapshot repo. The
reviewed snapshot timer installer also needs exactly `/home/ubuntu/.config/systemd/user`
so it can install the ubuntu user service and timer. Do not grant write access to the
whole Xiaoman Hermes profile, the whole qintopia-agentos state directory, or the whole
home.

When adding a new non-optional `ReadWritePaths` entry under `/home/ubuntu`, the release
systemd installer must prepare that exact fixed directory before installing the updated
runner unit. If the path is missing when systemd starts the runner, startup can fail
before `ExecStart`, leaving GitHub deploy requests waiting for a result that will never
be uploaded. The Hermes cron snapshot path and ubuntu user systemd unit directory are
prepared as `ubuntu:ubuntu 0700` so both the root deploy-runner apply path and the
ubuntu Hermes snapshot timer share the same server-local git history boundary.
Preparation must reject symlinks in every fixed path component before creating or
changing ownership, not only inspect the final directory.

## Validation

```bash
pnpm deploy:runner:check
pnpm check:light
```
