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
- provider account, cost cap, and Feishu Base storage boundary;
- isolated target group allowlist for downstream QiWe staging;
- QiWe send-ready work item UUID after human image approval; and
- trusted callback source for the one bounded QiWe callback.

## Staging Env Allowlist

The staging env file may contain only reviewed literal assignments for the staging
adapter keys. It must not contain production database URLs, production group ids, Hermes
secrets, NATS settings, unrelated Feishu tokens, proxy variables, shell commands,
command substitution, exports, or duplicate keys. The only Feishu secret allowed in this
file is the reviewed Huabaosi generated-image Base token, paired with its exact
allowlist entry for the fixed staging exercise.

Huabaosi staging keys:

- `QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED`
- `QINTOPIA_SIDECAR_DATABASE_URL`
- `QINTOPIA_HUABAOSI_IMAGE_PROVIDER`
- `QINTOPIA_HUABAOSI_IMAGE_MODEL`
- `QINTOPIA_HUABAOSI_IMAGE_API_BASE_URL`
- `QINTOPIA_HUABAOSI_IMAGE_API_KEY`
- `QINTOPIA_HUABAOSI_IMAGE_STORAGE_BACKEND=feishu-base`
- `QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED`
- `QINTOPIA_HUABAOSI_FEISHU_MIRROR_APPROVAL`
- `QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA`
- `QINTOPIA_DEPLOYED_COMMIT_SHA`
- `QINTOPIA_HUABAOSI_FEISHU_DATABASE_URL_SHA256`
- `QINTOPIA_HUABAOSI_FEISHU_BASE_TOKEN`
- `QINTOPIA_HUABAOSI_FEISHU_ALLOWED_BASE_TOKENS`
- `QINTOPIA_HUABAOSI_FEISHU_ARTIFACT_TABLE_ID`
- `QINTOPIA_HUABAOSI_FEISHU_ALLOWED_ARTIFACT_TABLE_IDS`
- `QINTOPIA_HUABAOSI_FEISHU_PROFILE_ENV_PATH`
- `QINTOPIA_HUABAOSI_FEISHU_SCHEMA_VERSION=huabaosi-generated-image-v1`
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

The Huabaosi staging smoke reads only the Huabaosi key allowlist above. If the same
staging env file already contains the downstream QiWe keys, the Huabaosi smoke must
ignore those keys and must not pass them to its child sidecar process. Unknown keys and
invalid assignment syntax still fail closed.

For the Feishu Base primary-storage path, Huabaosi image generation must not require
`QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS`; storage is proven by a `feishu-base://`
artifact URI from the worker, not by an HTTP media host.

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

The reviewed artifact source for the combined Huabaosi/QiWe staging exercise is the
manually dispatched GitHub Actions artifact
`qintopia-message-sidecar-staging-linux-x86_64-gnu`. Its manifest must show exactly
`cargo_features: [huabaosi-staging-adapter, qiwe-staging-adapter]`, `staging_only=true`,
and `production_eligible=false`. Do not use the production
`qintopia-message-sidecar-linux-x86_64-gnu` artifact for staging evidence, and do not
install the staging artifact under the production release root.

## Provision Staging Env File

Prepare the fixed staging env file from an owner-reviewed server-local JSON values file.
The values file must stay on the server and must not be committed, copied into reports,
or pasted into chat.

Use `docs/operations/message-sidecar-staging-values.template.json` only as a non-secret
checklist for the required keys. Copy its shape into the server-local
`/etc/qintopia/message-sidecar-staging-values.json`, replace every placeholder with
owner-reviewed staging values, and keep that server-local values file out of git, PRs,
reports, logs, and chat. The template itself is not renderer-ready and must not be
applied as-is.

First run the renderer in validation mode. This prints only
`staging_runtime_env_render=` sanitized JSON and does not write the env file:

```bash
deploy/sidecar/scripts/render-staging-runtime-env.py \
  --values /etc/qintopia/message-sidecar-staging-values.json \
  --expected-database-url-sha256 '<approved staging database URL sha256>'
```

After the sanitized report says `action_status=staging_env_render_ready`, create the
fixed env file with the exact owner approval phrase:

```bash
sudo deploy/sidecar/scripts/render-staging-runtime-env.py \
  --values /etc/qintopia/message-sidecar-staging-values.json \
  --expected-database-url-sha256 '<approved staging database URL sha256>' \
  --apply \
  --approval approved-staging-runtime-env-provision
```

The apply path writes only `/etc/qintopia/message-sidecar-staging.env`, requires root,
requires a non-existing output file, writes mode `0600`, renders only the reviewed
staging env key allowlist, verifies the staging database URL hash, requires exactly one
isolated staging group id, and does not contact Postgres, provider, media, Feishu, QiWe,
systemd, GitHub Releases, or any network endpoint.

The renderer requires exactly one isolated staging group id.

