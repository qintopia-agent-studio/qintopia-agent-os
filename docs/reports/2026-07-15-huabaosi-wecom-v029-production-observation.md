# Huabaosi WeCom v0.2.9 Production Observation

Date: 2026-07-15

## Observed Evidence

Release `v0.2.9` points to `7553f92b3205dc7e8632894212380630c139a111`, the current
`master` at publication. Deploy Production run `29338173008` completed successfully with
`dry_run=false`. The server result records the same release SHA, previous SHA
`9ab54cd938d08188b3ab980c7b84f8737da26e5b`, passed deploy-runner check, and no rollback.
Read-only verification found `release/current` at the deployed `v0.2.9` release.

The first operator connection used the inventory address directly and failed public-key
selection before any production command ran. The approved workstation's configured SSH
host alias selected the expected identity and allowed the read-only checks to continue.
No key material was read, copied, or changed.

The Huabaosi gateway observation then exited before producing a report. Fixed-scope
checks showed the system service inactive and the user service active. The canary
observation initially received an invalid explicit binary path; the release binary is
under `sidecar/qintopia-message-sidecar`, not `bin/qintopia-message-sidecar`.

After using the immutable release binary directly, the canary observation passed. It
confirmed that the canary service and timer are absent, canary enablement is false, and
the preflight remains fail-closed. No canary send was attempted.

## Root Cause

The production Huabaosi Hermes gateway is a user systemd service, while the phase-2
observation queried the system service and journal scopes. Its fixture test did not
require the `--user` scope, so CI could not detect the production mismatch.

The canary observation preferred an explicit binary or source-tree Cargo execution but
did not discover the immutable release binary beside the deploy bundle. The documented
release command therefore depended on an operator-provided path that was easy to get
wrong.

## Resolution

- Query the Hermes gateway and journal with fixed user scopes.
- Require the user-scope calls in the fixture test and deploy contract checker.
- Discover `sidecar/qintopia-message-sidecar` automatically when the canary observation
  runs from an immutable release.
- Add a release-layout fixture that proves no Cargo source tree is required.
- Keep the configured operator SSH alias as the connection entrypoint; an authentication
  failure is not authorization to inspect or copy private keys.

## Validation

- `node tools/deploy/test-huabaosi-wecom-observation.mjs`
- `node tools/deploy/test-huabaosi-wecom-canary-observation.mjs`
- `node tools/deploy/check-deploy-contracts.mjs`
- `node tools/deploy/check-deploy-runner.mjs`
- `pnpm lint:md`
- `pnpm check`
- `git diff --check`

The disabled canary production observation passed against the `v0.2.9` immutable
sidecar. The fixed gateway smoke and release-layout auto-discovery still require a
post-release production rerun from `release/current`.

## Remaining Boundary

The production route remains `hermes-gateway-huabaosi.service`. This repair does not
restart or reconfigure it, install a canary unit, send WeCom messages, generate images,
write Postgres or Feishu, call QiWe/provider/media endpoints, or modify live Hermes
profile state.

The gateway observation result is still pending deployment of this fix. A real canary
send still requires a separate owner-reviewed staging command, exact test allowlists,
reviewed output, and rollback evidence before PR 6 can begin.
