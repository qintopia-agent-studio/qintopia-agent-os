# Management UI Deployment Wiring

Date: 2026-09-28. This implementation follows the reviewed Draft PR #727 proposal. The
domain owner has created `agentos.qintopia.cn A 122.51.77.220`; DNS resolution alone
does not activate a service. This PR versions the deployment path; it performs no
production installation, certificate request, database grant or account bootstrap.

## Contract

- The existing R workbench owns the complete HTTPS root of `agentos.qintopia.cn`. Nginx
  proxies only to `127.0.0.1:18780`; when the UI is stopped, this host returns 503 and
  never serves the public COS site.
- A dedicated `qintopia-management-ui` system identity and root-only five-key
  environment file isolate the UI. The renderer binds the immutable R binary; the
  release installer installs the unit disabled and stopped, including on T. Activation
  requires separate R approval and verified live tenant, role, HTTPS, administrator and
  drain prerequisites.
- Runner and recovery already own deploy lock FD 9. Their UI calls inherit that
  descriptor, stop the UI before a pointer write, and verify the same invocation's exit,
  journal, process group and listener closure. Unknown outcomes retain the existing hold
  and forbid replay or automatic restart. External root maintenance actions take poller
  lock FD 8 then deploy lock FD 9 without waiting.
- The HTTP challenge site precedes the separate exact-host Certbot certificate. The
  HTTPS site is installed only after certificate and renewal checks. A failed Nginx
  switch restores the prior reviewed site. Decommission leaves certificate material and
  unrelated hosts intact.

## Verification And Production Boundary

2026-09-29 isolated Ubuntu 24.04/systemd 255 validation found that systemd clears
`InvocationID` and `ExecMainCode` from `systemctl show` after a unit stops. The former
same-invocation and exit-code comparison therefore rejects a successful drain. For a
previously running unit, the stop check must correlate the final `UNIT=<unit>` manager
journal entry's completed stop job and `INVOCATION_ID` with the pre-stop ID, verify the
same invocation's successful deactivation entry, and inspect its service journal for an
UNKNOWN code. Missing or conflicting journal evidence retains the hold. The Linux
fixture and mock must exercise this real post-stop state.

The same real unit drained an in-flight request and exited cleanly, but the port check
rejected its short-lived `TIME_WAIT` sockets as an occupied listener. The closure probe
must permit `SO_REUSEADDR` while still rejecting a live bind on `127.0.0.1:18780`;
process, unit, cgroup and HTTPS 503 checks remain required.

Targeted simulated tests check rendered unit isolation, disabled installation, runner
ordering, Linux FD 8/9 locking, same-invocation stop evidence, UNKNOWN rejection and
static route constraints. Run the new Nginx route test explicitly; the existing
`runtime:nginx:check` script does not discover it. The systemd PID1 fixture contains a
drain/invocation probe and recovery-lock probe. Its initial host limitation, and the
initial Nginx and restricted-role gaps, were closed by the isolated validation below.
Real Certbot/ACME, production role and tenant, and Chrome checks remain production
acceptance under a separately approved runbook. No passing local check authorizes merge,
release or activation.

The first `pnpm check:pr:auto` stopped at `deploy:contracts:check`: its existing Space
contract required the literal `failedPromotionEvents !== "quiesce\\npromote"`, while the
updated test initially compared the full Space/UI event sequence. The test now checks
the full UI-before-pointer sequence and separately preserves the original Space ordering
assertion. `node tools/deploy/check-deploy-contracts.mjs` then passed; the broader tier
was rerun after this correction. No checker or CI gate changed.

The next `pnpm check:pr:auto` passed `check:light` but stopped at `test:qiwe` in
`check:runtime`: the macOS system `python3` was 3.9.6, which cannot evaluate a test's
`list[str] | None` annotation and raises `RuntimeError: There is no current event loop`
when this suite constructs `asyncio.Event()`. The repository testing guide requires
Python 3.12. With `/opt/homebrew/opt/python@3.12/libexec/bin` first on `PATH`, the QiWe
suite passed all 326 tests (one Linux-only skip). This was a local interpreter mismatch;
no QiWe code, checker or CI setting changed.

The initial Linux lock and stop simulation passed in the cached container. Docker could
not fetch the Nginx image or Ubuntu apt packages because of network EOF errors, so
actual Nginx HTTP/HTTPS behavior and the Ubuntu systemd PID 1 fixture were unverified at
that point. The later isolated VM results supersede that gap.

The following `pnpm check:pr:auto` again passed `check:light` and QiWe but stopped at
`cargo fmt --check` because the installed Rust 1.96 toolchain was not on this shell's
`PATH`. No Rust source or check configuration changed.

With Python 3.12 and Rust 1.96 both on `PATH`, the next `pnpm check:pr:auto` passed
`check:light`, the 326-test QiWe suite, Rust formatting/checking, the default sidecar
suite (858 passed, three ignored), both sidecar smokes, both QiWe feature-boundary tests
and both Clippy configurations. Its all-feature sidecar run had one intermittent failure
in the unchanged `foundation_socket_restarts_without_replacing_active_listener` test
(`foundation_broker_active`; 872 other tests passed). The exact test passed alone; the
complete all-feature suite then passed with the repository's `RUST_MIN_STACK=33554432`
(873 passed, 208 ignored). A direct run without that stack setting overflowed in an
unrelated operations test and was repeated with the required setting. No sidecar source
or gate was changed.

