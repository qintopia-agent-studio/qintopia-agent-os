# COS Artifact Distribution

Qintopia Agent OS uses Tencent Cloud COS as the production artifact distribution layer.
GitHub Actions builds the artifact from an approved `master` commit, uploads it to COS,
and the Tencent Cloud server downloads from COS before systemd or Hermes repoints.

This avoids depending on GitHub artifact download endpoints from the Tencent Cloud
server during migration windows.

## Bucket Setup

Create one COS bucket dedicated to Agent OS release artifacts.

Recommended settings:

| Setting           | Value                                                                 |
| ----------------- | --------------------------------------------------------------------- |
| Region            | `ap-shanghai`                                                         |
| Bucket name       | `qintopia-agent-os-artifacts-1305166808`                              |
| Access permission | Private read/write                                                    |
| Versioning        | Optional; lifecycle and SHA paths already provide rollback protection |
| Public access     | Disabled                                                              |
| Object prefix     | `qintopia-agent-os/`                                                  |

Object layout:

```text
qintopia-agent-os/
  sidecar/
    <commit-sha>/
      qintopia-message-sidecar-linux-x86_64-gnu/
        artifact-manifest.json
        SHA256SUMS
        qintopia-message-sidecar
```

## Access Model

Use separate identities for CI upload and server download.

| Actor                  | Preferred credential             | Permission scope                               |
| ---------------------- | -------------------------------- | ---------------------------------------------- |
| GitHub Actions         | CAM SecretId/SecretKey in GitHub | write only under `qintopia-agent-os/sidecar/*` |
| CVM server             | CVM Role                         | read only under `qintopia-agent-os/sidecar/*`  |
| Lighthouse app server  | CAM SecretId/SecretKey           | read only under `qintopia-agent-os/sidecar/*`  |
| emergency CVM fallback | CAM SecretId/SecretKey           | read only under `qintopia-agent-os/sidecar/*`  |

Do not use root account keys. Do not put COS keys in git, systemd unit files, shell
history, or chat logs.

GitHub Actions upload uses COSCLI `config set`, `config add`, and `cp`: `config set`
writes SecretKey auth into a temporary config file, `config add` records the bucket
alias, and `cp` uploads through that temporary config. COSCLI may probe bucket/object
state and may use multipart upload depending on file size and COSCLI behavior. The
upload CAM policy should therefore allow bucket probe/list actions at the bucket scope
and object write/multipart actions at the artifact prefix scope.

Bucket-scoped probe actions:

```json
{
  "effect": "allow",
  "action": ["name/cos:HeadBucket", "name/cos:GetBucket"],
  "resource": [
    "qcs::cos:ap-shanghai:uid/1305166808:qintopia-agent-os-artifacts-1305166808/*"
  ]
}
```

Object-scoped upload actions:

```json
{
  "effect": "allow",
  "action": [
    "name/cos:HeadObject",
    "name/cos:OptionsObject",
    "name/cos:PutObject",
    "name/cos:InitiateMultipartUpload",
    "name/cos:UploadPart",
    "name/cos:CompleteMultipartUpload",
    "name/cos:AbortMultipartUpload",
    "name/cos:ListMultipartUploads",
    "name/cos:ListParts"
  ],
  "resource": [
    "qcs::cos:ap-shanghai:uid/1305166808:qintopia-agent-os-artifacts-1305166808/qintopia-agent-os/sidecar/*"
  ]
}
```

Keep write scope limited to:

```text
qcs::cos:ap-shanghai:uid/1305166808:qintopia-agent-os-artifacts-1305166808/qintopia-agent-os/sidecar/*
```

## GitHub Configuration

The workflow defaults are stored in `.github/workflows/ci.yml` because bucket and region
are deployment configuration, not secrets:

