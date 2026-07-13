# Pnpm Registry Signature Unavailable

Date: 2026-07-13

## Scope

Record the local deploy-bundle validation interruption while adding the Huabaosi image
provider disabled-state observation.

## Observed Evidence

`pnpm artifact:deploy-bundle` stopped before running the repository script because the
local pnpm version shim could not fetch and verify the registry signature for
`pnpm@10.29.2`. The shim explicitly warned against bypassing the failed version switch
with `pmOnFail=ignore`.

## Root Cause

The package-manager shim could not reach or validate its signed registry metadata. The
repository deploy-bundle builder had not started, so this was not a bundle-content or
contract failure.

## Resolution

- Do not disable pnpm signature enforcement.
- Confirm `package.json` maps `artifact:deploy-bundle` exactly to the repository-local
  `node tools/deploy/build-deploy-bundle.mjs` entrypoint.
- Run that fixed Node entrypoint directly. It built the deploy bundle, manifest, and
  checksum successfully without downloading a package-manager binary.
- Record the fallback rule in `AGENTS.md` so it is not generalized to arbitrary pnpm
  scripts.

## Validation

- `node tools/deploy/build-deploy-bundle.mjs`
- `node tools/deploy/check-deploy-runner.mjs`
- `sh .husky/pre-commit`

## Remaining Boundary

Direct Node execution is acceptable only when the package script has been inspected and
is exactly that repository-local entrypoint. It must not become a general bypass for
lockfile, package-manager, or signature checks.
