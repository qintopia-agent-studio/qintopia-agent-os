# Rust Test Stack Limit On macOS

Date: 2026-07-13

## Symptom

Running the complete sidecar suite with the macOS default test-thread stack aborted at
`xiaoman_activity::tests::read_apply_without_source_reports_configuration_gap` with a
stack overflow. The same test did not report a Rust assertion failure or a database,
Feishu, QiWe, or external-adapter error.

## Cause And Evidence

The test passes in isolation when the thread stack is increased. The complete suite also
passes with `RUST_MIN_STACK=33554432`. This is a local test-runtime stack constraint for
the existing async test path, not an AgentOS runtime or production deployment failure.

## Resolution

Use the following command for complete local sidecar validation on this machine:

```bash
RUST_MIN_STACK=33554432 cargo test --manifest-path runtime/sidecar/Cargo.toml
```

`pnpm test:sidecar` and CI set this value by default. Do not treat a stack overflow as
permission to skip the suite; record it and run the suite with the explicit stack
setting.

## Follow-up

The issue also reproduced in CI under the default test-thread stack. The workflow and
`pnpm test:sidecar` now set `RUST_MIN_STACK=33554432`. This is test-only; do not add it
to production worker or systemd runtime configuration. If it still fails at that limit,
investigate async call depth instead of weakening the test.
