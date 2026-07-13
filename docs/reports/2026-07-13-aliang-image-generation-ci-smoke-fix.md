# Aliang Image-Generation CI Smoke Fix

Date: 2026-07-13

## Symptom

PR #93 failed the disposable PostgreSQL integration job at the capability seed
assertion:

```text
expected '4', got '5'
```

## Cause

The image-generation migration correctly added `huabaosi.generate_image_asset`, and the
smoke query was updated to count all five capabilities. Its expected count was
accidentally left at the previous value of four. The failure occurred before activity
work-item processing and did not affect production.

## Resolution

Update the assertion to expect five seeded capabilities. Keep the explicit capability
list, rather than weakening it to an unbounded count, so future capability additions
remain deliberate smoke-contract changes.

## Validation

Run the guarded `operations-control-plane-apply-smoke.sh` in the CI disposable
PostgreSQL service. The local machine could not run it because OrbStack's Docker daemon
was not available; this must not be treated as a substitute for the CI database gate.
