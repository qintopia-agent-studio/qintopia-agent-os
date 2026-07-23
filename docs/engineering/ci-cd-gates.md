# CI/CD Gates

CI/CD is the enforcement layer for the monorepo collaboration model. Production-facing
work moves through git, review, CI, deployment preflight, and a runbook. Server-side
edits and single-file `scp` overwrites are not normal release paths.

## Required Local Check

Run the full repository gate before opening or merging runtime or deployment changes:

```bash
pnpm check
```

For docs-only work, run the light gate:

```bash
pnpm check:light
```

The light gate includes:

- formatting and Markdown linting
- Conventional Commits commit message validation
- registry and manifest validation
- active Agent package validation
- anti-drift policy checks
- secret and runtime-state scanning
- CI-safe deployment preflight

The runtime gate adds:

- QiWe package tests
- sidecar Rust tests
- no-credential sidecar smoke checks
- a Rust coverage baseline artifact and blocking strict Clippy gate
- a Xiaoman downstream apply smoke against a disposable GitHub Actions PostgreSQL
  service

## Secret And Runtime-State Gate

`pnpm secrets:check` blocks committed live secrets and runtime artifacts. It scans
git-visible files and fails on real `.env` files, private keys, credential files,
runtime databases, logs, sessions, caches, request dumps, and high-confidence long
credential assignments.

Example files are allowed only when the values are clearly placeholders, fake values,
test values, or environment variable references.

## Deployment Preflight

`pnpm deploy:preflight:ci` is non-mutating and runs inside `pnpm check`. It verifies
that the repository still has the required deployment policy, cutover plan, package
scripts, and CI gate coverage.

`pnpm deploy:preflight` is the local pre-deploy gate. It additionally requires:

- current branch is `master`
- worktree is clean
- deployment docs and rollback notes are present

This command does not deploy and does not connect to the server. Actual deployment
belongs to a reviewed runbook using an approved commit SHA.

## GitHub Actions CI

The GitHub Actions CI workflow runs on pull requests, pull-request body edits, and
pushes to `master`. It uses Node.js 24 actions and always runs `pnpm check:light`.

The CI workflow starts with a `changes` job. Markdown and docs-only changes skip
Python/Rust runtime checks while still completing the required `check` job. Runtime,
deployment script, package, workflow, or configuration changes run `pnpm check:runtime`
after the light gate.

Heavy checks are risk-tiered separately. Sidecar, Postgres, deploy sidecar script, or CI
workflow changes run `rust-quality-baseline` with Rust 1.96 and
`xiaoman-postgres-integration`; explicit non-Release manual dispatches also force the
heavy tier. Authenticated Release Please dispatches also force light, runtime, Rust, and
PostgreSQL validation on the exact release head. The Rust job stores LCOV, setup logs,
and a text summary as a short-retention artifact. Strict Clippy runs with
`cargo clippy --all-targets -- -D warnings` and blocks the heavy tier. The PostgreSQL
integration uses only a disposable `qintopia_test` service and runs the guarded
control-plane apply smoke with no production database URL, secrets, Feishu, QiWe, or
external adapters.

### Release Please PR Validation

GitHub does not recursively trigger PR workflows when a workflow using `GITHUB_TOKEN`
creates or updates the Release Please PR. A release PR can therefore show no checks even
though the repository CI contract is valid. No checks is not a passing state.

Run CI manually on the release PR's exact head branch:

```bash
gh workflow run ci.yml \
  --ref release-please--branches--master--components--qintopia-agent-os \
  -f release_please_pr_number=<pr-number>
```

The dispatch reads the PR through the GitHub API and fails unless it is open, targets
`master`, is authored by the Release Please bot, contains the generated-body marker, and
its head SHA equals the workflow checkout SHA. Only then does it run the dedicated
manifest/changelog validator and force the complete light, runtime, Rust quality, and
disposable PostgreSQL tiers. Workflow-dispatch check suites are not listed automatically
in the PR rollup, so a final aggregation job publishes a fixed
`Release Please validation` commit status on the verified head SHA only after every
required job succeeds. That status must pass and be visible on the PR before merge. This
validation does not approve publication; merging and publishing remain one owner release
decision.

The same token suppression can omit the ruleset-required `PR-Agent review assistant`
check. In that case run the PR-Agent workflow on the same exact release head:

```bash
gh workflow run pr-agent.yml \
  --ref release-please--branches--master--components--qintopia-agent-os \
  -f release_please_pr_number=<pr-number>
```

The workflow authenticates the open bot-owned PR and exact checkout SHA, then skips the
external PR-Agent action because generated Release Please metadata is not an AI review
target. A successful no-review job provides the required check without changing the PR.

Do not use workflow-level `paths-ignore` for required checks. A skipped workflow can
leave branch protection checks pending. Keep the workflow running and skip only the
heavy steps inside the workflow.

## Commit Message Gate

Commits must follow Conventional Commits. Allowed types are:

```text
build chore ci docs feat fix perf refactor revert style test
```

Use the type that matches the primary change:

- `feat`: new product, package, runtime, or workflow capability
- `fix`: bug fix, broken validation, runtime path issue, or incorrect behavior
- `docs`: documentation-only change
- `ci`: GitHub Actions, CI scripts, or commit/check gates
- `test`: tests or fixtures only
- `refactor`: behavior-preserving code reshaping
- `chore`: repository maintenance without product behavior change
- `build`: dependency, packaging, or artifact build system change

Do not invent ad hoc types. Local commits are checked by the Husky `commit-msg` hook,
and CI runs `pnpm commitlint:check` against the PR commit range.

## Artifact Publication

Artifact publication is opt-in and lives in the `Artifacts` workflow. Use
`workflow_dispatch` to choose:

- `build_sidecar`
- `build_deploy_bundle`
- `upload_cos`

As an explicit automation shortcut, a push to `master` whose head commit message
contains `[publish-artifacts]` publishes both artifact families and uploads to COS.
Normal docs, planning, and repository maintenance commits do not build or upload
artifacts.

Deployment must still use only an artifact from a successful workflow run for the
approved commit SHA, and the paired CI `check` job must have passed for the same commit.

The artifact contains the release binary, compressed bundle, `artifact-manifest.json`,
and `SHA256SUMS` for M9 server verification. The checksum file covers the binary,
bundle, and manifest. The server should download and verify this artifact for an
approved commit SHA, then set the executable bit after checksum verification, instead of
rebuilding the sidecar on the server.

After upload, artifact jobs prune older GitHub Actions artifacts with the same artifact
name and keep the latest ten builds by default. COS artifact pruning follows the same
latest-ten default. Override the counts with `QINTOPIA_ARTIFACT_KEEP_COUNT` and
`QINTOPIA_COS_ARTIFACT_KEEP_COUNT` only as an owner-approved repository variable change.
Artifact jobs have `actions: write` only for cleanup; repository CI remains read-only.

Rust dependency caching is intentionally not enabled yet. The sidecar is pinned to Rust
1.96.0, and the first Rust-specific cache trial produced post-step metadata cleanup
noise against newer registry crates. Keep CI clean first; revisit Rust caching only with
a tested cache command that is compatible with the pinned toolchain. Review Rust version
changes quarterly or alongside a dependency upgrade; do not upgrade the toolchain
opportunistically in unrelated feature work.

Required production-adjacent PR evidence:

- target package or runtime area
- commit SHA or branch under review
- validation command output
- external-send, database-write, runtime-profile, secret, Feishu, QiWe, systemd, and
  nginx impact
- smoke plan
- rollback plan
