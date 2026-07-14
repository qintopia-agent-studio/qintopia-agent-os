# Staging Adapter CI Test Execution Gap

Date: 2026-07-14

## Observed Evidence

The `Rust quality baseline` job executes the default-feature sidecar suite through
`cargo llvm-cov nextest` and type-checks all features through warning-denied Clippy. It
does not execute tests with `--all-features`.

QiWe live helpers compile only behind a non-default staging feature, and the reviewed
Huabaosi compile-gate PR introduces the same boundary for image generation. Their
feature-specific tests therefore compile in all-feature Clippy but do not run in the
required CI suite. Local all-feature results are useful review evidence, but they are
not a durable required check and can be omitted accidentally by a later PR.

## Risk

A staging-only state transition, owner gate, fake HTTP round trip, or feature-dependent
assertion can regress while the default suite and both Clippy configurations remain
green. The production binary would still exclude live adapters, but the separately built
staging binary could fail only when an owner-approved staging exercise begins. That is
too late for a path intended to reduce production release churn.

## Resolution

The Rust quality job will execute the complete non-ignored sidecar suite with all Cargo
features after the default coverage run. The command must:

- use the pinned `cargo-nextest` already installed by the job;
- enable all features so Huabaosi and QiWe staging tests execute;
- keep ignored disposable-PostgreSQL tests excluded because the dedicated PostgreSQL job
  owns their database and apply-smoke boundary; and
- fail the job on any test failure.

The repository CI contract will require this exact blocking step and reject options that
run ignored tests in the non-database quality job.

## Production Boundary

This change executes only local Rust tests and loopback fake adapters on GitHub-hosted
runners. It does not build a production artifact with staging features, publish a
release, connect to Postgres, call a real image provider or media service, send through
QiWe, write Feishu, install units, or change runtime configuration.

## Validation

- `cargo nextest run --manifest-path runtime/sidecar/Cargo.toml --all-features --no-fail-fast`
- default and all-feature warning-denied Clippy
- `node tools/ci/check-ci-contracts.mjs`
- `sh .husky/pre-commit`
- the PR-attached `Rust quality baseline` job

## Local Validation Evidence

Rust 1.96.0 and cargo-nextest 0.9.138 matched the versions pinned by CI. The default
suite passed 329 tests. The new all-feature command passed 326 tests and skipped the
eight guarded disposable-PostgreSQL tests. Both warning-denied Clippy configurations
passed.

`sh .husky/pre-commit`, CI/deploy/runtime contracts, Markdown and Prettier checks,
Python package tests, policy checks, secret scanning, deploy preflight, systemd render,
release/current modeling, and deploy-runner checks passed. The local `pnpm check:light`
attempt remained in the package-manager shim without output and was terminated before
any repository script started. After confirming the fixed `package.json` command chain,
each underlying repository-local entrypoint was run directly and passed; no pnpm failure
override was used.