For the PostgreSQL tier, a fresh owned `qintopia_test` on loopback port 55681 was used;
existing port 5432 belonged to another local cluster and port 55432 to an SSH forward.
The first `pnpm check:pr:postgres` mistakenly supplied the optional
`QINTOPIA_COLLABORATION_TEST_DATABASE_URL` without its matching `..._ENABLE=1`, causing
138 `explicit_test_enable_required` failures in the collaboration collection. After
recreating only this disposable database and using the runner's native
`QINTOPIA_SIDECAR_DATABASE_URL` fallback, the directed PostgreSQL tests, all 159
`person_collaboration` tests and all 24 `resident_welcome` tests passed. The final
`operations-control-plane-apply-smoke.sh` then rejected the random-port URL because its
hash is outside the reviewed allowlist. The existing testing guide documents this local
limitation; the allowlist and CI gate were not changed. The aggregate local PR tier
therefore had no passing final exit code at that stage. CI must verify the fixed
reviewed URL, and release preparation must still perform the real system and browser
acceptance listed above.

PR review identified a lock-proof gap: calling `flock -n` on FD 9 alone can acquire a
previously unlocked descriptor. The helper now opens the same lock file through a
distinct file description and proves an exclusive lock is already held, then verifies FD
9 can reacquire that lock on its inherited description. The Linux mock rejects an FD 9
that is merely open; the updated lock, drain and UNKNOWN simulation passed. The
historical installer fixture uses a fixed `git show`; this repository's CI checks out
complete history (`fetch-depth: 0`), and that fixture is not part of a deployed
artifact. A shallow source export cannot run this repository test without its historical
commit; no CI gate or extra fixture is added in this PR.

After the lock fix, `pnpm check:pr:auto` completed successfully with quick and heavy
Rust tiers: the all-feature suite passed 873 tests (208 ignored). Its PostgreSQL tier
was explicitly skipped because the owned disposable database had been stopped; the
separate PostgreSQL results and fixed-hash smoke limitation above still apply.

The next PR review found that an unknown Host could reach the HTTP redirect if this
vhost became port 80's default server. Both the bootstrap and HTTPS redirect templates
now return 421 for a nonmatching Host before route selection. The Nginx template test
covers both HTTP server blocks, and the runtime contract and deploy bundle checks pass.
The isolated real Nginx result is recorded below; production host behavior remains an
independent acceptance item.

### 2026-09-29 Isolated Linux, Nginx And Database Validation

- A fresh `qintopia-management-ui-pr728` VM ran Ubuntu 24.04.5, systemd 255 and Nginx
  1.24.0. The older `qintopia-recovery-test` VM and its request/result files were not
  used. The full `test-deploy-runner-systemd-linux.mjs` fixture passed its real PID 1
  lock, hold, recovery and invocation probes. The actual rendered management unit also
  passed `systemd-analyze verify` and ran disabled by default under dedicated UID 999,
  its sole group, a root-owned mode-0600 environment file and an exact-release
  executable. Another user could not read the environment file. The disposable VM's
  `/home/ubuntu` initially lacked traversal permission for that UID, so the first start
  failed `203/EXEC`; granting traversal in this VM allowed the test. Production
  traversal must be checked before activation.
- Real `systemctl` clears both `InvocationID` and `ExecMainCode` when this service
  stops. The lifecycle helper now correlates the last successful manager stop job and
  deactivation entry with the pre-stop invocation, and scans that invocation's service
  journal for UNKNOWN. An in-flight simulated HTTPS request completed with 200 while
  `quiesce` returned success; the process and listener closed, `verify-closed` passed,
  and HTTPS returned 503. `TIME_WAIT` sockets initially caused a false port-occupied
  failure; the corrected bind accepts them but still rejected a separate live listener.
- With the actual unit's `TimeoutStopSec=35s` and `SendSIGKILL=no`, a simulated stuck
  drain returned nonzero, recorded `Result=timeout`, and left the process and listener
  alive. `verify-closed` refused continuation. The test process was then killed by PID
  inside the disposable VM. A separate `production_ui_stop_outcome_unknown` journal
  injection made both `quiesce` and `verify-closed` fail even after normal process exit.
- Real Nginx on loopback returned 404 for the HTTP challenge bootstrap root, 308 for the
  final HTTP redirect, 421 for an unrelated HTTP Host and unrelated HTTPS SNI or Host,
  200 through a simulated UI upstream, and 503 with `Cache-Control: no-store` when the
  UI stopped. Forged forwarded headers did not reach the upstream. Using the actual
  `install-https` helper, a simulated certificate/key mismatch made `nginx -t` fail and
  restored the prior HTTP site (404); after restoring the matching simulated key and
  trusting its certificate only in the VM, the helper installed HTTPS and verified 503.
  Certbot renewal was stubbed to succeed solely for this local switch test; no ACME
  request or production certificate was made.
- A separate local PostgreSQL 18.6 instance on `127.0.0.1:55681/qintopia_test` applied
  all 46 migrations. Five lock functions were transferred to a dedicated `NOLOGIN` owner
  with fixed table privileges and Gateway `MAINTAIN`, and their `PUBLIC EXECUTE`
  remained revoked. The UI and original runtime role had explicit execute rights; an
  unrelated role was denied; an inherited UI role could call; and the UI had no
  Person/Gateway UPDATE, Gateway MAINTAIN, identity-schema CREATE or owner-role SET.
  Four row-lock functions returned true on nonempty simulated rows and false on
  mismatches. The fifth function held the Gateway table lock: a competing insert
  received SQLSTATE `55P03`. This verifies the post-transfer lock boundary; the earlier
  PR #723 report contains the broader restricted-role HTTP workflow test.

