# Xiaoman QiWe Group Arrival Confirmation Evidence

Date: YYYY-MM-DD

Use this template only after the real Xiaoman activity production evidence has passed
and a human operator has visually confirmed that the intended QiWe group received the
image message:

```bash
node tools/deploy/check-xiaoman-qiwe-group-arrival-confirmation-evidence.mjs \
  <production-evidence-output.txt> \
  <qiwe-group-arrival-confirmation-output.txt>
```

This report is the human group-arrival boundary. It complements the sanitized adapter
callback/send evidence; it must not include raw QiWe group ids, message ids, callback
payloads, file credentials, URLs, logs, or chat content.

## Confirmation Record

```text
xiaoman_qiwe_group_arrival_confirmation_evidence={"schema":"xiaoman-qiwe-group-arrival-confirmation-evidence-v1","success":true,"confirmation_status":"confirmed","confirmation_method":"human_visible_group_check","confirmed_by":"<safe-human-operator-id>","confirmed_at":"YYYY-MM-DDTHH:MM:SSZ","target_channel":"qiwe","target_group_alias":"community_activity_group","workflow_root_id":"<workflow-root-uuid>","send_ready_work_item_id":"<send-ready-work-item-uuid>","generated_image_artifact_id":"<generated-image-artifact-uuid>","artifact_content_hash":"sha256:<final-jpeg-sha256>","external_send_executed":true,"raw_secret_fields_retained":false}
```

## Checklist

| Step | Required result                                                                  | Passed |
| ---- | -------------------------------------------------------------------------------- | ------ |
| 1    | Real activity production evidence checker passed                                 |        |
| 2    | Human operator observed the image message in the intended QiWe group             |        |
| 3    | Confirmation ids and `artifact_content_hash` match the real activity evidence    |        |
| 4    | No raw group id, message id, media URL, callback credential, log, or chat copied |        |
| 5    | Group-arrival confirmation checker passed                                        |        |

## Exclusions

Do not record QiWe token, GUID, API secret material, raw target group id, message id,
request id, callback event id, file id, MD5 value, AES key, file size, filename, media
URL, database URL, database credentials, raw chat content, screenshots containing member
profiles, shell logs, or response bodies.
