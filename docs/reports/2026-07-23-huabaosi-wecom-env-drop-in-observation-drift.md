# Huabaosi WeCom Environment Drop-In Observation Drift

Date: 2026-07-23

## Observed Evidence

The Huabaosi WeCom gateway user service is active with the reviewed command and working
directory. Its only systemd drop-in is the fixed
`/home/ubuntu/.config/systemd/user/hermes-gateway-huabaosi.service.d/env.conf`, which
loads the fixed `/home/ubuntu/.hermes/profiles/huabaosi/.env` with `ignore_errors=no`.

The release-local gateway observation rejected this state before producing its sanitized
report because it required `DropInPaths` to be empty.

## Root Cause

The observation treated every systemd drop-in as an unreviewed command override. The
production Hermes installation uses one reviewed environment-only drop-in, so the check
rejected the standard runtime layout even though the effective command and working
directory still matched the production contract.

## Resolution

- Require exactly the fixed Huabaosi `env.conf` drop-in.
- Require exactly the fixed Huabaosi profile `.env` with `ignore_errors=no`.
- Continue to reject a missing or additional drop-in, another environment file, optional
  environment loading, command drift, and working-directory drift.
- Keep the observation read-only and prevent it from reading or printing `.env`
  contents.

## Validation

- `node tools/deploy/test-huabaosi-wecom-observation.mjs`
- `node tools/deploy/check-deploy-contracts.mjs`
- `node tools/deploy/check-deploy-runner.mjs`
- `npm run lint:md`
- `npm run format:check`
- `git diff --check`

## Remaining Boundary

This change does not restart or reconfigure the gateway, read the environment file, send
a WeCom message, generate an image, write Postgres or Feishu, call QiWe, or enable any
production timer. The corrected observation still requires a release and a read-only
production rerun before its result can be retained as current evidence.