The local probe binary, certificates, account and role names above are simulations, not
production assets. The final reviewed artifact identity and production preflight remain
pending; production credentials, certificate issuance, deployment, activation and Chrome
acceptance were not performed.

On the final lifecycle helper and fixture, `pnpm check:pr:auto` passed the quick tier,
QiWe, both sidecar smokes, the default 858-test sidecar suite, both feature-boundary
tests, Clippy and the all-feature suite (873 passed, 208 ignored). A first run reached
the PostgreSQL tier because the disposable port-55681 instance was still running, but
earlier tests had left simulated Space administrators in that database; the Space
integration test then hit its administrator ceiling. After stopping only that instance
and pinning the auto check to its closed URL, the full quick and heavy Rust tiers passed
with PostgreSQL explicitly skipped. The separate fresh-database lock and ACL probes
above remain the database evidence; neither the contaminated rerun nor the skipped tier
is recorded as a PostgreSQL pass. CI still owns the fixed-URL PostgreSQL tier, and
production acceptance remains separate.

## Rollback

Stop and prove UI closure before an R to T pointer change. Keep the management HTTPS
vhost and return 503 while T runs. A failed promotion restores original pointers and
units under the existing recovery hold; it never reopens the UI. Inspect the original
operation ID and audit/version evidence if a stop reports UNKNOWN. Site retirement is a
separate reviewed root maintenance action after quiescence; it removes only this vhost
and archives its renewal declaration.

## 2026-09-29 v0.3.4 Production Preparation (Read-Only)

