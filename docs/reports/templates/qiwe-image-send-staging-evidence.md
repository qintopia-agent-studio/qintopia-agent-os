# QiWe Image-Send Staging Evidence

Date: YYYY-MM-DD

Use this template only after the owner-approved isolated staging smoke has completed and
the retained output passes:

```bash
node tools/deploy/check-qiwe-image-staging-evidence.mjs <staging-evidence-output.txt>
```

This report is sanitized evidence for review. It is not a production enablement record
and must not add a listener, service, timer, production feature build, Feishu write, or
release activation.

## Boundary

- Repository commit SHA:
- Reviewed staging release identifier:
- Packaged sidecar binary SHA-256:
- Staging database URL SHA-256:
- Work item UUID:
- Final JPEG `artifact_content_hash`:
- Target group allowlist: isolated single group confirmed, identifier not recorded.
- Rollback owner:
- Rollback action:

## Phase Evidence

| Phase     | Smoke status | Evidence checker status | External upload requested | External send executed |
| --------- | ------------ | ----------------------- | ------------------------- | ---------------------- |
| preflight |              |                         | false                     | false                  |
| upload    |              |                         | true                      | false                  |
| callback  |              |                         | false                     | true                   |

## Sanitized Fields

- `sidecar_binary_sha256`:
- `artifact_content_hash`:
- `callback_credential_schema`:
- `callback_additional_field_count`:
- `external_upload_requested`:
- `external_send_executed`:
- Complete evidence checker mode passed: yes/no.

## Production Follow-Up Decision

- Production enablement PR allowed: yes/no.
- Reason:
- Required follow-up owner review:

## Exclusions

Do not record QiWe token, GUID, API secret material, target group id, database URL,
database credentials, media URI, filename, file id, MD5 value, AES key, file size,
provider response, request id, callback body, callback event id, message id, raw shell
output, raw logs, or response body.
