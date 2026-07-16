# Huabaosi Feishu Storage Production Config Observation

Date: 2026-07-16

## Observation

The production host `paxon-server` was inspected through SSH without printing secrets.
The active release symlink pointed at
`/home/ubuntu/qintopia-agent-os-releases/a7c9d9cd06cabbf73c5826de816194fe41c691dc`. The
Huabaosi image-generation timer was still `disabled` and `inactive`.

The release-local observation smoke passed when invoked with the immutable sidecar
binary. The guarded activation command then failed closed because the preflight service
reported `adapter_not_configured`, so the timer was not enabled.

## Root Cause

The production sidecar env already contained the image provider configuration and image
production release/database bindings, but it did not select the Feishu-backed storage
backend. Because `QINTOPIA_HUABAOSI_IMAGE_STORAGE_BACKEND` was absent, the adapter used
its default `http-media` backend and reported the missing HTTP media endpoint variables.

That was the wrong storage direction for the production canary. The owner-selected
boundary is the fixed Feishu Base generated-image table, not an HTTP media endpoint.

## Required Runtime Configuration

The reviewed server configuration channel must provide these values before activation:

```text
QINTOPIA_HUABAOSI_IMAGE_STORAGE_BACKEND=feishu-base
QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED=1
QINTOPIA_HUABAOSI_FEISHU_MIRROR_APPROVAL=approved-huabaosi-feishu-artifact-mirror
QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA=<current release sha>
QINTOPIA_HUABAOSI_FEISHU_DATABASE_URL_SHA256=<production database url sha256>
QINTOPIA_HUABAOSI_FEISHU_PROFILE_ENV_PATH=/home/ubuntu/.hermes/profiles/huabaosi/.env
QINTOPIA_HUABAOSI_FEISHU_SCHEMA_VERSION=huabaosi-generated-image-v1
```

The same configuration channel must also provide the Feishu Base token, the exact Base
token allowlist, the generated-image artifact table id, and the exact table id
allowlist. The generated-image table id comes from the owner-provided Feishu URL's
`table` query parameter. Do not commit the live table id or Base token to git, and do
not reuse the legacy poster ledger table.

`QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED=1` is required by the current production binary
for Feishu-backed generated-image storage validation. It does not enable the external
mirror worker timer by itself; systemd activation remains controlled by the dedicated
owner-approved activation script.

## Safety Boundary

No provider call, media upload, Postgres mutation, Feishu write, QiWe send, publication,
or timer enablement occurred during the failed activation attempt. The next activation
attempt should run only after the server-side env is updated and the release-local
preflight succeeds.
