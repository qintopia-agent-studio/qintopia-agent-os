# Staging Runtime Env Blocker

Date: 2026-07-17 Asia/Shanghai

## Current State

A read-only server continuation checked the fixed staging runtime boundary on
`paxon-server` after the staging sidecar artifact had been provisioned.

The immutable staging sidecar is present and still matches the reviewed artifact:

```text
release_sha=37fff8bf819f0df68825961203e7998b51a07c31
sidecar_sha256=8a04ab44cad0b60cbef499d7a58e0fb8fcac577be537d1418ec3649f38c4fa1f
sidecar_path=/home/ubuntu/qintopia-agent-os-staging-releases/37fff8bf819f0df68825961203e7998b51a07c31/sidecar/qintopia-message-sidecar
```

The fixed staging env file is still absent:

```text
/etc/qintopia/message-sidecar-staging.env: missing
```

## Readiness Evidence

The unified readiness evidence gate was run from the current server release with the
reviewed staging release SHA, sidecar SHA-256, and reviewed staging database URL
SHA-256.

It returned `not_ready` only because the fixed staging env file is missing:

```json
{
  "action_status": "not_ready",
  "limitations": [
    "prerequisite_env_file_path_missing",
    "huabaosi_readiness_env_file_path_missing",
    "qiwe_readiness_env_file_path_missing"
  ],
  "release_sha": "37fff8bf819f0df68825961203e7998b51a07c31",
  "packaged_sidecar_sha256": "8a04ab44cad0b60cbef499d7a58e0fb8fcac577be537d1418ec3649f38c4fa1f",
  "staging_database_url_sha256": "c6dc2730b2a3fdabf05d88e021340b748c5c5b5d06d8ec24b38feef387d39330",
  "success": false
}
```

Each child readiness report saw the release root and sidecar binary as present, secure,
and hash-matching. No env contents were read, no sidecar was executed, and no Postgres,
provider, media, Feishu, QiWe, service, timer, Release, or network action was performed
by the evidence gate.

## Server Config Inventory

A key-presence-only scan of controlled server-local files found:

| Location                                      | Present keys                                                                                  | Missing staging boundary                                                         |
| --------------------------------------------- | --------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| `/etc/qintopia/message-sidecar.env`           | Huabaosi provider, model, API base, API key, media max bytes, production sidecar database URL | reviewed staging DB URL, Feishu Base/table allowlists, Huabaosi profile env path |
| `/home/ubuntu/.hermes/profiles/erhua/.env`    | QiWe API URL, token, GUID                                                                     | isolated staging target group allowlist                                          |
| `/home/ubuntu/.hermes/profiles/huabaosi/.env` | no QiWe staging boundary keys                                                                 | Feishu Base/table allowlists for the generated-image table                       |
| `/home/ubuntu/.hermes/profiles/xiaoman/.env`  | no QiWe staging boundary keys                                                                 | none for Huabaosi image storage; Xiaoman is not the image storage profile        |

The existing production sidecar database URL does not match the reviewed staging
database URL SHA-256, so it must not be reused for staging.

## Required Next Inputs

To move from readiness blocker to real Huabaosi/QiWe staging, provision the fixed
server-local staging env file with owner-reviewed values for:

- `QINTOPIA_SIDECAR_DATABASE_URL`, whose SHA-256 must be
  `c6dc2730b2a3fdabf05d88e021340b748c5c5b5d06d8ec24b38feef387d39330`;
- `QINTOPIA_HUABAOSI_IMAGE_STORAGE_BACKEND=feishu-base`;
- `QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED=1`;
- `QINTOPIA_HUABAOSI_FEISHU_MIRROR_APPROVAL=approved-huabaosi-feishu-artifact-mirror`;
- `QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA` and `QINTOPIA_DEPLOYED_COMMIT_SHA`
  bound to the staged immutable release;
- `QINTOPIA_HUABAOSI_FEISHU_DATABASE_URL_SHA256` matching the same staging database
  hash;
- `QINTOPIA_HUABAOSI_FEISHU_BASE_TOKEN` and
  `QINTOPIA_HUABAOSI_FEISHU_ALLOWED_BASE_TOKENS`;
- `QINTOPIA_HUABAOSI_FEISHU_ARTIFACT_TABLE_ID` and
  `QINTOPIA_HUABAOSI_FEISHU_ALLOWED_ARTIFACT_TABLE_IDS`;
- `QINTOPIA_HUABAOSI_FEISHU_PROFILE_ENV_PATH`;
- `QINTOPIA_HUABAOSI_FEISHU_SCHEMA_VERSION=huabaosi-generated-image-v1`;
- `QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS`;
- `QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS` for exactly one isolated staging group; and
- an explicit owner decision on whether the staging env may reuse the existing Erhua
  QiWe API URL, token, and GUID.

The Huabaosi generated image is stored in the fixed Feishu Base generated-image table.
Do not provision a separate HTTP media upload/public BaseURL for this path.

Do not create a placeholder staging env merely to pass path-only readiness. That would
produce misleading readiness evidence and would still fail the real Huabaosi/QiWe
staging preflight.

## Production Boundary

This continuation did not enable production or staging writes. It did not create or edit
`/etc/qintopia/message-sidecar-staging.env`, publish a Release, install a listener,
enable a timer, call a provider, upload media, write Feishu, call QiWe, or send
externally.
