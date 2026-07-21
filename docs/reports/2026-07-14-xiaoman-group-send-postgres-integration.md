# Xiaoman Group Send-Ready PostgreSQL Integration

Date: 2026-07-14

## Scope

The Xiaoman and Huabaosi internal path reaches a queued `erhua.send_group_message` work
item after generated-image review and final human confirmation. The existing shell apply
smoke covers the full workflow, but the Rust `group_message_send` database path had only
pure validation unit tests.

## Evidence

The local Rust coverage baseline passed 251 of 251 tests and reported:

| Module                  | Line coverage |
| ----------------------- | ------------: |
| `xiaoman_activity.rs`   |        70.36% |
| `image_generation.rs`   |        56.79% |
| `operations.rs`         |        55.80% |
| `group_message_send.rs` |        33.17% |

The first sandboxed coverage run could not bind the local fake provider/media listener
and failed with `Operation not permitted`. Running the same command with local loopback
permission passed all tests. This was an execution sandbox restriction, not an adapter
failure.

## Resolution

Added an ignored Rust PostgreSQL integration test that:

- requires `QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE=1`;
- compiles only with the explicit `postgres-integration-tests` Cargo feature;
- refuses non-loopback URLs and any database name other than `qintopia_test`;
- runs migrations against the disposable database;
- proves an approved generated image records one send-ready event with
  `send_executed=false`;
- proves duplicate apply does not increment attempts or add another event; and
- proves a pending artifact fails the work item closed without send-ready, send, or
  publish events.

The GitHub Actions Xiaoman PostgreSQL job runs this Rust test before the existing full
operations apply smoke. The Rust quality job runs Clippy with `--all-features` so the
guarded integration-test code remains linted even though normal coverage excludes it.

Local OrbStack eventually started after `orb start` timed out, but Docker Hub returned
`Bad Gateway` for `pgvector/pgvector:pg16`. The GHCR fallback then stalled in the Docker
credential helper and was interrupted. Local disposable-Postgres execution therefore
remains unverified in this environment; the required GitHub Actions service job is the
authoritative integration result for this PR.

During local validation, `operations-control-plane-smoke.sh` was first run in parallel
with other Cargo commands. One sidecar invocation produced no JSON, but `run_json`
continued and emitted only Python `JSONDecodeError` traces. A sequential traced run then
passed the complete smoke, proving this was not a control-plane assertion regression.
The smoke helper now captures each command's stderr, stops immediately when the command
fails, and reports the step name when output is invalid JSON. This preserves the actual
failure instead of replacing it with a secondary parser error.

## Validation

- `RUST_MIN_STACK=33554432 cargo llvm-cov nextest --manifest-path runtime/sidecar/Cargo.toml --summary-only`:
  251 of 251 passed; total line coverage remained 40.28%.
- `cargo clippy --manifest-path runtime/sidecar/Cargo.toml --all-targets --all-features -- -D warnings`:
  passed.
- `sh .husky/pre-commit`: passed after formatting.
- Direct fixed Node, Python, Bash, Cargo, policy, secret, deploy, workflow, and agent
  entrypoints from the inspected `check:light` and `check:runtime` scripts: passed.
- `deploy/sidecar/scripts/operations-control-plane-smoke.sh`: passed sequentially.
- Failure injection with `QINTOPIA_SIDECAR_BIN=/usr/bin/false`: failed immediately with
  the named step and no JSON parser traceback.

The feature-gated PostgreSQL test compiled and Clippy passed locally. Its database
execution was not run locally because neither approved registry supplied the pgvector
image; the PR must not merge unless `Xiaoman PostgreSQL integration` passes in GitHub
Actions.

## Production Boundary

This change adds tests and smoke diagnostics only. It does not install a service or
timer, write production Postgres, call QiWe, generate images, upload media, write
Feishu, or publish externally. The real QiWe image-send adapter remains unimplemented
because the current generated image contract provides an HTTPS media URI while
`/msg/sendImage` requires separately issued QiWe file credentials.

## Remaining Boundary

CI proves only disposable database behavior. Real image generation still requires the
owner-approved provider/storage/staging decisions, and real group image sending still
requires an approved QiWe media-upload protocol, allowlist, staged smoke, idempotency
contract, and rollback owner.
