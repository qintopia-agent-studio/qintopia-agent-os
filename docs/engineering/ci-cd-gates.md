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
- registry and manifest validation
- active Agent package validation
- anti-drift policy checks
- secret and runtime-state scanning
- CI-safe deployment preflight

The runtime gate adds:

- QiWe package tests
- sidecar Rust tests
- no-credential sidecar smoke checks

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

The GitHub Actions CI workflow runs on pull requests and pushes to `master`. It uses
Node.js 24 actions and always runs `pnpm check:light`.

The CI workflow starts with a `changes` job. Markdown and docs-only changes skip
Python/Rust runtime checks while still completing the required `check` job. Runtime,
deployment script, package, workflow, or configuration changes run `pnpm check:runtime`
after the light gate.

Do not use workflow-level `paths-ignore` for required checks. A skipped workflow can
leave branch protection checks pending. Keep the workflow running and skip only the
heavy steps inside the workflow.

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

The artifact contains the release binary, `artifact-manifest.json`, and `SHA256SUMS` for
M9 server verification. The server should download and verify this artifact for an
approved commit SHA, then set the executable bit after checksum verification, instead of
rebuilding the sidecar on the server.

After upload, artifact jobs prune older GitHub Actions artifacts with the same artifact
name and keep only the latest two: the current build and the previous build for
rollback. COS artifact pruning follows the same latest-two policy. Artifact jobs have
`actions: write` only for cleanup; repository CI remains read-only.

Rust dependency caching is intentionally not enabled yet. The sidecar is pinned to Rust
1.75.0 for server compatibility, and the first Rust-specific cache trial produced
post-step metadata cleanup noise against newer registry crates. Keep CI clean first;
revisit Rust caching only with a tested cache command that is compatible with the pinned
toolchain.

Required production-adjacent PR evidence:

- target package or runtime area
- commit SHA or branch under review
- validation command output
- external-send, database-write, runtime-profile, secret, Feishu, QiWe, systemd, and
  nginx impact
- smoke plan
- rollback plan
