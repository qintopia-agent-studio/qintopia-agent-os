# Aliang Production Observation Stderr Remediation

Date: 2026-07-13

## Scope

Prevent the Huabaosi image provider disabled-state observation from exposing
image-worker stderr before sensitive-output validation.

## Observed Evidence

PR #101 automated review found that the image worker dry-run redirected and scanned only
stdout. A database or configuration failure could write diagnostics directly to stderr,
and `set -e` could stop the script before any later sensitive-value check.

## Root Cause

The adapter preflight path already captured stdout, stderr, and exit status, but the
worker preview path assumed a successful JSON response and left stderr attached to the
operator terminal.

## Resolution

- Capture worker stdout and stderr into separate temporary files.
- Record the worker exit status without allowing `set -e` to exit early.
- Scan both files for configured secrets and forbidden markers before interpreting the
  status or parsing JSON.
- Emit only a fixed error message when the dry-run fails.
- Extend the fake-sidecar test with a failing worker that writes the configured API key
  to stderr; the observation must fail without repeating that value.

## Validation

- `node tools/deploy/test-huabaosi-image-production-observation.mjs`
- `node tools/deploy/check-deploy-contracts.mjs`
- `sh .husky/pre-commit`
- PR #101 CI and automated review after remediation.

## Remaining Boundary

The observation validates known configured values and forbidden markers. Operators must
still avoid publishing arbitrary production logs, and the script remains read-only with
the provider worker disabled and unscheduled.
