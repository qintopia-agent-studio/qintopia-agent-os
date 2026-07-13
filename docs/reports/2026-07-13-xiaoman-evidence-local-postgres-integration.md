# Xiaoman Evidence Local PostgreSQL Integration

Date: 2026-07-13

## Observed Evidence

The source-grounded Xiaoman evidence change requires the guarded operations apply smoke
against the same pgvector-enabled PostgreSQL baseline used by CI. OrbStack initially
timed out while starting, then reported `Running`. Pulling the required image failed
before a database container was created:

```text
Error response from daemon: Get "https://registry-1.docker.io/v2/": Bad Gateway
```

No apply-smoke SQL or changed evidence-worker code ran against a local database during
this attempt.

## Root Cause

Docker Hub returned a gateway error while the local daemon requested
`pgvector/pgvector:pg16`. This is an external image-registry availability failure, not a
Postgres migration, Rust compilation, or evidence retrieval assertion failure.

## Resolution And Validation Boundary

- The image requirement was not weakened and the smoke was not pointed at production.
- Rust strict Clippy passed.
- The complete sidecar suite passed with 229 tests after allowing existing fake provider
  tests to bind loopback sockets.
- Shell syntax and focused evidence tests passed locally.
- The guarded apply smoke remains required in the PR's `Xiaoman PostgreSQL integration`
  job, which creates a disposable `qintopia_test` database from
  `pgvector/pgvector:pg16`.

The PR must not merge unless that PostgreSQL integration job passes for the final commit
SHA. No Feishu, QiWe, external provider, media endpoint, production database, or server
was contacted.

## Owner Action

Retry the local image pull only after Docker Hub is available. Treat the PR integration
job as the authoritative database proof; do not install pgvector or alter production to
work around this local infrastructure failure.