This dated snapshot supersedes the earlier statement that R's source identity was
pending. It records preparation, not production approval or a deployed management UI.
The deployment wiring PR (#728) is merged. Release Please PR #725 merged at
`2026-09-29T04:38:24Z`; `master`, tag `v0.3.4` and the draft's `targetCommitish` resolve
to `8173e53795062d4df53fc78f5099325b2e37ce59` (R). Release Please run `36522521842`
succeeded. The Release remains a draft with no GitHub assets. R's runtime and
deploy-bundle COS objects have no evidence from a published Release workflow; their
generation or presence, content digests and production request IDs remain **unverified
and pending**. Empty GitHub Release assets do not prove COS absence. The production
workflow requires the Release tag to equal the then-current `origin/master` HEAD. This
preparation edit is on a separate branch; merging it before the v0.3.4 publication would
advance `master` beyond fixed R and fail that gate. Keep the record reviewable off
`master` until publication, or make a separate explicit decision to regenerate the
Release and R identity.

### Current Evidence And Source Identity

- Public DNS and the origin resolver return `agentos.qintopia.cn A 122.51.77.220`.
  Direct origin HTTP and HTTPS requests return `200`; HTTPS certificate verification
  fails for this hostname. This is not the required stopped management UI `503` route.
  The fixed `/etc/nginx/sites-available/qintopia-management-ui.conf` and
  `/etc/nginx/sites-enabled/qintopia-management-ui.conf` paths, separate certificate and
  renewal declaration are absent; the active Nginx config has no exact-host block.
- Production `current=16e8d56b98001579c6288ba13199b80d6d3dfc74` (O) and
  `previous=83d694f2c3bc21fd78a73d25da3197379e2a14d5` (P). O's manifest has
  `commit_sha=deploy_bundle_sha=O`, `runtime_sha=previous_sha=P`; its request result is
  `succeeded`. The `v0.3.3` request `deploy-20260926T005852Z-62f403e3c5e6` has a local
  `failed` result and cannot be replayed.
- The ordinary deploy timer is enabled and waiting. The poller briefly ran during
  observation and exited successfully. No recovery hold, hold drop-in, `takeover.json`,
  or claim directory exists. `deploy.lock` exists; `poller.lock` is absent on O. The
  single recovery JSON is an older September 16 snapshot wrapper. The management UI
  account, unit and five-key environment file are absent. No credential values, private
  records or config file contents were read.
- The owner authorized changing only the existing repository variable
  `RELEASE_DEPLOY_DRY_RUN` to `true`; the command owner read it back with
  `updated_at=2026-09-29T04:51:16Z`, and this task independently read `true`. The
  production environment has no same-name override. This closes the earlier automatic
  **live** R request blocker. Publication still starts a workflow and a dry-run request,
  which must be inspected; old O may reject its six targets. A failed request is not
  retried. No Release or production deployment was authorized here.

### One Release, Two Signed Transitions

Use O and P above, `T=70e7984fab92ddab956009585212d0e9729767b5` and R above. T is a
fixed immutable transition directory identity, **not** another tag or Release. The old O
runner assembles T from the verified P runtime, the R deploy bundle and a new signed
O->T request. The R bundle contains the fixed launcher, new poller, recovery helper and
hold drop-in. Until that request has installed and verified T, there is no T-local
recovery executable; if T does not exist, keep the hold and perform read-only O/P,
original request and result analysis.

Publishing the single `v0.3.4` Release triggers `build-release-artifacts`, whose GitHub
Actions artifact is `qintopia-agent-os-release-build`. After protected `request-deploy`
approval, the workflow uploads the R primary runtime, QiWe companion and deploy bundle
to COS. The bundle key family is
`qintopia-agent-os/deploy-bundle/<R>/qintopia-agent-os-deploy-bundle/`. From the
reviewed immutable O tree and approved private COS environment, the existing
`deploy/sidecar/scripts/fetch-cos-artifact.sh --artifact-type deploy-bundle --sha <R> --output-dir /var/lib/qintopia-agent-os-deploy/recovery/staged`
checks the exact manifest commit SHA and `SHA256SUMS` and extracts `payload/`. Before
any `prepare`, record the published run, COS object/expected archive digests and
root-owned staged metadata. The launcher checks the staged inventory and bytes but does
not itself bind the manifest's `commit_sha` to R; the fetch and independent
approved-digest comparison remain mandatory. No R artifact has been verified or staged
for this batch. Recheck the existing P primary and QiWe companion COS objects, O/P
manifests and O's original signed success result before the first live request; the
September 26 verification is historical evidence, not a live preflight substitute.

The following are future command shapes, **not executed in this preparation**. Each
write requires separate production approval, a fresh read of live state and the
published R artifact. Issue workflow requests from reviewed `master`; obtain each signed
request ID from its own run, never from the tag or the failed v0.3.3 request.

```bash
O=16e8d56b98001579c6288ba13199b80d6d3dfc74
P=83d694f2c3bc21fd78a73d25da3197379e2a14d5
T=70e7984fab92ddab956009585212d0e9729767b5
R=8173e53795062d4df53fc78f5099325b2e37ce59
TARGETS=qintopia-system-services,hermes-erhua,hermes-xiaoman,hermes-silaoshi,hermes-huabaosi,hermes-anan

# Inspect the automatic Release dry-run result first; never replay it.
# Signed O->T dry run while the ordinary O poller still runs:
gh workflow run deploy-production.yml --ref master \
  -f commit_sha="$O" -f runtime_sha="$P" -f deploy_bundle_sha="$R" \
  -f release_sha="$T" -f release_scope=deploy-bundle \
  -f restart_targets=qintopia-system-services -f dry_run=true \
  -f rollback_on_smoke_failure=true
```

After its signed `dry_run_succeeded` result, an approved root operator on the origin
uses the existing immutable O fetch script and private COS environment. The approved R
bundle archive digest must come from the published run and COS readback; its value is
pending. An absent staged path is required before this one-time fetch.

```bash
set -euo pipefail
O=16e8d56b98001579c6288ba13199b80d6d3dfc74
R=8173e53795062d4df53fc78f5099325b2e37ce59
STAGED=/var/lib/qintopia-agent-os-deploy/recovery/staged
APPROVED_R_BUNDLE_TAR_SHA256='<verified-published-digest>'
[[ "$APPROVED_R_BUNDLE_TAR_SHA256" =~ ^[0-9a-f]{64}$ ]]
cd "/home/ubuntu/qintopia-agent-os-releases/$O"
test ! -e "$STAGED" && test ! -L "$STAGED"
deploy/sidecar/scripts/fetch-cos-artifact.sh --artifact-type deploy-bundle \
  --sha "$R" --output-dir "$STAGED"
test "$(sha256sum "$STAGED/qintopia-agent-os-deploy-bundle.tar.gz" | cut -d' ' -f1)" = \
  "$APPROVED_R_BUNDLE_TAR_SHA256"
"$STAGED/payload/deploy/runner/run-fixed-takeover-request.sh" prepare
```

Only after `prepare` verifies the hold, the separately approved live workflow request
uses the same tuple with `dry_run=false`. Run it from reviewed `master` on the operator
workstation, then read its actual signed request ID before invoking the fixed consumer
on the origin.

```bash
O=16e8d56b98001579c6288ba13199b80d6d3dfc74
P=83d694f2c3bc21fd78a73d25da3197379e2a14d5
T=70e7984fab92ddab956009585212d0e9729767b5
R=8173e53795062d4df53fc78f5099325b2e37ce59
gh workflow run deploy-production.yml --ref master \
  -f commit_sha="$O" -f runtime_sha="$P" -f deploy_bundle_sha="$R" \
  -f release_sha="$T" -f release_scope=deploy-bundle \
  -f restart_targets=qintopia-system-services -f dry_run=false \
  -f rollback_on_smoke_failure=true
```

On the origin, using only the signed ID from that run:

```bash
STAGED=/var/lib/qintopia-agent-os-deploy/recovery/staged
"$STAGED/payload/deploy/runner/run-fixed-takeover-request.sh" consume \
  '<O-to-T-live-request-id>'
```

Only if signed success exists but finalization was interrupted, inspect that evidence
first and then run this separate, evidence-only command with the original ID:

```bash
STAGED=/var/lib/qintopia-agent-os-deploy/recovery/staged
"$STAGED/payload/deploy/runner/run-fixed-takeover-request.sh" finalize \
  '<O-to-T-live-request-id>'
```

After verified T/O success, run the distinct full-R dry run on the operator workstation.
Its signed result must succeed before a separately approved live request with the same
tuple and another new request ID.

```bash
R=8173e53795062d4df53fc78f5099325b2e37ce59
TARGETS=qintopia-system-services,hermes-erhua,hermes-xiaoman,hermes-silaoshi,hermes-huabaosi,hermes-anan
gh workflow run deploy-production.yml --ref master \
  -f commit_sha="$R" -f runtime_sha="$R" -f deploy_bundle_sha="$R" \
  -f release_sha="$R" -f release_scope=sidecar-runtime,deploy-bundle,hermes-plugins \
  -f restart_targets="$TARGETS" -f dry_run=true \
  -f rollback_on_smoke_failure=true
```

After the full-R dry-run result succeeds and the distinct live request is approved:

```bash
R=8173e53795062d4df53fc78f5099325b2e37ce59
TARGETS=qintopia-system-services,hermes-erhua,hermes-xiaoman,hermes-silaoshi,hermes-huabaosi,hermes-anan
gh workflow run deploy-production.yml --ref master \
  -f commit_sha="$R" -f runtime_sha="$R" -f deploy_bundle_sha="$R" \
  -f release_sha="$R" -f release_scope=sidecar-runtime,deploy-bundle,hermes-plugins \
  -f restart_targets="$TARGETS" -f dry_run=false \
  -f rollback_on_smoke_failure=true
```

The O->T live request is signed by the existing production workflow. The fixed consumer
compares its approved ID to the COS pointer at the actual read and to the parsed
request; `poller.lock`, a durable claim and `deploy.lock` protect consumption. `consume`
normally verifies the signed success result, archived request, T/O pointers, journal and
manifests and **finalizes in the same invocation**. `finalize <original-id>` only
resumes those evidence checks after interruption. It cannot clear a later T->R or R->T
hold. T->R uses a new request ID and a direction-bound journal; unknown result upload or
archive retains isolation and forbids replay.

After R/T success, normal exact-previous rollback to T/O uses a fresh signed
`rollback-production.yml` mixed-mode request: `expected_current_release_tag=v0.3.4`,
`previous_commit_sha=O`, `previous_runtime_sha=P`, `previous_deploy_bundle_sha=R`,
`previous_release_sha=T`, an approved `action_restart_targets_csv`, and `dry_run=true`
before a separately approved live request. The runner binds T's original signed O->T
success to its mixed manifest. Before R->T, quiesce and prove the UI closed; rollback
does not reactivate it. For an interrupted O->T, T->R or R->T operation with verified T
installed, use only the fixed
`/home/ubuntu/qintopia-agent-os-releases/<T>/deploy/runner/recover-release-lineage.sh --request-id <original-request-id>`
entry with its original journal, signed request, COS result absence proof and FD 8 then
FD 9. Upload intent or unknown COS response retains the hold. Never change pointers by
hand.

The normal rollback command shapes below produce two **different** signed request IDs;
they do not invoke maintenance recovery. The action target CSV must be approved for the
actual rollback, even if it equals the six forward targets.

```bash
O=16e8d56b98001579c6288ba13199b80d6d3dfc74
P=83d694f2c3bc21fd78a73d25da3197379e2a14d5
T=70e7984fab92ddab956009585212d0e9729767b5
R=8173e53795062d4df53fc78f5099325b2e37ce59
TARGETS=qintopia-system-services,hermes-erhua,hermes-xiaoman,hermes-silaoshi,hermes-huabaosi,hermes-anan
gh workflow run rollback-production.yml --ref master \
  -f expected_current_release_tag=v0.3.4 \
  -f previous_commit_sha="$O" -f previous_runtime_sha="$P" \
  -f previous_deploy_bundle_sha="$R" -f previous_release_sha="$T" \
  -f action_restart_targets_csv="$TARGETS" -f dry_run=true
```

After its signed dry-run result succeeds and live rollback is separately approved:

```bash
O=16e8d56b98001579c6288ba13199b80d6d3dfc74
P=83d694f2c3bc21fd78a73d25da3197379e2a14d5
T=70e7984fab92ddab956009585212d0e9729767b5
R=8173e53795062d4df53fc78f5099325b2e37ce59
TARGETS=qintopia-system-services,hermes-erhua,hermes-xiaoman,hermes-silaoshi,hermes-huabaosi,hermes-anan
gh workflow run rollback-production.yml --ref master \
  -f expected_current_release_tag=v0.3.4 \
  -f previous_commit_sha="$O" -f previous_runtime_sha="$P" \
  -f previous_deploy_bundle_sha="$R" -f previous_release_sha="$T" \
  -f action_restart_targets_csv="$TARGETS" -f dry_run=false
```

### First Management UI Configuration

The R/T `management-ui-lifecycle.sh` exposes `prepare`, `install-http`, `issue-cert`,
`install-https` and `activate`. The immutable release installer only places a disabled,
stopped unit. These are later, separately authorized production actions:

| Step            | Object and effect                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     | Required check                                                                                                                                                                                                                                |
| --------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `prepare`       | Create the sole-group `qintopia-management-ui` system account (`/nonexistent`, `nologin`) and `/etc/qintopia/collaboration-management-ui.env` as `root:root 0600`, using private stdin. Its five keys are `QINTOPIA_FOUNDATION_PRODUCTION_ENABLE=1`, live `QINTOPIA_FOUNDATION_TENANT`, live `QINTOPIA_FOUNDATION_IDENTITY_NAMESPACE`, restricted `QINTOPIA_FOUNDATION_DATABASE_URL` for login role `qintopia_management_ui`, and `QINTOPIA_COLLABORATION_PUBLIC_ORIGIN=https://agentos.qintopia.cn`. | Confirm actual tenant, namespace, role and release binary traversal. Do not put values or credentials in Git or this report.                                                                                                                  |
| Database        | Apply the versioned five-function migration through the release path, then create the `qintopia_management_ui` restricted login role and dedicated `NOLOGIN` function owner (local validation used `qintopia_management_ui_lock_owner`). Transfer five `qintopia_identity.management_ui_lock_%` owners and grant only fixed lock rights to that owner, including Gateway `MAINTAIN`; grant explicit function `EXECUTE` to the UI and retain runtime `EXECUTE`.                                        | Check effective ACL, fixed `SECURITY DEFINER` search path, no `PUBLIC EXECUTE`, no UI Person/Gateway UPDATE, Gateway MAINTAIN, schema CREATE, owner membership or `SET ROLE`; exercise nonempty restricted-role and concurrent insert probes. |
| `install-http`  | Install the exact-host HTTP challenge site at `/etc/nginx/sites-{available,enabled}/qintopia-management-ui.conf`; root returns 404.                                                                                                                                                                                                                                                                                                                                                                   | No conflicting `server_name`; `nginx -t` and stopped-state check pass.                                                                                                                                                                        |
| `issue-cert`    | Obtain independent Certbot certificate `qintopia-management-ui` for only `agentos.qintopia.cn`, with its renewal declaration.                                                                                                                                                                                                                                                                                                                                                                         | Inspect uncertain issuance before retry; verify exact SAN and validity.                                                                                                                                                                       |
| `install-https` | Verify renewal dry run and switch the site to HTTPS proxy `127.0.0.1:18780`; disabled UI returns 503.                                                                                                                                                                                                                                                                                                                                                                                                 | `nginx -t`, exact SAN, HTTPS 503 and no public-site response.                                                                                                                                                                                 |
| `activate`      | Enable/start `qintopia-agentos-management-ui.service` on the exact current approved full-scope release binary.                                                                                                                                                                                                                                                                                                                                                                                        | Current full-scope release, no hold or claim, dedicated identity, role and stopped-route checks passed.                                                                                                                                       |

**Unresolved call sequence:** `management-ui-lifecycle.sh` takes FD 8 then FD 9 itself.
Its `prepare`, HTTP and certificate modes additionally require an authenticated
request-bound maintenance hold and an exact current immutable release. The fixed O->T
launcher's `prepare` creates a hold before T exists or a request ID is bound; its
successful `consume` binds the request, then automatically finalizes and removes that
hold. The ordinary T->R runner writes a direction-bound journal, but installs a hold
only on failure or unknown closure; a successful T->R poller clears its claim without
leaving a hold. Thus neither successful path provides a reviewed first-setup window. Do
not race the consumer, hand-write a hold or manufacture an uncertain result. The exact
source boundaries are `deploy/runner/management-ui-lifecycle.sh`'s
`check_maintenance_hold` and non-activation mode branch,
`deploy/runner/run-fixed-takeover-request.sh`'s combined `consume|finalize` branch,
`deploy/runner/qintopia-agent-os-deploy-runner`'s T-to-R `write_recovery_journal` and
failure-only `install_recovery_hold` calls, and
`deploy/runner/poll-deploy-requests.sh`'s post-archive claim removal.

## 2026-09-29 Maintenance Patch Implementation (Local Only)

### Minimal Maintenance Patch

The owner agreed to the specific maintenance-gate scope on 2026-09-29. The
`begin <T-to-R-request-id>` and `finish <same-id>` modes are now implemented on this
branch in the existing `management-ui-lifecycle.sh` root entry; they are not executable
behavior in fixed R. The change reuses the request ID as `hold_token`, the existing
recovery hold and systemd drop-in, FD 8 then FD 9, and the immutable current release. It
adds no deploy request type, result schema, workflow, service or general recovery
protocol. One private, root-owned maintenance record is necessary to persist the
original timer enabled state, request/result digests and begin/finish phase across
process death. Do not overload `takeover.json` or forge a recovery claim: those
represent different transactions.

1. `begin` locks FD 8 then FD 9, checks their identities, the exact current R and
   previous T pointers, closed management UI, the immutable R files and manifest, and no
   outstanding claim, foreign hold or later direction journal. It requires T-to-R
   `smoke-passed` journal identity and its recorded `upload_intent` digest. It verifies
   the processed original production request and local `succeeded` result with the
   existing `wait-deploy-result.sh --verify-archived-request`, then matches their
   full-scope R/R/R tuple, six targets, result checks, journal request digest, R/T
   manifests and pointers. The cleared claim is evidence of the poller's result PUT
   readback and archive finalization; a still-present claim, failed result or uncertain
   upload is never converted into a maintenance success.
2. Under those locks, `begin` records and fsyncs the timer's exact enabled/disabled
   state before changing it, disables and stops the timer, proves the ordinary service
   and fixed consumer are stopped, then rechecks the request/claim and pointer evidence.
   It creates the request-bound hold with exclusive creation and fsync, installs and
   verifies the existing drop-in, reloads systemd and proves the timer remains stopped.
   Only after all checks may the operator run `prepare`, HTTP, certificate and HTTPS
   modes. A repeated `begin` for the same ID resumes from the durable record; a
   different ID or ambiguous intermediate state stops for review. A crash before the
   hold/drop-in is complete never authorizes setup, and a crash after it leaves the
   ordinary poller isolated.
3. `finish` takes the same locks and revalidates the record, hold owner, drop-in,
   original signed success, R/T lineage, no claim or later journal, disabled UI and
   exact-host HTTPS `503`. It restores and verifies the recorded timer state while the
   hold still blocks the service, then unlinks only that request's hold as the final
   state change and fsyncs its directory. Repeating `finish` after an uncertain return
   may succeed only if the record, absent hold and restored timer state match. Any
   failed reload, timer restoration, 503 check, changed identity or unknown stop leaves
   the hold and requires readback; it never replays the deploy or enables UI.

The source change is one existing file: `deploy/runner/management-ui-lifecycle.sh`.
Focused regression cases belong in the existing
`tools/deploy/test-deploy-runner-systemd-linux.mjs` fixture. Update
`deploy/runner/README.md`, this report and `docs/operations/production-deploy-runner.md`
for the new operating contract. Relative to approved #728, these are five
already-touched paths and no new tracked path; #728 also touched runner, recovery,
installer, renderer, Nginx and bundle builder, which this proposal does not need to
change. `build-deploy-bundle.mjs` already packages the lifecycle script, result verifier
and hold drop-in. If implementation proves a second source file or production gate
necessary, rescope and review it before editing.

The smallest meaningful verification matrix is: successful signed T-to-R begin,
setup-window hold and stopped timer, then HTTPS 503 finish with both initially enabled
and initially disabled timers; forged or mismatched request/result/journal, changed R/T
pointers, unfinished or newer claim, later journal and foreign hold all fail closed;
kill or injected failure before/after each durable begin and finish boundary must either
resume on the same ID or retain isolation. The targeted container mode
`node tools/deploy/test-deploy-runner-systemd-linux.mjs --management-ui-maintenance`
passed with simulated signed request/results for both originally enabled and disabled
timers. It rejected a mismatched upload digest, unfinished claim, active UI, foreign
hold, later journal, bad result signature, changed pointer, failed HTTPS 503 and failed
timer restoration; an interrupted `preparing` record resumed on the same ID. The
existing `--management-ui-mock` lock/drain test and `pnpm deploy:runner:check` passed. A
separate Ubuntu 24.04/systemd 255 PID 1 probe used the actual hold drop-in and timer: a
held service did not start, timer disable/restore retained its prior state, and service
start succeeded only after removing the hold. The same disposable VM then ran the actual
lifecycle begin/finish entry with simulated signed request/result, generated test
certificate and controlled 503 response; both originally enabled and disabled timers
completed. The first attempt stopped on a stale UNKNOWN journal from the VM's prior UI
test; a new successful simulated UI stop superseded it. The second attempt exposed that
the simulated unit was `static`, not `disabled`; adding its normal `[Install]` section
corrected the fixture. Neither failure relaxed lifecycle checks. The final helper also
requires the signing environment to be root-owned, mode 0600 and singly linked before
sourcing it; the isolated fixture rejects mode 0644.

A later VM run used the repository's `create-deploy-request.mjs` to produce a signed,
schema-validated full-scope R request with simulated credentials. The existing test
file's `--management-ui-maintenance-systemd-producer` mode put it in a simulated local
COS store and ran the actual T runner and poller. T's mixed manifest caused the runner
to write the T-to-R `smoke-passed` direction journal and signed result; the poller wrote
`upload_intent`, read the result back from the simulated store, archived the original
request and removed its claim. The producer emitted mode 0644 request/result files and a
mode 0600 journal in the root-owned mode 0700 state directory. The same run then
executed real PID 1
`begin -> prepare -> install-http -> issue-cert -> install-https -> finish` and all
eight `SIGKILL` recovery boundaries for both initially enabled and disabled timers. The
promotion, installer and smoke commands were fixture stubs; Certbot created only a
VM-local trusted test certificate. COS, ACME, PostgreSQL, production DNS routing and
user-visible login were not exercised. The VM cleanup removed its test units, hold,
certificate and release tree after each run. The directory `fsync` failure injection
also confirmed the timer does not change before the maintenance record is durable. This
is isolated system behavior evidence, not production acceptance. The final Docker
maintenance fixtures for both original timer states, the earlier UI lock/stop fixtures,
`bash -n`, ShellCheck, `node --check`, `pnpm lint:md`, `pnpm format:check` and
`git diff --check` passed. The final `pnpm deploy:runner:check` failed at the existing
NATS ACL wrapper's fixed 20-second timeout; repeating that wrapper reached the same
seventh-case timeout. Directly running
`python3 tools/deploy/test_space_automation_nats_acl_preflight.py` passed all nine cases
in 21.2 seconds. That direct result isolates the timeout but does not make the aggregate
check pass. The wrapper needs a separately reviewed CI/check change; this five-file
patch does not alter it.

With Python 3.12 and Rust 1.96 on `PATH`, the first subsequent `pnpm check:pr:auto`
passed its quick and heavy Rust tiers but failed in the PostgreSQL Space configuration
test: the existing local `127.0.0.1:5432/qintopia_test` had more active simulated
administrators than the supported ceiling. The checker's default URL reached that
existing cluster; it was not reset or cleaned. A final rerun pinned
`QINTOPIA_SIDECAR_DATABASE_URL` to the confirmed closed loopback port 55682. It passed
the full quick and heavy Rust tiers, including the QiWe suite, 858 default sidecar tests
(three ignored), both smokes, feature-boundary tests, Clippy and 873 all-feature sidecar
tests (208 ignored). The PostgreSQL tier was explicitly skipped, not passed. Earlier
fresh-database lock and ACL evidence above remains separate; CI still owns the fixed-URL
PostgreSQL tier. No production change or activation occurred.

### Independent Failure Reproduction

The ignored local script `.local-workspace/management-ui-success-no-hold-probe.sh` runs
the **current real** lifecycle helper in disposable `python:3.12-slim`, with root-owned
fixed R metadata, `current=R`, `previous=T`, a simulated successful request/result and
T-to-R journal, and no hold or claim. Command:

```bash
docker run --rm -v "$PWD:/repo:ro" python:3.12-slim \
  bash /repo/.local-workspace/management-ui-success-no-hold-probe.sh
```

At simulated request `deploy-20260929T120000Z-8173e53`, the helper's `prepare` exited
`1`; Python raised `FileNotFoundError` for
`/var/lib/qintopia-agent-os-deploy/recovery/hold` at `hold.lstat()`. The wrapper
asserted that the environment file was not created. The fixture does **not** sign its
simulated records or run the deploy consumer, systemd or COS; it isolates the exact
local rejection that occurs before those services are called. The signature and
end-to-end positive path remain for the proposed code's focused tests.

### CI Nine-Principle Review

1. Actual problem: successful T-to-R clears its claim and lacks a setup hold, so first
   `prepare` fails despite a valid deployed R; the isolated invocation confirms the
   rejection point.
2. Proposed CI delta: **0** new or modified workflows, jobs, steps, check rules,
   dependencies and configuration values. The two lifecycle modes do change the
   production maintenance authorization gate, so they require a separate owner review.
3. Evidence: lifecycle lines 95-153, fixed takeover `consume|finalize`, runner's
   failure-only `install_recovery_hold`, poller's post-archive claim removal, and the
   reproduction above.
4. Complexity: reuse existing signed request/result and drop-in; add only the durable
   state needed to restore the timer after interruption.
5. Existing CI runs runner checks, while the existing real systemd PID 1 fixture is
   invoked explicitly in an isolated Linux VM. The failure is lifecycle behavior, so add
   focused cases to that fixture and existing runner checks without a new CI entry.
6. Limit scope to the five existing paths listed above; no runner, recovery, COS,
   release workflow, Nginx or database changes are proposed.
7. No business-specific CI gate exception or weakened assertion is proposed.
8. No non-deploy task enters production deployment; setup remains an explicit root
   maintenance action after a verified deployment.
9. The request-bound hold, signed-result verifier and timer restoration pattern are
   already used by takeover/recovery. No additional shared framework is justified.

The technical conclusion is **no CI change request**, but **new production maintenance
gate scope relative to #728**. The owner agreed to this specific code scope; CI passing
does not authorize merging, publication or production execution.

### Release Order Consequences

- The first usable management UI requires this maintenance entry. Keep the fixed v0.3.4
  draft unpublished and its tag unchanged. After review, merge the patch through the
  normal protected process and let Release Please produce the next valid draft/version.
  Rebind R and its artifact digests to that new immutable commit; recheck the O-to-T
  bundle and T-to-new-R requests against those identities. Do not move the existing tag
  or edit the old draft target in place.
- Publishing fixed R first triggers the existing automatic **dry-run** request because
  `RELEASE_DEPLOY_DRY_RUN=true`; it also fixes R's bundle without this maintenance
  entry. O-to-T and T-to-R may then deploy the disabled unit, but no supported first
  account/HTTP/certificate/HTTPS setup window exists. The UI stays unavailable until a
  separately reviewed later release and maintenance operation. This is a real feature
  delay, not the default resolution of the first-UI requirement.

Do not publish the old v0.3.4 draft after this patch advances `master`: its tag would no
longer equal `origin/master` HEAD. A reviewed patch may enter `master` for the next
Release Please version. This PR itself does not authorize merging, publication,
deployment, certificate requests or production modification.

After that entry and private inputs are approved, complete account, database, HTTP,
certificate and stopped HTTPS checks, then activate on the approved current full-scope
release. Only then can Chrome verify real login, permission, save and rollback on the
deployed page, followed by approved real business observation. Those checks cannot be
pre-publication gates. For an unknown save/stop outcome, read back the original
operation ID and audit/version evidence; do not replay the save, delete
credentials/certificate or restart the UI.

### Remaining Authorizations And Evidence

1. Review this maintenance patch and use the next valid Release Please draft for the
   first usable UI. Publication and the protected workflow's production step need their
   own authorization. The repository dry-run variable is already `true`; reconfirm its
   effective value and tag-to-HEAD equality for the chosen new version just before
   publication, then inspect the automatic signed dry-run result, including any
   old-runner rejection.
2. Exact chosen release COS object digests, P/O identity recheck and approved root
   staging, followed by distinct signed O->T dry-run/live and T->release dry-run/live
   decisions. Capture each original request, signed result,
   pointer/manifest/installer/smoke evidence, and rollback identities. No digest or live
   result can be filled before its action.
3. A reviewed immutable maintenance-hold entry before first UI setup; separately
   approved private account/DB grant, ACME certificate, exact-host site and activation.
   Check the real tenant and manager identity/grant before activation. Chrome and real
   business acceptance follow activation; any outbound business action needs its own
   authorization.
