# Release and deployment contract

These are retained operating constraints, not a new permission grant. Read the sections
relevant to the change before editing. Original conditions and exceptions remain
binding; summaries in AGENTS.md do not replace them.

Backtick paths in root-source rules are repository-relative; paths in Sidecar-source
rules retain their original `runtime/sidecar/` base. New navigation links are relative
to this document.

[Migration inventory](../../../docs/plans/active/agents-guidance/README.md) records the
baseline and source locations.

## root-031

<!-- preserved-rule: root-031 -->

- COSCLI installer defaults must use a versioned official GitHub release asset and the
  matching asset digest, not the floating `coscli-linux-amd64` alias. The alias can move
  before Tencent download docs update their SHA table and break production release
  deploys at install-time checksum verification.

<!-- /preserved-rule: root-031 -->

## root-055

<!-- preserved-rule: root-055 -->

- Deploy-runner production one-shots run from the root service boundary. If a one-shot
  needs ubuntu user systemd, use fixed `/usr/sbin/runuser -u ubuntu` with
  `XDG_RUNTIME_DIR=/run/user/<ubuntu-uid>` and the matching user bus address; direct
  root `systemctl --user` cannot prove the ubuntu user timer boundary.

<!-- /preserved-rule: root-055 -->

## root-057

<!-- preserved-rule: root-057 -->

- After ordinary release promotion, deploy-runner must execute
  `deploy/runner/install-release-systemd-units.sh` and `deploy/runner/smoke-release.sh`
  from the just-promoted release directory, not from the root runner's already-installed
  `RUNNER_DIR`. The root runner may be older than the release it is promoting, so using
  stale runner-local install or smoke logic can block self-bootstrap fixes before they
  reach production.

<!-- /preserved-rule: root-057 -->

## root-065

<!-- preserved-rule: root-065 -->

- Release Please PR manual CI validation:
  `gh workflow run ci.yml --ref <release-please-head-branch> -f release_please_pr_number=<pr-number>`

<!-- /preserved-rule: root-065 -->

## root-066

<!-- preserved-rule: root-066 -->

- Release Please PR required PR-Agent check validation:
  `gh workflow run pr-agent.yml --ref <release-please-head-branch> -f release_please_pr_number=<pr-number>`

<!-- /preserved-rule: root-066 -->

## root-080

<!-- preserved-rule: root-080 -->

- Staging runtime values metadata observation smoke:
  `QINTOPIA_STAGING_RUNTIME_VALUES_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/staging-runtime-values-observation-smoke.sh`

<!-- /preserved-rule: root-080 -->

## root-127

<!-- preserved-rule: root-127 -->

- AgentOS downstream evidence/visual timers observation smoke:
  `QINTOPIA_OPERATIONS_DOWNSTREAM_TIMERS_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/operations-downstream-timers-observation-smoke.sh`

<!-- /preserved-rule: root-127 -->

## root-132

<!-- preserved-rule: root-132 -->

- When adding a new workflow package, update `registry/workflows.yaml`,
  `tools/workflows/check-workflows.mjs`, and `deploy/restart-target-rules.yaml` in the
  same PR. CI treats unmatched `workflows/**` files as production-adjacent.

<!-- /preserved-rule: root-132 -->

## root-136

<!-- preserved-rule: root-136 -->

- CI must install the fixed cargo-nextest and cargo-llvm-cov versions from checksum-
  verified prebuilt releases through `taiki-e/install-action` with Cargo fallback
  disabled. Do not restore per-run `cargo install`; it reintroduces crates.io index
  failures before tests start. Prepare a non-empty diagnostic artifact before tool
  download so an installation failure cannot be obscured by a second missing-artifact
  error.

<!-- /preserved-rule: root-136 -->

## root-137

<!-- preserved-rule: root-137 -->

