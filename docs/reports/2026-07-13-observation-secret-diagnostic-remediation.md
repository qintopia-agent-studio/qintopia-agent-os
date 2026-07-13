# Observation Secret Diagnostic Remediation

Date: 2026-07-13

## Scope

Prevent Xiaoman and operations production-observation smokes from repeating a detected
secret in their failure diagnostics.

## Observed Evidence

PR #100 automated review found that `assert_no_sensitive_output` printed the matched
`token` value when a unit, journal, or worker report contained forbidden output. The
matched value can be a database URL, image-provider API key, Feishu token, or QiWe
credential loaded from the server environment.

## Root Cause

The scripts correctly failed when sensitive output was detected, but their diagnostic
message interpolated the same forbidden value. The safety check could therefore repeat
the secret into CI or operator logs while reporting the original leak.

## Resolution

- Replace value-bearing diagnostics in all observation scripts used by the aggregate
  Xiaoman preflight with a fixed `contains forbidden sensitive output` message.
- Keep exact-value matching in memory so the checks still detect configured secret
  values without printing them.
- Extend the deploy contract check to require the redacted diagnostic and reject the
  previous value-bearing form in the new image-request starter observation.

## Validation

- `bash -n deploy/sidecar/scripts/*observation-smoke.sh`
- `node tools/deploy/check-deploy-contracts.mjs`
- `node tools/deploy/check-xiaoman-preflight-readiness.mjs`
- `sh .husky/pre-commit`
- PR #100 CI and automated review after the remediation commit.

## Remaining Boundary

These smokes inspect only bounded unit, journal, and dry-run output. They do not make
arbitrary logs safe to publish, and operators must still avoid copying raw production
output into git or chat.

## Follow-Up: Custom Timer Interval

The updated PR review also found that the new image-request starter observation used a
fixed `2min` default unless the operator set a separate `_EXPECTED` variable. A reviewed
deployment using
`QINTOPIA_XIAOMAN_ACTIVITY_IMAGE_GENERATION_STARTER_TIMER_INTERVAL=5min` could therefore
install the correct unit and still fail observation.

The observation now initializes its expected interval after loading the environment. It
uses the explicit observation `_EXPECTED` override first, then the actual deployment
interval, then the `2min` default. The deploy contract requires this fallback chain so a
future edit cannot silently restore the false failure.
