# Staging Runtime Provisioning Runbook

Date: 2026-07-16

This runbook defines the owner-reviewed staging runtime inputs required before a real
Huabaosi image-generation smoke or downstream QiWe image-send staging exercise. It does
not provision production, publish a Release, install a service, enable a timer, write
Feishu, call a provider, call QiWe, or send externally.

## Scope

The fixed staging runtime boundary is:

- `/etc/qintopia/message-sidecar-staging.env`
- `/home/ubuntu/qintopia-agent-os-staging-releases/<40-hex-sha>/sidecar/qintopia-message-sidecar`

The current server observation
`docs/reports/2026-07-16-staging-runtime-prerequisite-observation.md` shows both fixed
paths are absent on `paxon-server`. Real staging cannot start until an owner-approved
operator provisions those inputs and records the approved hashes.

## Required Owner Inputs

Record these values in the review decision before provisioning. Do not commit the secret
values themselves.

- staging release SHA;
- packaged staging sidecar SHA-256;
- staging database URL SHA-256;
- isolated database identity and rollback owner;
- Huabaosi image request work item UUID;
- provider account, cost cap, and media storage boundary;
- isolated media host allowlist;
- isolated target group allowlist for downstream QiWe staging;
- QiWe send-ready work item UUID after human image approval; and
- trusted callback source for the one bounded QiWe callback.

## Staging Env Allowlist

The staging env file may contain only reviewed literal assignments for the staging
adapter keys. It must not contain production database URLs, production group ids, Hermes
secrets, NATS settings, Feishu tokens, proxy variables, shell commands, command
substitution, exports, or duplicate keys.

Huabaosi staging keys:

- `QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED`
- `QINTOPIA_SIDECAR_DATABASE_URL`
- `QINTOPIA_HUABAOSI_IMAGE_PROVIDER`
- `QINTOPIA_HUABAOSI_IMAGE_MODEL`
- `QINTOPIA_HUABAOSI_IMAGE_API_BASE_URL`
- `QINTOPIA_HUABAOSI_IMAGE_API_KEY`
- `QINTOPIA_HUABAOSI_MEDIA_UPLOAD_ENDPOINT`
- `QINTOPIA_HUABAOSI_MEDIA_PUBLIC_BASE_URL`
- `QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS`
- `QINTOPIA_HUABAOSI_MEDIA_MAX_BYTES`

Downstream QiWe staging keys, once the QiWe staging PR is present on the staged release:

- `QINTOPIA_QIWE_IMAGE_SEND_ENABLED`
- `QINTOPIA_QIWE_IMAGE_SEND_WEBHOOK_READY`
- `QINTOPIA_SIDECAR_DATABASE_URL`
- `QIWE_API_URL`
- `QIWE_TOKEN`
- `QIWE_GUID`
- `QINTOPIA_QIWE_IMAGE_SEND_ALLOWED_HOSTS`
- `QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS`
- `QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS`

The env file must be readable only by the staging operator/root boundary. Readiness
smokes must verify only file metadata and must not read or print env contents.

## Release Root Requirements

The staging release root must be immutable for the staging exercise:

- fixed root path under `/home/ubuntu/qintopia-agent-os-staging-releases`;
- release directory name is the exact reviewed 40-character lowercase commit SHA;
- sidecar binary path is exactly `sidecar/qintopia-message-sidecar` under that release;
- no checked path component is a symlink;
- no checked path component is group- or world-writable;
- the sidecar binary SHA-256 matches the owner-approved value; and
- the staged sidecar is compiled only with the reviewed staging feature needed for the
  exercise, never as a production artifact.

## Validation Sequence

Run the validations in this order after provisioning. Retain only sanitized stdout
records and checker results.

1. `QINTOPIA_STAGING_RUNTIME_PREREQUISITE_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/staging-runtime-prerequisite-observation-smoke.sh`
   with the approved release SHA and packaged staging sidecar SHA-256.
2. Huabaosi staging readiness smoke with the approved release SHA and sidecar SHA-256.
3. Huabaosi staging smoke for exactly one approved image request work item.
4. `node tools/deploy/check-huabaosi-image-staging-evidence.mjs`.
5. Record `docs/reports/templates/huabaosi-image-generation-staging-evidence.md`.
6. After the QiWe staging PR is present on the staged release, run QiWe readiness,
   preflight, upload, callback, QiWe evidence check, and cross-flow hash check.

Hold immediately if any readiness report says the env file is missing, the release root
is missing, the binary hash mismatches, an unsupported env key exists, or any evidence
line contains a forbidden sensitive shape.

## Production Boundary

This runbook is not production enablement. It must not be used to install a production
timer, enable a listener, merge or publish a Release, write Feishu, call a production
provider, send to a production group, or treat local fake-smoke results as real staging
evidence.
