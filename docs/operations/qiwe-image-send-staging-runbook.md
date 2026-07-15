# QiWe Image-Send Staging Runbook

Date: 2026-07-15

This runbook is the owner-approved staging path for proving Xiaoman final image delivery
without enabling production QiWe sending. It exercises exactly one reviewed
`group_message_request` work item through the `qiwe-staging-adapter` upload and callback
send path.

It does not install a listener, service, timer, production build, Feishu writeback, or
production runtime configuration.

## Preconditions

- Use an immutable staging sidecar binary compiled with the non-default
  `qiwe-staging-adapter` feature.
- Keep production sidecar artifacts on default/production features only; production
  artifacts must not contain `qiwe-staging-adapter`.
- Prepare one reviewed send-ready work item UUID for the final approved JPEG.
- Use exactly one isolated, case-sensitive target group allowlist entry.
- Use a staging env file whose path is absolute, contains `staging`, and contains only
  the fixed key allowlist parsed by
  `deploy/sidecar/scripts/qiwe-image-send-staging-smoke.sh`.
- Have the owner-approved staging database URL SHA-256 ready. Do not paste the database
  URL into a command, report, PR, or chat.
- Confirm the callback source can stream one raw `cmd=20000` callback directly to stdin.
  Do not write the callback body to disk.

## Preflight Phase

Run this first on the reviewed staging host. It validates the staging binary, env
allowlist, owner phrase, database hash, webhook readiness, and allowlist counts without
claiming a work item, opening a QiWe upload, or reading callback stdin:

```bash
QINTOPIA_QIWE_IMAGE_STAGING_SMOKE_ENABLE=1 \
QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL=approved-staging-qiwe-image-send \
QINTOPIA_QIWE_IMAGE_STAGING_PHASE=preflight \
QINTOPIA_QIWE_IMAGE_STAGING_ENV_FILE=/etc/qintopia/message-sidecar-staging.env \
QINTOPIA_QIWE_IMAGE_STAGING_DATABASE_URL_SHA256='<approved staging database URL sha256>' \
deploy/sidecar/scripts/qiwe-image-send-staging-smoke.sh
```

Success means the local staging boundary is ready for an owner-approved upload phase. It
does not prove a send-ready work item exists, contact QiWe, or send an image.

## Upload Phase

Run this from the repository root on the reviewed staging host:

```bash
QINTOPIA_QIWE_IMAGE_STAGING_SMOKE_ENABLE=1 \
QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL=approved-staging-qiwe-image-send \
QINTOPIA_QIWE_IMAGE_STAGING_PHASE=upload \
QINTOPIA_QIWE_IMAGE_STAGING_ENV_FILE=/etc/qintopia/message-sidecar-staging.env \
QINTOPIA_QIWE_IMAGE_STAGING_DATABASE_URL_SHA256='<approved staging database URL sha256>' \
QINTOPIA_QIWE_IMAGE_STAGING_WORK_ITEM_ID='<approved send-ready UUID>' \
deploy/sidecar/scripts/qiwe-image-send-staging-smoke.sh
```

Success means the upload was accepted and the work item is awaiting exactly one bounded
callback. It does not mean `/msg/sendImage` has run. The smoke prints only fixed
`qiwe_image_send_staging_evidence=<json>` lines plus the final pass message; keep those
evidence JSON objects instead of raw subprocess output.

## Callback Phase

Stream the trusted callback directly to the smoke. The callback must not pass through a
file, environment variable, CLI argument, NATS event, report, log, or shell history.

```bash
trusted-staging-callback-source | \
QINTOPIA_QIWE_IMAGE_STAGING_SMOKE_ENABLE=1 \
QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL=approved-staging-qiwe-image-send \
QINTOPIA_QIWE_IMAGE_STAGING_PHASE=callback \
QINTOPIA_QIWE_IMAGE_STAGING_ENV_FILE=/etc/qintopia/message-sidecar-staging.env \
QINTOPIA_QIWE_IMAGE_STAGING_DATABASE_URL_SHA256='<same approved staging database URL sha256>' \
QINTOPIA_QIWE_IMAGE_STAGING_WORK_ITEM_ID='<same approved send-ready UUID>' \
deploy/sidecar/scripts/qiwe-image-send-staging-smoke.sh
```

Success means the callback matched the approved JPEG identity, the send gate committed
`sending`, and exactly one reviewed staging `/msg/sendImage` completed for the isolated
allowlisted group. Keep the printed `qiwe_image_send_staging_evidence=<json>` objects
only after confirming no raw callback, request id, credentials, group id, media URI, or
database URL appears in operator notes.

## Evidence Check

After the real staging run, put only the retained smoke stdout or copied evidence lines
into an operator-local file and validate it before attaching it to a PR or review note:

```bash
node tools/deploy/check-qiwe-image-staging-evidence.mjs <staging-evidence-output.txt>
```

For a preflight-only rehearsal, use:

```bash
node tools/deploy/check-qiwe-image-staging-evidence.mjs --preflight-only <preflight-evidence-output.txt>
```

The checker requires a ready preflight record plus matching upload/callback work item
evidence for complete mode. It fails closed if raw callback keys, database URLs, QiWe
tokens, group ids, media URIs, unexpected fields, duplicate upload/callback records, or
an incomplete send outcome appear.

## Evidence To Keep

Record only these fields:

- repository commit SHA and staging binary SHA-256;
- exact command phases run: `upload` and `callback`;
- staging database URL SHA-256, not the URL;
- work item UUID;
- fixed action statuses from the smoke;
- callback credential schema id;
- callback additional-field count;
- whether `external_upload_requested` and `external_send_executed` were true;
- rollback owner and action.

## Evidence To Exclude

Never record:

- QiWe token, GUID, API endpoint secret material, or target group id;
- database URL or credentials;
- media URI, filename, file id, MD5 value, AES key, file size, or provider response;
- raw request id, callback body, callback event id, message id, or response body;
- raw shell output if it includes anything outside the fixed sanitized report fields.

## Hold Conditions

Stop and do not retry automatically if:

- preflight rejects the staging adapter, approval phrase, database hash, webhook
  readiness, or allowlists;
- the upload phase reports anything other than `image_upload_accepted`;
- the callback phase reports anything other than `image_send_completed`;
- output scanning reports forbidden sensitive text;
- a callback is missing, duplicated, expired, or does not match the approved JPEG
  filename/MD5/byte-size identity;
- any non-success after the send gate leaves the attempt `ambiguous`.

## Rollback

Rollback is to leave production builds unchanged, keep
`QINTOPIA_QIWE_IMAGE_SEND_ENABLED=0` outside the isolated staging exercise, disable any
temporary staging callback bridge enablement, and retain both QiWe worker commands
unscheduled. Do not add production listener, service, timer, or release activation in
the staging-evidence PR.
