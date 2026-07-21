# Huabaosi Image Generation Staging Evidence

Date: YYYY-MM-DD

Use this template only after the owner-approved isolated Huabaosi staging smoke has
completed and the retained output passes:

```bash
node tools/deploy/check-huabaosi-image-staging-evidence.mjs <huabaosi-staging-evidence-output.txt>
```

This report is sanitized evidence for one staging `generated_image` creation. It is not
a production enablement record and must not add a timer, service, QiWe send, release
publish, or production activation.

## Boundary

- Repository commit SHA:
- Reviewed staging release identifier:
- Packaged sidecar binary SHA-256:
- Staging database URL SHA-256:
- Image request work item UUID:
- Final JPEG `content_hash`:
- Storage backend: `feishu-base`
- Final JPEG dimensions:
- Final JPEG byte size:
- Review status: `pending`
- Rollback owner:
- Rollback action:

## Phase Evidence

| Phase      | Smoke status              | Evidence checker status | External provider call | Feishu Base write | QiWe send |
| ---------- | ------------------------- | ----------------------- | ---------------------- | ----------------- | --------- |
| preflight  | `adapter_config_ready`    |                         | false                  | false             | false     |
| generation | `generated_image_created` |                         | true                   | true              | false     |

## Sanitized Fields

- `database_url_sha256`:
- `sidecar_binary_sha256`:
- `work_item_id`:
- `content_hash`:
- `mime_type`: `image/jpeg`
- `storage_backend`: `feishu-base`
- `width`:
- `height`:
- `byte_size`:
- `review_status`: `pending`
- Complete Huabaosi evidence checker passed: yes/no.

## Follow-Up Decision

- QiWe staging send must wait for manual approval revalidation and combined
  Feishu-to-QiWe bridge evidence: yes/no.
- Reason:
- Required follow-up owner review:
- Confirmed no QiWe send, production timer, service, Release publish, or production
  activation was added by this staging evidence: yes/no.

## Exclusions

Do not record provider endpoint, provider response, API key, token, database URL,
database credentials, Feishu base/table/record ids, attachment URI, file name, file id,
MD5 value, AES key, file size from provider callbacks, raw prompt, raw source material,
message id, raw chat, raw shell output, raw logs, or response body.
