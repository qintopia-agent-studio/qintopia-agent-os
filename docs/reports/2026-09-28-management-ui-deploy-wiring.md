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

Targeted simulated tests check rendered unit isolation, disabled installation, runner
ordering, Linux FD 8/9 locking, same-invocation stop evidence, UNKNOWN rejection and
static route constraints. Run the new Nginx route test explicitly; the existing
`runtime:nginx:check` script does not discover it. The systemd PID1 fixture contains a
drain/invocation probe and recovery-lock probe, but the full fixture could not run on
this host. Actual HTTP/HTTPS Nginx response and site restore, Certbot, database
effective privileges and Chrome checks remain production acceptance under a separately
approved runbook. No passing local check authorizes merge, release or activation.

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

The Linux lock and stop simulation passed in the cached container. Docker could not
fetch the Nginx image or Ubuntu apt packages because of network EOF errors, so actual
Nginx HTTP/HTTPS behavior and the Ubuntu systemd PID 1 fixture are unverified. They
remain release-preparation acceptance items, not local passes.

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

## Rollback

Stop and prove UI closure before an R to T pointer change. Keep the management HTTPS
vhost and return 503 while T runs. A failed promotion restores original pointers and
units under the existing recovery hold; it never reopens the UI. Inspect the original
operation ID and audit/version evidence if a stop reports UNKNOWN. Site retirement is a
separate reviewed root maintenance action after quiescence; it removes only this vhost
and archives its renewal declaration.
