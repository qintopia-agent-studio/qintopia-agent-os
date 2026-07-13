# PR Body Edit CI Cancellation

Date: 2026-07-14

## Scope

PR #111 received a follow-up QiWe contract hardening commit. Its first post-fix CI run
appeared as a failed PostgreSQL integration check before any test step executed.

## Evidence

- CI run `29282739241` started at `2026-07-13T20:31:48Z` for commit `f3a8263`.
- The PostgreSQL job completed with conclusion `cancelled` after 23 seconds while
  `Initialize containers` was running. Checkout, Rust setup, integration tests, and the
  apply smoke were all skipped.
- Updating the PR body started a second CI run, `29282772478`, at `2026-07-13T20:32:17Z`
  for the same commit.
- Repository concurrency cancelled the older run when the newer pull-request event was
  accepted. No PostgreSQL assertion or application command failed.

## Resolution

Treat the cancelled run as superseded rather than rerunning its skipped job. The PR body
was validated locally before the final push and will not be edited again during the
replacement CI run. PR #111 must remain unmerged until the latest run passes every
required check.

## Validation

- `gh run list --branch codex/xiaoman-qiwe-image-send-contract`: showed the cancelled
  and replacement CI runs with the same head SHA.
- `gh run view 29282739241 --json jobs`: confirmed cancellation occurred during
  container initialization and every test step was skipped.
- The latest replacement run is the authoritative CI result for commit `f3a8263`.

## Production Boundary

The cancelled job never reached checkout or a test command. The replacement PostgreSQL
job uses only the disposable GitHub Actions `qintopia_test` service. Neither run can
contact production Postgres, QiWe, Feishu, image providers, or production deployment.

## Follow-Up

Complete PR body updates before the final push when practical. If a later body edit
creates a replacement run, inspect run concurrency and wait for the newest run rather
than treating the superseded job as a product failure.
