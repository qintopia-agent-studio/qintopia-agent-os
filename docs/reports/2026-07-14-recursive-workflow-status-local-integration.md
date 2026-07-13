# Recursive Workflow Status Local PostgreSQL Integration

Date: 2026-07-14

## Observed Evidence

The recursive workflow status change needs the guarded operations apply smoke against
the same pgvector PostgreSQL 16 baseline used by CI. `orb start` first returned:

```text
start VM: timed out waiting for VM to start
```

The VM continued starting after that client timeout. A follow-up `orb status` reported
`Running`, and `docker info` reported Docker `29.4.0`. The database image pull then
failed before a container was created:

```text
Error response from daemon: Get "https://registry-1.docker.io/v2/": Bad Gateway
```

No migration, recursive CTE, or guarded apply-smoke assertion ran against a local
database in this attempt.

The first full Rust suite inside the filesystem sandbox passed 241 tests and failed the
three fake provider/media tests before their assertions because loopback
`TcpListener::bind` returned `Operation not permitted`. Re-running the same command
outside that network sandbox passed all 244 tests.

## Root Cause

The OrbStack client timeout did not represent a persistent daemon failure. The blocking
condition was Docker Hub returning a gateway error while fetching
`pgvector/pgvector:pg16`. It occurred before repository code or SQL executed.

## Resolution And Validation Boundary

- The pgvector requirement was not removed or replaced with an incompatible local
  PostgreSQL instance.
- No production database, server, Feishu, QiWe, image provider, media endpoint, or send
  adapter was used.
- Focused Rust tests, formatting, Clippy, workflow checks, deploy contracts, shell
  syntax, and diff checks remain local prerequisites.
- `RUST_MIN_STACK=33554432 cargo test --manifest-path runtime/sidecar/Cargo.toml` passed
  244/244 outside the network sandbox; no external endpoint was contacted.
- The PR's `Xiaoman PostgreSQL integration` job must run the guarded apply smoke against
  its disposable `qintopia_test` service for the final commit SHA.
- That job must prove a nested image-generation request resolves to the top-level
  Xiaoman activity root, preserves its direct visual parent, reports depth two, and is
  included in workflow sync descendant refs.

The PR must not merge unless this integration job passes.

## Owner Action

Retry the local image pull only after Docker Hub recovers. Do not install pgvector on or
point the smoke at production as a workaround.

## Workbench Mirror Follow-Up

The follow-up that exposes recursive status in the `feishu_task_dry_run` workbench
mirror retried the same CI-equivalent image pull. Docker Hub again returned the same
`Bad Gateway` response before a container was created. Focused workbench Rust tests
passed, including the recursive SQL shape and sanitized lineage summary. The final full
Rust suite passed 246/246 outside the network sandbox.

The final PR must still pass `Xiaoman PostgreSQL integration`. Its guarded apply smoke
must prove the mirror keeps three immediate child refs, reports four descendants,
includes the depth-two Huabaosi image-generation request with its direct visual parent,
and writes only a dry-run `human_workbench_refs` row plus audit event. It must not call
Feishu or any external adapter.
