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
