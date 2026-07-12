# Xiaoman PostgreSQL Integration Hardening Record

Date: 2026-07-12

## Scope

PR #78 adds a disposable PostgreSQL 16 plus pgvector CI job for the guarded AgentOS
control-plane apply smoke. The job writes only to its temporary `qintopia_test`
database. It does not deploy, write Feishu, send QiWe messages, or call external
adapters.

## Findings And Resolutions

| Finding                                                                                     | Classification          | Resolution                                                                                        | Prevention                                                                             |
| ------------------------------------------------------------------------------------------- | ----------------------- | ------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| The temporary CI database lacked the `vector` extension required by migrations.             | Test environment defect | Use `pgvector/pgvector:pg16` rather than plain `postgres:16-alpine`.                              | CI contract requires the pgvector PostgreSQL 16 image.                                 |
| The apply smoke read a nonexistent `work_items.approved_artifact_id` column.                | Smoke assertion defect  | Read the persisted UUID from `payload->>'approved_artifact_id'`.                                  | PostgreSQL apply smoke executes this assertion on every runtime-sensitive PR.          |
| Workbench mirror used `FOR UPDATE` after a `LEFT JOIN`, which PostgreSQL rejects.           | Runtime defect          | Lock only `work_items` with `FOR UPDATE OF wi SKIP LOCKED`.                                       | The apply smoke exercises both targeted and queue mirror paths.                        |
| The smoke expected an invalid `completed` workbench status to be recorded before rejection. | Smoke assertion defect  | Assert rejection at event intake and assert that no event row is written.                         | Existing Rust test and PostgreSQL smoke cover the intake boundary.                     |
| The smoke attempted to mirror a visual request after review had completed it.               | Smoke ordering defect   | Mirror while the request is `awaiting_review`, then process review.                               | The smoke now follows the work item state machine.                                     |
| A policy-denied workbench event remained selectable by the background worker.               | Runtime defect          | Exclude recorded events with a matching `denied_by_policy.source_event_id` from worker selection. | The smoke asserts the worker has no remaining processable event after a policy denial. |

## Current Safety Boundary

- A policy denial remains auditable through `denied_by_policy`.
- A denied event is terminal for automatic worker selection; an operator can still
  inspect it and decide on a separate corrective action.
- No change in this record enables Feishu writes, QiWe sends, visual generation, or
  production deployment.

## Validation Evidence

- Rust 1.96 strict Clippy: `cargo clippy --all-targets -- -D warnings`.
- Sidecar unit tests: `cargo test --manifest-path runtime/sidecar/Cargo.toml`.
- Guarded database apply smoke in CI:
  `deploy/sidecar/scripts/operations-control-plane-apply-smoke.sh` with
  `QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE=1` and a disposable PostgreSQL URL.

Do not merge or deploy this series until the PostgreSQL integration job completes
successfully for the final PR SHA.
