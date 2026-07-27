# Xiaoman Staging Database Hash Drift

Date: 2026-07-27

## Finding

The provisioned staging runtime used the reviewed database URL hash
`be30c6feef7c9ea5e5d1916a88dd0277f885d8eb56ed0ade88864bc368a97502`, while the
`v0.2.36` Huabaosi staging sidecar contained only the earlier reviewed hash
`c6dc2730b2a3fdabf05d88e021340b748c5c5b5d06d8ec24b38feef387d39330`.

The staging worker therefore stopped before database or external I/O with
`database URL hash is not in the reviewed allowlist`. No image, Feishu record, or
QiWe request was created by that failed attempt.

## Resolution Boundary

The current owner-reviewed staging hash is added to the staging-only allowlist. The
production database validation and production artifact feature boundary are unchanged.
The old staging hash remains allowlisted to preserve the existing reviewed staging
runtime until it is retired explicitly.

## Validation

- `RUST_MIN_STACK=33554432 cargo test --manifest-path runtime/sidecar/Cargo.toml --features huabaosi-staging-adapter`
- `node tools/deploy/check-deploy-contracts.mjs`
- `node tools/deploy/check-xiaoman-production-evidence-chain-local.mjs`

The code change does not modify server files, enable timers, write Postgres or Feishu,
call an image provider, or send through QiWe.
