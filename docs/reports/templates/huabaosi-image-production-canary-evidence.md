# Huabaosi Image Production Canary Evidence

Date: YYYY-MM-DD

Use this template only after the owner-approved Huabaosi one-shot production canary has
completed and the retained output passes:

```bash
node tools/deploy/check-huabaosi-image-production-canary-evidence.mjs \
  <production-canary-output.txt>
```

This report is sanitized first-record production evidence for one Feishu-backed pending
JPEG. It is not a production-complete record, does not approve the generated image, and
must not include QiWe delivery, publish, timer enablement, mirror write activity, raw
provider responses, or secrets.

## Boundary

- Repository commit SHA:
- Production release SHA:
- Runtime artifact profile: `huabaosi-production`.
- Packaged sidecar binary SHA-256:
- Production database URL SHA-256:
- Release-local binary verified: yes/no.
- Owner-approved sidecar SHA-256 matched: yes/no.
- Owner-approved database URL SHA-256 matched: yes/no.
- Reviewed brief artifact UUID:
- Reviewed brief work item UUID:
- Created image-generation work item UUID:
- Generated-image artifact UUID:
- Final JPEG `content_hash`:
- Storage backend: `feishu-base`
- Final JPEG dimensions:
- Final JPEG byte size:
- Review status: `pending`
- Provider timer enabled during canary: no.
- Provider timer active during canary: no.
- Rollback owner:
- Rollback action:

## Phase Evidence

| Phase            | Required result                          | External provider call | Feishu readback | Feishu write | QiWe send |
| ---------------- | ---------------------------------------- | ---------------------- | --------------- | ------------ | --------- |
| `preflight`      | `adapter_config_ready`                   | false                  | false           | false        | false     |
| `brief_review`   | `review_recorded` for one `poster_brief` | false                  | false           | false        | false     |
| `request_intake` | `image_generation_requests_created`      | false                  | false           | false        | false     |
| `generation`     | `generated_image_created`                | true                   | false           | true         | false     |
| `revalidation`   | `feishu_primary_storage_revalidated`     | true                   | true            | false        | false     |

## Sanitized Fields

- `release_sha`:
- `artifact_profile`: `huabaosi-production`
- `sidecar_binary_sha256`:
- `database_url_sha256`:
- `release_binary_verified`: `true`
- `approved_sidecar_sha256_matched`: `true`
- `approved_database_url_sha256_matched`: `true`
- `brief_artifact_id`:
- `brief_work_item_id`:
- `image_generation_work_item_id`:
- `artifact_id`:
- `content_hash`:
- `mime_type`: `image/jpeg`
- `storage_backend`: `feishu-base`
- `width`:
- `height`:
- `byte_size`:
- `review_status`: `pending`
- `database_writes_executed`: `false`
- `external_calls_executed`: `true`
- `sensitive_fields_redacted`: `true`
- Huabaosi production canary evidence checker passed: yes/no.

## Follow-Up Decision

- First-record canary retained for final Xiaoman completion evidence: yes/no.
- Reason:
- Required owner follow-up before any production-complete claim:
- Confirmed no image approval, mirror apply, publish, QiWe send, or timer enablement was
  executed by this canary: yes/no.

## Exclusions

Do not record provider endpoint, provider response, API key, token, database URL,
database credentials, Feishu base/table/record ids, attachment token, attachment URI,
file name, file id, MD5 value, AES key, file size, media URL, request id, callback event
id, message id, raw shell output, raw logs, or raw chat content.
