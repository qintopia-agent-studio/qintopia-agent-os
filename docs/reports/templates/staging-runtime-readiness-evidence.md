# Staging Runtime Readiness Evidence

Date:

This report is sanitized evidence that the fixed staging runtime is ready for real
Huabaosi image-generation and QiWe image-send staging exercises. It is not production
enablement and does not record secrets, env file contents, raw provider output, media
URLs, callback credentials, group ids, or database URLs.

## Reviewed Inputs

- Reviewed staging release SHA:
- Packaged sidecar binary SHA-256:
- Staging database URL SHA-256:
- Fixed staging env path: `/etc/qintopia/message-sidecar-staging.env`
- Fixed staging release root:
  `/home/ubuntu/qintopia-agent-os-staging-releases/<40-hex-sha>`
- Rollback owner:

## Runtime Evidence

Retain the sanitized `staging_runtime_readiness_evidence=` JSON emitted by:

```bash
QINTOPIA_STAGING_RUNTIME_READINESS_EVIDENCE_ENABLE=1 \
QINTOPIA_STAGING_RUNTIME_RELEASE_SHA='<approved staging release sha>' \
QINTOPIA_STAGING_RUNTIME_SIDECAR_SHA256='<approved staging sidecar binary sha256>' \
QINTOPIA_STAGING_RUNTIME_DATABASE_URL_SHA256='<approved staging database URL sha256>' \
  deploy/sidecar/scripts/staging-runtime-readiness-evidence-smoke.sh
```

Expected status:

- `action_status=ready_for_huabaosi_qiwe_staging_smokes`
- prerequisite observation ready
- Huabaosi staging readiness ready
- QiWe staging readiness ready
- sidecar hash matches the reviewed SHA-256
- staging database URL is represented only by its SHA-256

## Follow-On Evidence

| Step | Evidence Source                                 | Expected Sanitized Status                    | Result |
| ---- | ----------------------------------------------- | -------------------------------------------- | ------ |
| 1    | Huabaosi staging smoke                          | one approved staging image request completed |        |
| 2    | `check-huabaosi-image-staging-evidence.mjs`     | pass                                         |        |
| 3    | QiWe staging preflight                          | ready without claim/send                     |        |
| 4    | QiWe staging upload                             | hashed upload correlation only               |        |
| 5    | QiWe staging callback                           | callback credentials memory-only             |        |
| 6    | `check-xiaoman-image-send-staging-evidence.mjs` | Huabaosi/QiWe image hash match               |        |

## Forbidden Evidence

Do not record:

- env file contents or shell-expanded config;
- database URLs, API keys, provider tokens, Feishu tokens, QiWe token/GUID, callback
  file credentials, media URLs, filenames, MD5 values, group ids, or raw provider
  output;
- raw sidecar stdout beyond the reviewed sanitized evidence JSON;
- service/timer enablement, production Release publishing, Feishu writes, production
  provider calls, or production QiWe sends.
