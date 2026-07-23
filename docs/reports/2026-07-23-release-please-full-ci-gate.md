# Release Please Full CI Gate

## Incident

The manual validation for Release Please PR `#261` authenticated the exact bot-owned
head SHA and published `Release Please validation=SUCCESS`, but completed in about 20
seconds. The `Rust quality baseline` and `Xiaoman PostgreSQL integration` jobs were
skipped, and the main `check` job ran only release metadata validation.

This contradicted the repository policy that ordinary pull requests are risk-tiered
while Release Please, pre-publication, and production deployment remain full safety
boundaries. A successful metadata-only status could therefore be mistaken for full
release validation.

## Cause

The change detector did not force the full, Rust, or PostgreSQL outputs for an
authenticated Release Please dispatch. Downstream job and runtime-step conditions also
explicitly excluded Release Please PRs. The commit status was published from the main
check job before the independent heavy jobs could finish.

## Contract

- An authenticated Release Please PR always sets `full-check`, `rust-quality-check`, and
  `postgres-integration-check` to `true`.
- Release metadata, light checks, runtime checks, Rust quality, and disposable
  PostgreSQL integration must all pass on the exact release head.
- A final aggregation job publishes `Release Please validation=SUCCESS` only after the
  main, Rust, and PostgreSQL jobs succeed. Any failed or skipped required job publishes
  failure and fails the aggregation job.
- Release Please merge and draft GitHub Release publication remain explicit owner
  decisions.
- Cargo tool installation disables HTTP/2 multiplexing, retries transient downloads, and
  retains installation logs in the Rust quality artifact when setup fails.

The fix changes CI validation only. It does not merge a Release Please PR, publish a
GitHub Release, deploy production, activate an Hermes profile, or access production
state.
