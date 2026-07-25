# Xiaoman Real Activity Production Evidence

Date: YYYY-MM-DD

Use this template only after a single owner-approved production activity has completed
and the sanitized evidence output passes:

```bash
QINTOPIA_XIAOMAN_REAL_ACTIVITY_PRODUCTION_SIDECAR_SHA256='<approved qiwe production sidecar binary sha256>' \
QINTOPIA_XIAOMAN_REAL_ACTIVITY_PRODUCTION_DATABASE_URL_SHA256='<approved production database URL sha256>' \
qintopia-message-sidecar xiaoman-real-activity-production-evidence \
  --workflow-root-id <completed-xiaoman-activity-root-uuid> > production-evidence-output.txt
node tools/deploy/check-xiaoman-real-activity-production-evidence.mjs <production-evidence-output.txt>
```

This report proves one real Xiaoman activity moved through signal intake, Huabaosi image
generation, human generated-image approval, group-message send-ready, QiWe group image
delivery, and sanitized evidence retention. It is not a substitute for the owner release
decision, rollback readiness, or direct human confirmation that the message arrived in
the intended group.

## Boundary

- Repository commit SHA:
- Production release SHA:
- Runtime artifact profile: `qiwe-production`.
- Packaged sidecar binary SHA-256:
- Release-local binary verified: yes/no.
- Owner-approved sidecar SHA-256 matched: yes/no.
- Production database URL SHA-256:
- Owner-approved database URL SHA-256 matched: yes/no.
- Xiaoman source event signal UUID:
- AgentOS workflow root UUID:
- Image-generation work item UUID:
- Generated-image artifact UUID:
- Send-ready work item UUID:
- Final JPEG `artifact_content_hash`:
- QiWe callback credential schema id:
- Callback additional-field count:
- Target group allowlist: `community_activity_group` alias confirmed, raw group id not
  recorded.
- QiWe group arrival confirmed by human operator: yes/no.
- Sanitized evidence checker passed: yes/no.
- Rollback owner:
- Rollback action:

## Execution Checklist

| Step | Evidence phase                 | Required result                                                                              | Passed |
| ---- | ------------------------------ | -------------------------------------------------------------------------------------------- | ------ |
| 1    | `signal_intake`                | `signal_ingest_submitted`, one Xiaoman activity root created from a real event signal        |        |
| 2    | `image_generation`             | `generated_image_created`, one Feishu-backed 1024x1024 JPEG with `review_status=pending`     |        |
| 3    | `human_approval`               | `generated_image_approved`, Feishu attachment revalidated before approval                    |        |
| 4    | `send_ready`                   | `send_ready_recorded`, `review_policy=human_final_confirmation`, target alias is allowlisted |        |
| 5    | `qiwe_upload`                  | `image_upload_accepted`, async upload requested, no message send yet                         |        |
| 6    | `qiwe_callback_send`           | `image_send_completed`, bounded callback received, exactly one external send executed        |        |
| 7    | `sanitized_evidence_retention` | `sanitized_evidence_retained`, all retained IDs and hashes bind to the same activity chain   |        |
| 8    | Evidence checker               | `Xiaoman real activity production evidence check passed.`                                    |        |

## Sanitized Evidence Fields

- `production_release_sha`:
- `runtime_artifact_profile`: `qiwe-production`
- `sidecar_binary_sha256`:
- `release_binary_verified`: `true`
- `approved_sidecar_sha256_matched`: `true`
- `database_url_sha256`:
- `approved_database_url_sha256_matched`: `true`
- `source_event_signal_id`:
- `workflow_root_id`:
- `generated_image_artifact_id`:
- `send_ready_work_item_id`:
- `artifact_content_hash`:
- `callback_credential_schema`:
- `callback_additional_field_count`:
- `retained_report_schema`: `xiaoman-real-activity-production-evidence-v1`
- `raw_secret_fields_retained`: `false`

## Completion Decision

- Xiaoman production-complete gate satisfied: yes/no.
- Reason:
- Follow-up owner decision:

## Exclusions

Do not record QiWe token, GUID, API secret material, target group id, database URL,
database credentials, media URI, filename, file id, MD5 value, AES key, file size,
Feishu attachment token, provider response, request id, callback body, callback event
id, provider message id, QiWe message id, raw shell output, raw logs, raw chat content,
sender ids, or response body.