| Name                 | Value                                    |
| -------------------- | ---------------------------------------- |
| `TENCENT_COS_BUCKET` | `qintopia-agent-os-artifacts-1305166808` |
| `TENCENT_COS_REGION` | `ap-shanghai`                            |
| `TENCENT_COS_PREFIX` | `qintopia-agent-os`                      |

Add these repository secrets:

| Name                     | Description                                            |
| ------------------------ | ------------------------------------------------------ |
| `TENCENT_COS_SECRET_ID`  | CAM key with upload permission for the artifact prefix |
| `TENCENT_COS_SECRET_KEY` | SecretKey for `TENCENT_COS_SECRET_ID`                  |

Optional repository variables can override the workflow defaults:

| Name                 | Default                                  |
| -------------------- | ---------------------------------------- |
| `TENCENT_COS_BUCKET` | `qintopia-agent-os-artifacts-1305166808` |
| `TENCENT_COS_REGION` | `ap-shanghai`                            |
| `TENCENT_COS_PREFIX` | `qintopia-agent-os`                      |

The `sidecar-artifact` job uploads to COS only when the upload SecretId and SecretKey
are present.

## Server Configuration

Preferred server environment file:

```text
/etc/qintopia/cos-artifacts.env
```

CVM Role mode, for CVM hosts:

```bash
export TENCENT_COS_BUCKET="qintopia-agent-os-artifacts-1305166808"
export TENCENT_COS_REGION="ap-shanghai"
export TENCENT_COS_PREFIX="qintopia-agent-os"
export TENCENT_COS_AUTH_MODE="CvmRole"
export TENCENT_COS_CVM_ROLE_NAME="<cvm-role-name>"
```

SecretKey mode, for Tencent Cloud Lighthouse app servers or CVM fallback:

```bash
export TENCENT_COS_BUCKET="qintopia-agent-os-artifacts-1305166808"
export TENCENT_COS_REGION="ap-shanghai"
export TENCENT_COS_PREFIX="qintopia-agent-os"
export TENCENT_COS_SECRET_ID="<read-only-secret-id>"
export TENCENT_COS_SECRET_KEY="<read-only-secret-key>"
```

Keep this file `0600` and outside git.

## Upload Path

GitHub Actions runs:

```bash
deploy/sidecar/scripts/upload-cos-artifact.sh \
  --artifact-dir dist/sidecar-artifacts/qintopia-message-sidecar-linux-x86_64-gnu \
  --sha "$GITHUB_SHA"
```

The upload script verifies `SHA256SUMS` before upload and writes only these files:

- `artifact-manifest.json`
- `SHA256SUMS`
- `qintopia-message-sidecar`

## Download Path

On the server:

```bash
set -a
. /etc/qintopia/cos-artifacts.env
set +a

deploy/sidecar/scripts/fetch-cos-artifact.sh \
  --sha <approved-target-sha> \
  --output-dir /home/ubuntu/qintopia-agent-os-artifacts/<approved-target-sha>
```

The download script verifies:

- requested commit SHA matches `artifact-manifest.json`
- artifact name and target match the expected sidecar target
- manifest checksum matches `SHA256SUMS`
- `sha256sum -c SHA256SUMS` passes

Only after this should systemd or Hermes references be repointed.

## Remaining Owner Inputs

Received non-secret COS values:

- COS bucket full name including APPID: `qintopia-agent-os-artifacts-1305166808`
- COS region: `ap-shanghai`
- object prefix: use default `qintopia-agent-os`

Still needed before the first COS-backed server fetch:

- whether the CVM will use CVM Role or SecretKey fallback for reads
- CVM Role name, if using CVM Role

Provide these secrets only through GitHub repository Secrets or server-local files:

- CI upload `TENCENT_COS_SECRET_ID`
- CI upload `TENCENT_COS_SECRET_KEY`
- server read-only SecretId/SecretKey, only if not using CVM Role

After the GitHub secrets are configured, push a commit to `master` and confirm the
`sidecar-artifact` job uploaded to COS. Then run the server download command and verify
the artifact before continuing M9-F.