- Local PR validation is also risk-tiered. Use `pnpm check:pr:auto` before opening an
  ordinary PR; it always runs the quick tier and escalates to heavy Rust checks for
  sidecar, Postgres, deploy script, and CI workflow changes. Use `pnpm check:pr:heavy`
  when you want the full local quick + Rust + disposable PostgreSQL mirror and local
  `qintopia_test` is ready on `127.0.0.1:5432`.

<!-- /preserved-rule: root-137 -->

## root-139

<!-- preserved-rule: root-139 -->

- Document first for new features, behavior changes, migrations, runtime changes, or
  production-adjacent work.

<!-- /preserved-rule: root-139 -->

## root-141

<!-- preserved-rule: root-141 -->

- Do not manually edit root `CHANGELOG.md` in ordinary feature or fix PRs. Release
  Please owns routine release changelog updates from merged Conventional Commits.

<!-- /preserved-rule: root-141 -->

## root-142

<!-- preserved-rule: root-142 -->

- Merging a Release Please PR prepares a version and draft GitHub Release. Manual owner
  publication remains the default production boundary.

<!-- /preserved-rule: root-142 -->

## root-143

<!-- preserved-rule: root-143 -->

- Low-risk classification is a safety boundary for the conversational programming-
  extension runner, not approval to merge or publish. Every generated candidate PR,
  Release Please PR, and draft Release must be reviewed and advanced explicitly by the
  owner.

<!-- /preserved-rule: root-143 -->

## root-146

<!-- preserved-rule: root-146 -->

- If any earlier Release Please version or draft GitHub Release in the current release
  sequence was not published, do not publish the newest version. Stop, reconcile or
  delete the unpublished drafts as an explicit release decision, then regenerate and
  validate a fresh Release Please PR instead of skipping ahead.

<!-- /preserved-rule: root-146 -->

## root-148

<!-- preserved-rule: root-148 -->

- Before publishing a draft GitHub Release, confirm its tag points to current
  `origin/master`. If `master` advanced after the draft was prepared, do not publish or
  retry the stale tag; validate and publish the next Release Please PR instead.

<!-- /preserved-rule: root-148 -->

## root-149

<!-- preserved-rule: root-149 -->

- Production release deploy resolution must not scan unbounded historical Actions logs.
  Keep `deploy-production.yml` release-run lookup bounded to recent completed release
  deploy runs and keep `tools/deploy/collect-release-deploy-results.mjs` enforcing a
  `--max-release-runs` cap before it calls `gh run view --log`.

<!-- /preserved-rule: root-149 -->

## root-150

<!-- preserved-rule: root-150 -->

- Do not merge a Release Please PR unless the draft GitHub Release will be published or
  intentionally deleted in the same release decision. The repository release manifest
  must track the latest published Release tag; deleted draft-only releases must not
  remain as the Release Please baseline.

<!-- /preserved-rule: root-150 -->

## root-151

<!-- preserved-rule: root-151 -->

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

<!-- /preserved-rule: root-151 -->

## root-158

<!-- preserved-rule: root-158 -->

- Do not hot-edit production servers.

<!-- /preserved-rule: root-158 -->

## root-159

<!-- preserved-rule: root-159 -->

- Existing `/etc/qintopia/message-sidecar.env` owner/mode drift must be repaired only by
  reviewed deploy/runner code. The release systemd installer normalizes it to
  `root:ubuntu 0640`; do not run ad-hoc production `chown`/`chmod`.

<!-- /preserved-rule: root-159 -->

## root-162

<!-- preserved-rule: root-162 -->

- Any script expected to exist under `/home/ubuntu/qintopia-agent-os-releases/current`
  after deployment must be included in `tools/deploy/build-deploy-bundle.mjs` and
  guarded by `tools/deploy/check-deploy-contracts.mjs`; adding a repo file alone does
  not put it on the production release root.

<!-- /preserved-rule: root-162 -->

## root-163

<!-- preserved-rule: root-163 -->

- Production COS fetch must leave `artifact-manifest.json`, `SHA256SUMS`, and packaged
  archives mode `0444`, while the sidecar binary remains `0755`. These files are
  immutable non-secret release evidence needed by unprivileged release-local
  observation; mode `0640` can make a valid release unverifiable after root-owned
  promotion.

