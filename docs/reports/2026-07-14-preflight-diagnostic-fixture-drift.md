# Preflight Diagnostic Fixture Drift

Date: 2026-07-14

## Observed Evidence

PR #114 CI run `29288112137` failed in the `check` job's `Light check` step. The
deploy-runner test reproduced the failure locally:

```text
Error: expected disabled observation to pass for config_valid=0
KeyError: 'missing_configuration'
```

The Rust preflight tests, Xiaoman disposable PostgreSQL integration, PR body check, and
PR-Agent review did not report this failure.

## Root Cause

The production observation smoke was updated to validate the new sanitized
`missing_configuration` field. Its deploy-runner test uses a fake sidecar executable,
and the fake valid and invalid preflight JSON responses still implemented the previous
report shape. The smoke correctly rejected that stale contract.

The pre-commit hook did not run `tools/deploy/check-deploy-runner.mjs`, so its quick
checks could not detect the fixture drift. CI's broader light check did detect it.

## Resolution

Update both fake preflight responses to include the new field. The valid response uses
an empty list. The invalid response uses one fixed public variable name already present
in `runtime/sidecar/.env.example`, preserving the smoke's public-name allowlist check.

Do not make the production smoke tolerate a missing field: that would weaken report
contract validation and could hide drift in a deployed sidecar.

## Validation

Run:

```text
node tools/deploy/test-huabaosi-image-production-observation.mjs
node tools/deploy/check-deploy-runner.mjs
sh .husky/pre-commit
git diff --check
```

The replacement PR workflow must also pass `check`, Rust quality, and Xiaoman disposable
PostgreSQL integration before merge.

## Remaining Boundary

This repair changes only a local fake sidecar fixture and engineering documentation. It
does not enable image generation, access provider or media endpoints, write Postgres,
write Feishu, call QiWe, or send externally.

## Follow-Up Owner Action

The PR owner must wait for the replacement CI run to pass. Future preflight report-shape
changes must update Rust tests, shell assertions, and fake sidecar responses together.
