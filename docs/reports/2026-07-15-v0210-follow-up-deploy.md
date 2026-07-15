# v0.2.10 Same-SHA Follow-up Deploy

Date: 2026-07-15

## Current State

Release `v0.2.10` points to `36cd1df9912639911970120c295ef0b917826909`. Its initial
production deploy succeeded, but that deploy was processed by the previous release
runner. A same-SHA follow-up was therefore required to install the Huabaosi production
image-generation systemd units added by the release.

The corrected follow-up deploy completed successfully. The three Huabaosi units are
loaded from the immutable `v0.2.10` release, while the external worker timer remains
disabled and inactive. Real image generation has not started.

## Observed Failure

Workflow run `29396076293` submitted the reviewed release SHA and artifact scope but
narrowed `restart_targets` to `qintopia-system-services`. The server rejected the
request before promotion with:

```text
existing release manifest restart_targets mismatch
```

The result was `failed`, `current` remained on `v0.2.10`, and rollback was not attempted
because no promotion or restart had occurred.

## Root Cause

The existing immutable release manifest records both identity fields and operational
inputs from the initial promotion. Its `restart_targets` are:

```text
qintopia-system-services,hermes-erhua
```

`promote-release.sh` intentionally requires an existing release directory to match the
new request's `release_sha`, `runtime_sha`, `deploy_bundle_sha`, `commit_sha`,
`release_scope`, and `restart_targets`. The production enablement report incorrectly
instructed the follow-up to use only `qintopia-system-services`, making the documented
request incompatible with the immutable manifest.

## Resolution

Workflow run `29396408652` reused the exact existing manifest values:

- release, runtime, deploy-bundle, and commit SHA:
  `36cd1df9912639911970120c295ef0b917826909`;
- release scope: `sidecar-runtime,deploy-bundle,hermes-plugins`;
- restart targets: `qintopia-system-services,hermes-erhua`;
- `dry_run=false`; and
- rollback on smoke failure enabled.

The server returned `succeeded`. Read-only systemd inspection then confirmed:

- `qintopia-agentos-huabaosi-image-generation-preflight.service` is loaded;
- `qintopia-agentos-huabaosi-image-generation-worker.service` is loaded with the fixed
  `run-huabaosi-image-generation-worker --once --apply` command from `v0.2.10`;
- `qintopia-agentos-huabaosi-image-generation-worker.timer` is loaded but remains
  disabled and inactive; and
- the disabled-state production observation smoke passed.

## Production Preflight

The no-network production preflight was started once after unit installation. It failed
closed with `adapter_not_configured`. The report exposed only these public configuration
names:

- `QINTOPIA_HUABAOSI_IMAGE_PROVIDER`;
- `QINTOPIA_HUABAOSI_IMAGE_MODEL`;
- `QINTOPIA_HUABAOSI_IMAGE_API_BASE_URL`;
- `QINTOPIA_HUABAOSI_IMAGE_API_KEY`;
- `QINTOPIA_HUABAOSI_MEDIA_UPLOAD_ENDPOINT`;
- `QINTOPIA_HUABAOSI_MEDIA_PUBLIC_BASE_URL`; and
- `QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS`.

No values, endpoints, credentials, database URL, or media URI were read into git or
printed by the preflight. It opened no network or database connection. The worker timer
remained disabled, so no provider call, media upload, Postgres mutation, image artifact,
Feishu write, QiWe send, or publication occurred.

## Remaining Boundary

Production image generation is not active. The owner must provision the provider and
media configuration plus the exact approval, published release SHA, and production
database URL hash through the reviewed server configuration channel. After the
release-local preflight succeeds, the explicit activation command may enable the timer
for one-item-per-invocation canary processing. The first pending final JPEG must be
reviewed before broadening the timer window.

QiWe delivery and full Xiaoman end-to-end acceptance remain separate later gates.
