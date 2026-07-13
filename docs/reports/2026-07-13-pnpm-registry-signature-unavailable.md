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

## 2026-07-14 Recurrence

`pnpm check:light` stopped at the same signed package-manager version lookup before any
repository check ran. The command still required `pnpm@10.29.2`, and no
`pmOnFail=ignore` override was used. After re-reading the fixed `check:light` script,
its repository-local Prettier, Markdownlint, Node, Python, and Bash entrypoints were run
directly. All formatting, 191 Python tests, registry, MCP, skills, workflow, runtime,
deploy, CI, agent, policy, secret, preflight, release-model, deploy-runner, and systemd
checks passed. This does not validate pnpm registry availability; CI must still run the
normal package-manager path on the final commit.

The recursive workflow status change reproduced the same failure when running the full
`pnpm check`: the shim could not fetch signed metadata for `pnpm@10.29.2`, and no
repository check started. Signature enforcement remained enabled. The affected fixed
repository entrypoints, sidecar smokes, Rust format/Clippy/tests, and pre-commit checks
were run directly. The final PR still requires the normal GitHub `check` job to pass;
local direct execution is not a substitute for that package-manager gate.

The Xiaoman group send-ready PostgreSQL integration change reproduced the same failure
again on 2026-07-14. Both `pnpm exec prettier` and `pnpm check` waited for the signed
package-manager lookup, then refused to run because `@pnpm/exe`, the macOS arm64
package, and `pnpm@10.29.2` could not be fetched and verified. The repository-local
Prettier binary and the inspected fixed Node, Python, Bash, and Cargo entrypoints were
run directly. Signature enforcement was not bypassed, and the PR still requires the
normal GitHub `check` job.