Do not hand-edit `/etc/qintopia/message-sidecar-staging.env` to satisfy a path-only
readiness check. If a rendered env file must be replaced, remove the stale file through
an explicit owner-reviewed rollback step before rerunning the renderer.

## Provision Staging Sidecar Artifact

After owner approval, provision the staging-only artifact with the reviewed helper from
the deploy bundle:

```bash
QINTOPIA_STAGING_SIDECAR_PROVISION_APPROVAL=approved-staging-sidecar-provision \
  deploy/sidecar/scripts/fetch-staging-sidecar-artifact.sh \
  --sha 37fff8bf819f0df68825961203e7998b51a07c31
```

The helper downloads only the successful `artifacts.yml` GitHub Actions artifact named
`qintopia-message-sidecar-staging-linux-x86_64-gnu`, verifies `SHA256SUMS`, verifies the
manifest staging feature boundary, rejects production-eligible manifests, and installs
only under `/home/ubuntu/qintopia-agent-os-staging-releases/<sha>/sidecar/`.

Current reviewed staging artifact evidence:

- GitHub Actions run:
  `https://github.com/qintopia-agent-studio/qintopia-agent-os/actions/runs/29495174705`
- release SHA: `37fff8bf819f0df68825961203e7998b51a07c31`
- sidecar SHA-256: `8a04ab44cad0b60cbef499d7a58e0fb8fcac577be537d1418ec3649f38c4fa1f`
- staging tarball SHA-256:
  `87deb0c580b361c690a0de67ad31f14e2285b6230c1eb872204835a7ab1e4895`
- deploy bundle SHA-256:
  `a733f809f323771ff88fae9b0e3ee4694b291c36b5dbdb9df60db628b5046e11`
- deploy bundle contains `deploy/sidecar/scripts/fetch-staging-sidecar-artifact.sh`.

## Validation Sequence

Run the validations in this order after provisioning. Retain only sanitized stdout
records and checker results.

1. `QINTOPIA_STAGING_RUNTIME_PREREQUISITE_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/staging-runtime-prerequisite-observation-smoke.sh`
   with the approved release SHA and packaged staging sidecar SHA-256.
2. `QINTOPIA_STAGING_RUNTIME_VALUES_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/staging-runtime-values-observation-smoke.sh`.
   This is a read-only metadata check for the server-local values JSON, renderer, and
   fixed output env path. It does not read values, execute the renderer, or print
   secrets. It must report `ready_for_render_validation` before the renderer validation
   command runs.
3. `deploy/sidecar/scripts/render-staging-runtime-env.py` in validation mode, then
   `--apply --approval approved-staging-runtime-env-provision` only after the sanitized
   render report is ready and the owner has approved the server-local values file.
4. Unified staging runtime readiness evidence:

   ```bash
   QINTOPIA_STAGING_RUNTIME_READINESS_EVIDENCE_ENABLE=1 \
   QINTOPIA_STAGING_RUNTIME_RELEASE_SHA='<approved staging release sha>' \
   QINTOPIA_STAGING_RUNTIME_SIDECAR_SHA256='<approved staging sidecar binary sha256>' \
   QINTOPIA_STAGING_RUNTIME_DATABASE_URL_SHA256='<approved staging database URL sha256>' \
     deploy/sidecar/scripts/staging-runtime-readiness-evidence-smoke.sh
   ```

   Retain only the emitted `staging_runtime_readiness_evidence=` JSON. It must report
   `ready_for_huabaosi_qiwe_staging_smokes` before any Huabaosi or QiWe real staging
   exercise. The report records only the approved release SHA, packaged sidecar SHA-256,
   staging database URL SHA-256, child readiness statuses, and sanitized limitations; it
   does not read the env file contents or execute the sidecar.

5. Huabaosi staging smoke for exactly one approved image request work item. The final
   JPEG storage boundary is the fixed Huabaosi Feishu Base table, not an HTTP
   upload/public URL service.
6. `node tools/deploy/check-huabaosi-image-staging-evidence.mjs`.
7. Record `docs/reports/templates/huabaosi-image-generation-staging-evidence.md`.
8. After the separate Feishu attachment revalidation and QiWe delivery path is present
   on the staged release, run QiWe readiness, preflight, upload, callback, QiWe evidence
   check, and cross-flow hash check. The current QiWe async upload path requires a
   stable allowlisted HTTPS `fileUrl`, so QiWe intake must continue to fail closed for
   `feishu-base://` artifacts. Do not solve this by exposing Feishu attachment tokens,
   adding an unreviewed public proxy/upload service, or falling back to QiWe synchronous
   upload APIs marked deprecated in the reviewed protocol plan.

Hold immediately if any readiness report says the env file is missing, the release root
is missing, the binary hash mismatches, the staging database URL hash is absent, an
unsupported env key exists, or any evidence line contains a forbidden sensitive shape.

## Production Boundary

This runbook is not production enablement. It must not be used to install a production
timer, enable a listener, merge or publish a Release, write Feishu, call a production
provider, send to a production group, or treat local fake-smoke results as real staging
evidence.