<!-- /preserved-rule: root-163 -->

## root-164

<!-- preserved-rule: root-164 -->

- Production COS archive extraction runs under the root deploy runner and must use
  `tar --no-same-owner` for both sidecar and deploy-bundle payloads. Never preserve
  GitHub runner numeric owners from an artifact archive or propagate them into the
  immutable production release with `cp -a`; the promoted release tree must remain owned
  by the deploy runner.

<!-- /preserved-rule: root-164 -->

## root-166

<!-- preserved-rule: root-166 -->

- Staging sidecar provisioning runs as the `ubuntu` operator, not root. It must create
  the fixed staging release root, release directory, and sidecar directory with explicit
  mode `0755` independent of ambient `umask`, then freeze the immutable release and
  sidecar directories to `0555`. Failed attempts may remove only paths they created;
  they must not reuse or delete an existing release directory.

<!-- /preserved-rule: root-166 -->

## root-216

<!-- preserved-rule: root-216 -->

- CI must execute non-ignored sidecar tests with all Cargo features so staging-only
  adapter tests actually run. This is test coverage only: ignored PostgreSQL tests
  remain in the disposable integration job. Production artifacts must still use only
  reviewed production features; an all-features CI build must never be promoted or
  treated as a production artifact.

<!-- /preserved-rule: root-216 -->

## root-217

<!-- preserved-rule: root-217 -->

- Heavy PR checks are risk-tiered. Keep `check` meaningful for ordinary PRs, but run
  `rust-quality-baseline` and `postgres-integration` only for sidecar, Postgres, deploy
  sidecar script, or CI workflow changes. Explicit manual dispatches and authenticated
  Release Please validation force the full light, runtime, Rust, and PostgreSQL tiers.
  Do not weaken production deploy or published Release gates; those remain the full
  safety boundary.

<!-- /preserved-rule: root-217 -->

## root-237

<!-- preserved-rule: root-237 -->

- `staging-runtime-prerequisite-observation-smoke.sh` is a read-only observation gate
  for fixed staging env and immutable release prerequisites. It must never read env
  contents, execute the sidecar, connect to Postgres, call external services, install
  units, enable timers, or report secret-bearing values. Its path checks must lstat
  every parent component and reject symlinks, non-directories, group/world-writable
  parents, unexpected parent owners, and a sidecar binary the running user cannot
  execute; tests for these checks must use repository-local temporary roots, not `/tmp`.

<!-- /preserved-rule: root-237 -->

## root-244

<!-- preserved-rule: root-244 -->

- Server Hermes patches under `docs/operations/review-pool/hermes/` are non-deployable
  migration evidence. Do not add them to release bundles or apply them to production;
  migrate each accepted behavior into an owned package with focused tests and a separate
  cutover PR.

<!-- /preserved-rule: root-244 -->

## root-247

<!-- preserved-rule: root-247 -->

- The disposable operations apply smoke may exercise the Huabaosi retry state only when
  both `huabaosi-staging-adapter` and `postgres-integration-tests` are compiled,
  `QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE=1`, the database is exactly `qintopia_test` on
  a literal loopback IP with its approved URL hash, and every provider/media endpoint
  and allowlist host is a literal loopback IP. This exception must never accept an
  external host or production database.

<!-- /preserved-rule: root-247 -->

## root-271

<!-- preserved-rule: root-271 -->

- The first release containing a deploy-runner behavior change is processed by the
  previous runner. Use a reviewed follow-up `workflow_dispatch` request for the same
  published SHA to activate the new runner behavior; do not bootstrap it with server
  edits.

<!-- /preserved-rule: root-271 -->

## root-272

<!-- preserved-rule: root-272 -->

