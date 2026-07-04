# CI/CD Gates

CI/CD is the enforcement layer for the monorepo collaboration model. Production-facing
work moves through git, review, CI, deployment preflight, and a runbook. Server-side
edits and single-file `scp` overwrites are not normal release paths.

## Required Local Check

Run the full repository gate before opening or merging a PR:

```bash
pnpm check
```

The check currently includes:

- formatting and Markdown linting
- registry and manifest validation
- active Agent package validation
- anti-drift policy checks
- secret and runtime-state scanning
- CI-safe deployment preflight
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

## GitHub Actions

The GitHub Actions CI workflow runs on pull requests and pushes to `master`. It uses
Node.js 24 actions, installs Node.js 24, pnpm, Python, Rust 1.75.0, and the `rustfmt`
component, then runs `pnpm check`.

On `master` pushes, the workflow also builds and uploads the
`qintopia-message-sidecar-linux-x86_64-gnu` artifact. The `check` and `sidecar-artifact`
jobs run in parallel so wall-clock CI time is bounded by the slower job instead of the
sum of both jobs. Deployment must still use only an artifact from a successful workflow
run for the approved commit SHA, which means the paired `check` job has passed for the
same commit.

The artifact contains the release binary, `artifact-manifest.json`, and `SHA256SUMS` for
M9 server verification. The server should download and verify this artifact for an
approved commit SHA, then set the executable bit after checksum verification, instead of
rebuilding the sidecar on the server.

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
