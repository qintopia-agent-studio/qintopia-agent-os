# QiWe Feishu Release Identity Binding

Date: 2026-07-29

## Summary

Release `v0.2.52` deployed successfully for commit
`a7ea0cbb39c43e796e87a8c905a547a278cd378a`. The release corrected the Feishu mirror
candidate filter, the exact QiWe production feature predicate, and the external timer
first-trigger behavior. Post-deploy dry-runs then proved those fixes, but the guarded
QiWe production preflight still failed before timer activation.

No diagnosis step claimed work, wrote Postgres or Feishu, uploaded media, processed a
callback, called QiWe, or sent a group message. The QiWe send timer and Feishu mirror
timer remained disabled.

## Evidence

- Deploy Production run `30426400957` completed successfully for `v0.2.52`, and
  `release/current` resolved to the released commit.
- The primary artifact reported `huabaosi-production` with the reviewed Huabaosi feature
  pair. The companion reported `qiwe-production` with exactly `qiwe-production-adapter`
  and `huabaosi-feishu-mirror-adapter`.
- Huabaosi image-generation activation and its read-only production observation passed
  with a finite future timer trigger.
- The Feishu mirror queue dry-run returned `no_mirrorable_generated_images` with no
  external calls or database writes.
- The QiWe companion queue dry-run returned `image_upload_preview` for one existing
  send-ready work item with upload and send execution both false.
- The Erhua callback bridge observation passed in enabled mode and resolved to the same
  release-local QiWe companion.
- A no-network production preflight with temporary send enablement and only the unit's
  existing deployed-SHA binding reported every public gate ready except
  `config_valid=false`.
- Repeating that preflight with `QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA` bound
  to the same release SHA returned `production_adapter_ready`.

## Root Cause

The QiWe production companion now owns the reviewed Feishu primary-storage delivery
bridge. Its apply preflight therefore validates the Feishu release identity as well as
the QiWe and database gates.

The systemd renderer still passed an empty capability-specific release environment to
the QiWe preflight and worker services. It bound `QINTOPIA_DEPLOYED_COMMIT_SHA` at the
final exec boundary but left `QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA` to the
persistent runtime env file. That value can describe an older release, so the current
immutable companion failed its Feishu delivery configuration check. Existing renderer
tests explicitly forbade the Feishu release variable in QiWe units and preserved the
obsolete ownership assumption.

## Remediation

- Bind `QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA` to `TARGET_SHA` at the final
  `ExecStart` and `ExecStartPre` boundary for the QiWe production preflight and worker.
- Continue to execute the independent `qiwe-production` companion. Do not replace it
  with the primary Huabaosi binary.
- Do not pass `QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_RELEASE_SHA` to QiWe services.
- Update the existing render/install checks to require the Feishu release binding and
  reject vulnerable `Environment=` release identity assignments.
- Keep runtime secrets and owner-controlled enablement in the persistent env file.

## Validation

Run the focused deploy checks, then the repository PR tier:

```bash
deploy/sidecar/scripts/render-systemd-units.sh \
  --target-sha a7ea0cbb39c43e796e87a8c905a547a278cd378a \
  --check
node tools/deploy/test-release-systemd-install.mjs
node tools/deploy/check-release-model.mjs
node tools/deploy/check-deploy-contracts.mjs
node tools/deploy/check-deploy-runner.mjs
pnpm check:pr:auto
```

After the reviewed fix is released, require the installed QiWe units to bind both
release identities to `release/current`. Rerun the release-local production preflight
before changing the persistent send enablement or activating the QiWe timer.

## Remaining Boundary And Rollback

This fix does not enable QiWe sending. Keep
`qintopia-agentos-qiwe-image-send-worker.timer` disabled until the corrected release is
deployed, production preflight passes from the installed unit, and the owner approves
the persistent send enablement. The Feishu mirror timer is not required for the current
Feishu-primary-storage send candidate and remains disabled.

Rollback is to leave both external timers disabled and retain the previous immutable
release pointer. Do not repair the installed unit or `/etc/qintopia/message-sidecar.env`
by hand.