- If the previous runner rejects the new Huabaosi artifact feature contract before
  promotion, the default-disabled `legacy_runner_bootstrap` workflow mode is the only
  allowed bridge. It must bind the legacy runtime to the latest trusted successful
  deploy result, accept only the exact deployed Huabaosi three-feature artifact, use a
  distinct transition release SHA, and restrict scope/restarts to `deploy-bundle` and
  `qintopia-system-services`. Normal fetches must continue to require the current
  three-feature artifact. Run a dry-run before any live bootstrap.

<!-- /preserved-rule: root-272 -->

## root-274

<!-- preserved-rule: root-274 -->

- Deploy result diagnostics may include only bounded non-secret runner facts such as the
  fixed failure stage, numeric exit status, promotion state, and profile activation
  attempt state. Do not upload raw server logs, journal output, env files, secrets,
  external adapter payloads, or command output into COS deploy result JSON.

<!-- /preserved-rule: root-274 -->

## root-275

<!-- preserved-rule: root-275 -->

- If the server poller rejects a malformed deploy request before the runner starts, it
  must still upload a bounded `status=failed` result with
  `checks=[{"name":"deploy-request-validation","status":"failed"}]`. That fallback may
  normalize invalid SHA/profile/scope/restart fields; `wait-deploy-result.sh` must
  accept only this exact validation-failure shape and keep strict identity matching for
  every other deploy result.

<!-- /preserved-rule: root-275 -->

## sidecar-020

<!-- preserved-rule: sidecar-020 -->

- Treat `deploy/sidecar/docs/server-deployment.md` as historical rollback evidence, not
  the current deployment path.

<!-- /preserved-rule: sidecar-020 -->

## sidecar-026

<!-- preserved-rule: sidecar-026 -->

- Group-message send-readiness and policy-denial transitions must release the complete
  claim tuple (`claimed_by`, `locked_at`, and `claim_expires_at`) and require exactly
  one work-item update before appending the corresponding audit event.

<!-- /preserved-rule: sidecar-026 -->

## sidecar-028

<!-- preserved-rule: sidecar-028 -->

- The complete sidecar suite needs a 32 MiB test-thread stack. `pnpm test:sidecar` and
  CI set `RUST_MIN_STACK=33554432`; this is test-only and must not be copied into the
  production sidecar service environment.

<!-- /preserved-rule: sidecar-028 -->

## sidecar-031

<!-- preserved-rule: sidecar-031 -->

- Huabaosi live provider/media execution must compile with exactly one non-default live
  feature: `huabaosi-staging-adapter` or `huabaosi-production-adapter`. A build with
  neither or both must reject apply before Postgres. Staging keeps the exact owner
  phrase and reviewed staging database hash. Production must verify the exact production
  approval phrase, deployed release SHA binding, database URL hash binding, and adapter
  policy before Postgres or external I/O; shell scripts cannot be the only enforcement
  point.

<!-- /preserved-rule: sidecar-031 -->

## sidecar-038

<!-- preserved-rule: sidecar-038 -->

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

<!-- /preserved-rule: sidecar-038 -->

## sidecar-048

<!-- preserved-rule: sidecar-048 -->

- A staging-feature callback apply must validate explicit enablement, API/media/group
  allowlists, and webhook readiness before reading stdin. Upload apply must validate the
  same adapter configuration before connecting to Postgres.

<!-- /preserved-rule: sidecar-048 -->

## sidecar-050

<!-- preserved-rule: sidecar-050 -->

- CI must execute the non-ignored sidecar suite with all features so staging-only
  adapter tests run, then run warning-denied Clippy once with no default features and
  once with all features. The all-feature test/build is CI-only and cannot stand in for
  the production feature set; ignored PostgreSQL tests stay in their disposable
  integration job.

<!-- /preserved-rule: sidecar-050 -->

## sidecar-052

<!-- preserved-rule: sidecar-052 -->

- External adapter modules must use `bounded_http`; do not add another raw socket HTTP
  implementation. Test-only loopback HTTP is allowed, while production clients require
  HTTPS and the reviewed endpoint/host allowlists.

<!-- /preserved-rule: sidecar-052 -->
