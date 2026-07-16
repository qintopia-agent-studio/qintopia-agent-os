# Xiaoman Image-Send Staging Evidence

Date: YYYY-MM-DD

Use this template only after the owner-approved isolated staging run has produced both
Huabaosi generated-image evidence and QiWe image-send evidence, and both sanitized
outputs pass:

```bash
node tools/deploy/check-huabaosi-image-staging-evidence.mjs <huabaosi-staging-evidence-output.txt>
node tools/deploy/check-qiwe-image-staging-evidence.mjs <qiwe-staging-evidence-output.txt>
node tools/deploy/check-xiaoman-image-send-staging-evidence.mjs <huabaosi-staging-evidence-output.txt> <qiwe-staging-evidence-output.txt>
```

This report proves one Xiaoman activity image can move through staging from Huabaosi
final JPEG generation to QiWe upload/callback/send without enabling production QiWe
sending. It is not a production enablement record and must not add a listener, service,
timer, production feature build, Feishu write, release publish, or production
activation.

## Boundary

- Repository commit SHA:
- Reviewed staging release identifier:
- Packaged sidecar binary SHA-256:
- Staging database URL SHA-256:
- Huabaosi image request work item UUID:
- QiWe send-ready work item UUID:
- Final JPEG `content_hash`:
- QiWe `artifact_content_hash`:
- Hash match confirmed by `check-xiaoman-image-send-staging-evidence.mjs`: yes/no.
- Target group allowlist: isolated single group confirmed, identifier not recorded.
- Rollback owner:
- Rollback action:

## Execution Checklist

| Step | Command or evidence         | Required result                                                                           | Passed |
| ---- | --------------------------- | ----------------------------------------------------------------------------------------- | ------ |
| 1    | Huabaosi staging readiness  | `ready_for_staging_preflight`                                                             |        |
| 2    | Huabaosi staging smoke      | one pending `generated_image` with `review_status=pending`                                |        |
| 3    | Huabaosi evidence checker   | `Huabaosi image staging evidence check passed.`                                           |        |
| 4    | QiWe staging readiness      | `ready_for_staging_preflight`                                                             |        |
| 5    | QiWe preflight phase        | `staging_adapter_ready`, no upload, no send                                               |        |
| 6    | QiWe upload phase           | `image_upload_accepted`, `external_upload_requested=true`, `external_send_executed=false` |        |
| 7    | QiWe callback phase         | `image_send_completed`, `external_upload_requested=false`, `external_send_executed=true`  |        |
| 8    | QiWe evidence checker       | `QiWe image-send staging evidence check passed.`                                          |        |
| 9    | Cross-flow evidence checker | `Xiaoman image-send staging evidence check passed.`                                       |        |

## Sanitized Evidence Fields

- Huabaosi `content_hash`:
- Huabaosi `review_status`:
- QiWe `artifact_content_hash`:
- QiWe `callback_credential_schema`:
- QiWe `callback_additional_field_count`:
- QiWe `sidecar_binary_sha256`:
- QiWe `external_upload_requested` in upload phase:
- QiWe `external_send_executed` in callback phase:
- Complete QiWe evidence checker mode passed: yes/no.
- Cross-flow Huabaosi/QiWe hash checker passed: yes/no.

## Production Follow-Up Decision

- QiWe production enablement PR allowed: yes/no.
- Reason:
- Required follow-up owner review:
- Confirmed no production listener, service, timer, feature build, Feishu write, Release
  publish, or production activation was added by this staging evidence: yes/no.

## Exclusions

Do not record QiWe token, GUID, API secret material, target group id, database URL,
database credentials, media URI, filename, file id, MD5 value, AES key, file size,
provider response, request id, callback body, callback event id, message id, raw shell
output, raw logs, or response body.
