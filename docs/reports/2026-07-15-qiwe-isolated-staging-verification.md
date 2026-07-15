# QiWe Isolated Staging Verification

Date: 2026-07-15

## Current State

The repository contains the reviewed two-phase staging entrypoint
`deploy/sidecar/scripts/qiwe-image-send-staging-smoke.sh` and the disabled-by-default
Hermes callback bridge. Local validation proves the guarded upload/callback command
shape, callback credential redaction, fixed subprocess arguments, exact child
environment allowlist, and staging-feature Rust fake-server send path.

This workstation does not have the owner-approved staging runtime inputs required to
perform a real QiWe send:

- no `/etc/qintopia/message-sidecar-staging.env`;
- no `/home/ubuntu/qintopia-agent-os-staging-releases` release root;
- no approved staging database URL hash;
- no approved `group_message_request` work item id; and
- no trusted live `cmd=20000` callback stream.

Therefore this run did not contact QiWe, open a staging database connection, process a
real callback, or send an image. Treating the fake-sidecar smoke as real staging would
hide the remaining owner gate.

## Evidence

Local fake and unit validation passed:

```text
node tools/deploy/test-qiwe-image-staging-smoke.mjs
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest tests.test_image_callback_bridge -v
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tests -v
cargo test --manifest-path runtime/sidecar/Cargo.toml qiwe_image_send --features qiwe-staging-adapter
```

Results:

- fake two-phase staging smoke passed;
- focused callback bridge tests passed 11/11;
- complete QiWe Python suite passed 171/171;
- staging-feature QiWe Rust selection passed 57/57.

The first callback bridge test attempt from the repository root failed because the test
module imports `image_callback_bridge` relative to `skills/qiwe`. Re-running from
`skills/qiwe` used the package-local test layout and passed.

## Remaining Owner Gate

To complete real isolated staging, run the existing smoke against a reviewed staging
binary and isolated group:

```text
QINTOPIA_QIWE_IMAGE_STAGING_SMOKE_ENABLE=1
QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL=approved-staging-qiwe-image-send
QINTOPIA_QIWE_IMAGE_STAGING_PHASE=upload
QINTOPIA_QIWE_IMAGE_STAGING_ENV_FILE=/etc/qintopia/message-sidecar-staging.env
QINTOPIA_QIWE_IMAGE_STAGING_DATABASE_URL_SHA256=<approved staging database URL sha256>
QINTOPIA_QIWE_IMAGE_STAGING_SIDECAR_SHA256=<approved staging sidecar binary sha256>
QINTOPIA_QIWE_IMAGE_STAGING_WORK_ITEM_ID=<approved send-ready UUID>
deploy/sidecar/scripts/qiwe-image-send-staging-smoke.sh
```

Then stream the trusted callback directly to the callback phase:

```text
trusted-staging-callback-source |
QINTOPIA_QIWE_IMAGE_STAGING_SMOKE_ENABLE=1
QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL=approved-staging-qiwe-image-send
QINTOPIA_QIWE_IMAGE_STAGING_PHASE=callback
QINTOPIA_QIWE_IMAGE_STAGING_ENV_FILE=/etc/qintopia/message-sidecar-staging.env
QINTOPIA_QIWE_IMAGE_STAGING_DATABASE_URL_SHA256=<same approved staging database URL sha256>
QINTOPIA_QIWE_IMAGE_STAGING_SIDECAR_SHA256=<same approved staging sidecar binary sha256>
QINTOPIA_QIWE_IMAGE_STAGING_WORK_ITEM_ID=<same approved send-ready UUID>
deploy/sidecar/scripts/qiwe-image-send-staging-smoke.sh
```

The callback must remain memory-only: no file, environment variable, CLI argument, NATS
event, report, log, or committed artifact may contain callback credentials, request ids,
filenames, file ids, MD5 values, media URLs, group ids, database URLs, tokens, or
provider response bodies.

## Production Boundary

This verification did not enable production or staging runtime configuration, install a
service or timer, change nginx/systemd, write Feishu, call a media provider, contact
QiWe, or send externally. Production remains blocked until the owner-approved isolated
staging run records only sanitized evidence: staging database URL SHA-256, fixed
callback credential schema id, fixed outcome labels, exact release SHA, and rollback
owner/action.
