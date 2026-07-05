# COS Artifact Distribution

Qintopia Agent OS uses Tencent Cloud COS as the target production artifact distribution
layer. GitHub Actions builds the artifact from an approved `master` commit and always
publishes the GitHub Actions artifact for audit and fallback. COS upload is explicit
opt-in and uses Tencent COS Global Acceleration for the GitHub-hosted runner path.

After COS upload is enabled and verified, the Tencent Cloud server downloads from COS
before systemd or Hermes repoints. This avoids depending on GitHub artifact download
endpoints from the Tencent Cloud server during migration windows.

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
        qintopia-message-sidecar.tar.gz
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

This follows Tencent Cloud's COSCLI command model:

- COSCLI is Tencent Cloud's official COS command-line tool:
  <https://www.tencentcloud.com/document/product/436/43249>
- `config set` writes base auth fields such as `secret_id`, `secret_key`, `mode`, and
  CVM role settings: <https://www.tencentcloud.com/document/product/436/43251>
- `config add` records bucket name, region, and alias:
  <https://www.tencentcloud.com/document/product/436/43251>
- `cp` uploads and downloads files:
  <https://www.tencentcloud.com/document/product/436/43256>

TencentCloud also publishes `TencentCloud/cos-action`, but the current
`TencentCloud/cos-action@v1` action metadata still uses `node12`. This repository keeps
GitHub Actions on Node.js 24-compatible action runtimes, so the CI path calls COSCLI
directly instead of depending on that action.

## Network Path Decision

Direct GitHub-hosted runner upload to the Shanghai bucket without acceleration was too
slow to rely on during release windows. The CI evidence showed that authentication and
small object writes worked, but binary/bundle transfer was too slow to finish within
bounded release transport timeouts:

| CI run        | Payload                                      | Result                                            |
| ------------- | -------------------------------------------- | ------------------------------------------------- |
| `28730023511` | raw `qintopia-message-sidecar` binary        | timed out after uploading about 15.9% of 24.8 MB  |
| `28731038907` | raw binary with multipart tuning             | timed out after uploading about 4.8 MB of 24.8 MB |
| `28731484765` | compressed `qintopia-message-sidecar.tar.gz` | timed out after uploading about 479 KB of 8.47 MB |

This is network-path evidence, not an authentication failure. Do not keep increasing
timeouts as the primary fix.

The direct GitHub Actions to COS path uses Tencent COS Global Acceleration:

1. Enable Global Acceleration on bucket `qintopia-agent-os-artifacts-1305166808`.
2. Set repository variable `TENCENT_COS_ENDPOINT=cos.accelerate.myqcloud.com`.
3. Set repository variable `TENCENT_COS_UPLOAD_ENABLED=true`.
4. Inspect the `sidecar-artifact` COS upload and prune logs before allowing a server
   cutover to depend on a new artifact SHA.

Tencent documents the global acceleration domain format as
`<BucketName-APPID>.cos.accelerate.myqcloud.com`; COSCLI `config add` stores the bucket
name separately and accepts the endpoint through `-e/--endpoint`.

If the accelerated path becomes slow again, use a Tencent-cloud-side uploader instead of
making GitHub-hosted runners the release transport bottleneck. In that model GitHub
Actions remains the builder/audit source, and a Tencent-side job or server-side approved
fetch pushes the verified artifact into COS.

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

COS prune uses `coscli ls` to discover sidecar manifest objects, so CI upload/prune also
needs bucket list permission under the artifact prefix. Recursive delete uses
`DeleteObject` and `DeleteMultipleObjects`.

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
    "name/cos:ListParts",
    "name/cos:DeleteObject",
    "name/cos:DeleteMultipleObjects"
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

| Name                         | Default                                  | Notes                                                                       |
| ---------------------------- | ---------------------------------------- | --------------------------------------------------------------------------- |
| `TENCENT_COS_BUCKET`         | `qintopia-agent-os-artifacts-1305166808` | non-secret bucket name                                                      |
| `TENCENT_COS_REGION`         | `ap-shanghai`                            | non-secret bucket region                                                    |
| `TENCENT_COS_PREFIX`         | `qintopia-agent-os`                      | object prefix                                                               |
| `TENCENT_COS_ENDPOINT`       | empty                                    | use `cos.accelerate.myqcloud.com` only after bucket acceleration is enabled |
| `TENCENT_COS_UPLOAD_ENABLED` | `false`                                  | must be exactly `true` to upload to COS                                     |

