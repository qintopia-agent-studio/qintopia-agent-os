# Aliang Image Adapter Local Integration Preflight

Date: 2026-07-13

## Scope

Validate the guarded Xiaoman downstream apply smoke locally after changing the
group-message dependency from `poster_brief` to approved `generated_image`.

## Observed Evidence

- A local `qintopia_test` database was created for the disposable smoke.
- `operations-control-plane-apply-smoke.sh` stopped during `migrate`, before any
  work-item or artifact assertion ran.
- The local PostgreSQL installation does not provide the required `vector` extension:

  ```text
  create vector extension; pre-install it if this user cannot create extensions
  extension "vector" is not available
  ```

- OrbStack was started successfully, but pulling the CI-equivalent
  `pgvector/pgvector:pg16` image failed from Docker Hub with `502 Bad Gateway`.

## Root Cause

The AgentOS migration contract requires pgvector. The local PostgreSQL server lacks the
extension, and the CI-equivalent image could not be retrieved from Docker Hub during
this run. This occurred before the changed image-generation or group-message code
executed.

## Resolution And Boundary

- No production server, release, Feishu, QiWe, image provider, or media endpoint was
  contacted.
- The smoke remains guarded by `QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE=1` and CI runs it
  against a fresh `pgvector/pgvector:pg16` service database.
- The local fake provider/media Rust tests cover the network protocol without external
  credentials. The CI PostgreSQL job remains the required database proof for this PR.

## Validation

- `cargo test --manifest-path runtime/sidecar/Cargo.toml image_generation::tests` passes
  locally, including successful and mismatched readback cases.
- `bash -n deploy/sidecar/scripts/operations-control-plane-apply-smoke.sh` passes
  locally.
- Required follow-up before merge: the `xiaoman-postgres-integration` CI job must pass.

## Owner Action

Use a pgvector-enabled disposable database only when a local rerun is needed. Retry the
image pull after Docker Hub is available; do not install extensions or change
configuration on a production database to work around this local preflight limitation.

## 2026-07-14 Bounded-Retry Follow-Up

The same CI image was still absent locally, and the latest pull attempt in this
workspace again returned Docker Hub `Bad Gateway`. The new bounded-retry assertions
therefore did not run against a local database. Rust tests prove provider failure
classification and loopback refusal behavior; the guarded apply smoke now proves the
first recoverable failure requeues for 60 seconds and the third failure becomes
terminal. The final PR SHA must not merge unless `Xiaoman PostgreSQL integration` runs
those assertions successfully on its disposable pgvector service. No production database
or external provider is an acceptable substitute.

## 2026-07-14 JPEG Final-Artifact Follow-Up

The JPEG final-artifact branch again attempted to run the guarded PostgreSQL apply smoke
against a disposable local database. OrbStack reached `Running`, but two consecutive
pulls of the CI-pinned `pgvector/pgvector:pg16` image failed before container creation:

```text
Error response from daemon: Get "https://registry-1.docker.io/v2/": Bad Gateway
```

The local Homebrew PostgreSQL 17 installation still has no pgvector extension, so it is
not an equivalent substitute. No migration or apply-smoke command was run against any
database in this follow-up, and no production database, provider, media endpoint,
Feishu, or QiWe service was contacted.

Local evidence completed after PR #111 merged and the JPEG branch was rebased onto its
merge commit:

- the complete Rust suite passed with 274 tests and no failures;
- strict all-target/all-feature Clippy and Rust format checks passed;
- coverage nextest passed with 41.47% total line coverage and 61.39% line / 68.98%
  region coverage for `image_generation.rs`;
- fake provider/media tests proved provider PNG decoding, deterministic white-background
  JPEG conversion, bounded decoder allocation, JPEG upload metadata, and same-byte
  readback; and
- shell syntax, workflow/deploy contracts, Markdown lint, and repository pre-commit
  passed.

The final PR SHA must keep `Xiaoman PostgreSQL integration` green. That disposable CI
job is the required evidence for the changed approval SQL and JPEG-shaped apply-smoke
fixture; a merge is forbidden if the job is skipped, cancelled, or fails.
