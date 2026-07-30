# Xiaoman Preflight QiWe Companion Routing

Date: 2026-07-30

## Summary

The release-local Xiaoman aggregate production preflight passes one discovered sidecar
binary to every child observation. That binary is the primary `huabaosi-production`
artifact under `sidecar/`, while the QiWe production observations require the separate
`qiwe-production` companion under `sidecar-profiles/qiwe-production/`.

As a result, the aggregate preflight can fail at the QiWe image-send observation even
when the independently executed QiWe send and callback observations pass against the
reviewed companion. This is a read-only composition defect; it does not indicate that a
worker ran, a timer changed, or an external send failed.

## Root Cause

The aggregate preflight's isolated child environment was introduced before production
releases installed QiWe as a separate companion artifact. The composition continued to
discover only `sidecar/qintopia-message-sidecar` and injected that primary path into all
children through `QINTOPIA_SIDECAR_BIN`.

The later dual-artifact release layout correctly kept Huabaosi and QiWe production
features separate, but the aggregate caller was not updated to route observations by
artifact profile.

## Resolution

- Discover the primary Huabaosi and QiWe companion binaries separately from the fixed
  release-local layout.
- Pass the selected binary to `run_step` explicitly.
- Keep the first nine Xiaoman, operations, and Huabaosi observations on the primary
  binary.
- Route the QiWe image-send and callback-bridge observations to the QiWe companion.
- Preserve `env -i`; each child still receives only the fixed system `PATH`, its enable
  flag, and its selected release-local sidecar path.

The change does not alter workers, timers, systemd units, database state, Feishu,
provider calls, callback processing, QiWe requests, or external send behavior.

## Validation

The production observation contract test constructs a temporary release layout with both
binary profiles, replaces all child observations with recorders, executes the real
aggregate script, and asserts the exact route for every child. Repository deploy,
preflight-readiness, production-evidence, PR-tier, and PR-doctor checks must also pass.

## Release And Acceptance Boundary

Published release `v0.2.57` is immutable and does not contain this repair. Its runtime
state must be accepted with the independent release-local observations that already
select the reviewed binary profile directly. The corrected aggregate composition becomes
available only after this change is merged, released, and deployed.

Do not hot-edit the `v0.2.57` release tree or reinterpret this repair as evidence that a
new real activity completed. A fresh real-activity callback/send remains a separate
production-completion gate.