The `sidecar-artifact` job uploads to COS only when `TENCENT_COS_UPLOAD_ENABLED=true`
and both upload secrets are present. If upload is disabled, CI still builds and uploads
the GitHub Actions artifact.

After a successful COS upload, CI prunes old COS artifact directories and keeps only the
latest two sidecar artifact SHA directories for
`qintopia-message-sidecar-linux-x86_64-gnu`. The prune step uses
`deploy/sidecar/scripts/prune-cos-artifacts.sh --keep 2`, so COS retention matches the
GitHub Actions artifact retention policy.

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

The upload script verifies `SHA256SUMS` before upload and writes these files by default:

- `artifact-manifest.json`
- `SHA256SUMS`
- `qintopia-message-sidecar.tar.gz`

`qintopia-message-sidecar.tar.gz` contains the release binary. The server fetch script
extracts it and then verifies the extracted `qintopia-message-sidecar` with
`SHA256SUMS`. This keeps the server runtime layout unchanged while reducing the object
payload sent from GitHub-hosted runners to COS.

Set `TENCENT_COS_ARTIFACT_PAYLOAD=raw` only for emergency debugging when you need to
upload the raw binary object directly.

COSCLI execution is bounded so the workflow fails with a useful diagnostic instead of
waiting for the whole job timeout:

| Environment variable              | Default | Applies to                 |
| --------------------------------- | ------- | -------------------------- |
| `COSCLI_CONFIG_TIMEOUT_SECONDS`   | `60`    | `config set`, `config add` |
| `COSCLI_TRANSFER_TIMEOUT_SECONDS` | `300`   | `cp` upload/download       |
| `COSCLI_PART_SIZE_MB`             | `4`     | upload multipart part size |
| `COSCLI_THREAD_NUM`               | `8`     | upload transfer threads    |

If a transfer times out, the script prints the bucket alias, object prefix, and
sanitized COSCLI output without printing credentials.

The script passes `TENCENT_COS_ENDPOINT` into `coscli config add -e` when the variable
is set. Leave it empty unless the bucket-side endpoint feature has already been enabled.

The COS prune script lists objects under `qintopia-agent-os/sidecar/`, filters
`artifact-manifest.json` paths locally, sorts sidecar SHA directories by manifest update
time, and deletes older directories with COSCLI recursive delete. Use `--dry-run` before
changing retention behavior.

GitHub artifact upload compresses the sidecar artifact to about 9 MB, while the raw
release binary is about 25 MB. COS distribution therefore uses the compressed sidecar
bundle as the default transport payload. The upload script also uses a smaller part size
and multiple transfer threads so larger future bundles can use COSCLI multipart
concurrency without changing the artifact contract.

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
- compressed bundle is extracted into `qintopia-message-sidecar`
- manifest checksum matches `SHA256SUMS`
- `sha256sum -c SHA256SUMS` passes

Only after this should systemd or Hermes references be repointed.

## Current Owner Inputs

Received non-secret COS values:

- COS bucket full name including APPID: `qintopia-agent-os-artifacts-1305166808`
- COS region: `ap-shanghai`
- object prefix: use default `qintopia-agent-os`

Server-side read credentials are stored outside git in `/etc/qintopia/cos-artifacts.env`
on the Tencent Cloud Lighthouse server.

Provide these secrets only through GitHub repository Secrets or server-local files:

- CI upload `TENCENT_COS_SECRET_ID`
- CI upload `TENCENT_COS_SECRET_KEY`
- server read-only SecretId/SecretKey, only if not using CVM Role

Before turning on CI upload, decide whether to enable COS Global Acceleration for this
bucket. If enabled, set `TENCENT_COS_ENDPOINT` and `TENCENT_COS_UPLOAD_ENABLED=true`,
push a commit to `master`, and confirm the `sidecar-artifact` job uploaded to COS. Then
run the server download command and verify the artifact before continuing M9-F.
