# Xiaoman Mutation Local PostgreSQL Integration

Date: 2026-07-14

## Observed Evidence

The Xiaoman event-signal mutation change requires the guarded operations apply smoke
against the CI-equivalent pgvector PostgreSQL image. OrbStack initially timed out while
starting and then reported `Running`. Pulling the required image failed before a
database container was created:

```text
Error response from daemon: Get "https://registry-1.docker.io/v2/": Bad Gateway
```

No migration, mutation SQL, or apply-smoke assertion ran against a local database in
this attempt.

After the focused and full Rust suites passed, a second image inspection confirmed the
image was still absent. A second pull attempt returned the same Docker Hub `Bad Gateway`
response, so no local database container was started.

## Root Cause

Docker Hub returned a gateway error for `pgvector/pgvector:pg16`. This happened before
the changed migration or sidecar code executed and is not evidence that a status/gap
mutation assertion failed.

## Resolution And Validation Boundary

- The pgvector requirement was not weakened.
- No production database, server, Feishu, QiWe, provider, or media endpoint was used.
- The no-database Xiaoman acceptance smoke passed.
- Rust compilation and focused Xiaoman unit tests passed with the documented macOS
  `RUST_MIN_STACK=33554432` setting.
- The PR's `Xiaoman PostgreSQL integration` job must run the guarded apply smoke against
  its disposable `qintopia_test` service for the final commit SHA.

The PR must not merge unless that integration job proves migration application, status
and gap writes, audit rows, replay idempotency, conflicting mutation rejection, and the
no-Feishu boundary.

## Owner Action

Retry the local image pull only after Docker Hub recovers. Do not install pgvector on or
point the smoke at production as a workaround.
