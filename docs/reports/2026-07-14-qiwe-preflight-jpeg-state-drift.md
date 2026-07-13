# QiWe Preflight JPEG State Drift

Date: 2026-07-14

## Scope

Run the merged Huabaosi and QiWe no-network preflights after PR #112 resolved the
provider-PNG-to-final-JPEG artifact path.

## Observed Evidence

`qiwe-image-send-preflight` correctly failed with `adapter_not_configured` on a local
machine without staging credentials. Its sanitized limitations still included this
obsolete state:

```text
the current generated-image artifact is PNG and requires a separately reviewed
compatibility decision
```

The same merged revision already produces, uploads, reads back, persists, audits, and
approves only the deterministic final JPEG. The preflight opened no network or database
connection and emitted no configuration values.

## Root Cause

PR #112 updated the implementation, active plans, workflow README, QiWe architecture,
and agent guardrails, but the static runtime limitation in `qiwe_image_send.rs` remained
from the earlier contract-only boundary. Existing preflight tests checked redaction and
enablement behavior but did not assert the artifact-format state.

## Resolution

- Replace the obsolete compatibility warning with the current boundary: the final JPEG
  format is implemented, while owner-approved staging must still verify media readback
  and complete callback credentials.
- Add a Rust assertion that the serialized report contains the final-JPEG state and does
  not contain the obsolete current-PNG claim.
- Keep invalid local configuration fail-closed and keep send enablement disabled.

## Validation

- `cargo fmt --check --manifest-path runtime/sidecar/Cargo.toml`
- `cargo clippy --manifest-path runtime/sidecar/Cargo.toml --all-targets --all-features -- -D warnings`
- `RUST_MIN_STACK=33554432 cargo test --manifest-path runtime/sidecar/Cargo.toml`
- `cargo run --quiet --manifest-path runtime/sidecar/Cargo.toml -- qiwe-image-send-preflight`
- `sh .husky/pre-commit`
- `git diff --check`

The preflight command is expected to exit non-zero until staging configuration exists;
the validated evidence is its sanitized, current-state report.

Result: strict Clippy, 274 Rust tests, Markdown/repository pre-commit, and diff checks
passed. The expected non-zero preflight now reports that the deterministic final JPEG is
required and owner-approved staging must verify isolated media upload and same-byte
readback; it no longer claims that the current artifact is PNG.

## Production Boundary

This repair changes report text and tests only. It does not configure credentials,
enable image generation or QiWe sending, contact Postgres/Feishu/QiWe/media providers,
install a worker or timer, deploy a release, or modify production state.

## Follow-Up

Run the protected Huabaosi staging generation only after the owner records provider,
storage, budget, reviewer, and rollback decisions. QiWe sending remains blocked until a
staging `cmd=20000` callback proves complete file credentials.
