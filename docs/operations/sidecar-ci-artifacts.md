# Sidecar CI Artifacts

M9.1 moves sidecar release binary creation into GitHub Actions. The server should deploy
an artifact built from an approved commit SHA instead of rebuilding the binary with
local Node.js, pnpm, or Rust tooling.

## Artifact Contract

The CI artifact name is:

```text
qintopia-message-sidecar-linux-x86_64-gnu
```

Each artifact contains:

- `qintopia-message-sidecar`: release binary
- `qintopia-message-sidecar.tar.gz`: compressed release binary bundle used for COS
  transport
- `artifact-manifest.json`: commit, branch, target, build time, runner, Rust toolchain,
  Cargo feature list, file size, and file checksum
- `SHA256SUMS`: server-side checksum file covering the release binary, compressed
  bundle, and `artifact-manifest.json`

The initial target is `linux-x86_64-gnu`, matching the current production server:

```text
Linux x86_64, Ubuntu glibc
```

Production sidecar artifacts are built with exactly the non-default
`huabaosi-production-adapter` and guarded `huabaosi-feishu-mirror-adapter` Cargo
features, and record only those names in `cargo_features`. Neither
`huabaosi-staging-adapter` nor `qiwe-staging-adapter` may appear in this builder or a
server-source production build, and all-features production artifacts remain forbidden.
Feishu primary storage for the Huabaosi canary is guarded by the production adapter path
and creates only pending AgentOS artifacts. The Feishu mirror worker is compiled into
the reviewed production artifact but can run only after persistent enablement, owner
approval, release/database hash binding, allowlists, release-local preflight, and
explicit activation. Runtime environment variables cannot select staging code or bypass
these bindings. The builder also refuses a dirty or unreadable git worktree so
`commit_sha` cannot describe different uncommitted source bytes.

The staging-only sidecar artifact name is:

```text
qintopia-message-sidecar-staging-linux-x86_64-gnu
```

It is built only by the manually dispatched Artifacts workflow when
`build_staging_sidecar=true`. It compiles exactly `huabaosi-staging-adapter` and
`qiwe-staging-adapter`, records `staging_only=true` and `production_eligible=false` in
the manifest, is retained only as a GitHub Actions artifact, and must be installed only
under `/home/ubuntu/qintopia-agent-os-staging-releases/<approved 40-hex sha>` for
owner-approved Huabaosi/QiWe staging evidence. It is never uploaded to COS, never
included in the production release build, and must not be fetched or promoted by
production deployment scripts.

## CI Requirements

The `sidecar-artifact` job runs on `master` pushes. It runs in parallel with the `check`
job to keep CI wall-clock time low. The deployment gate is the successful workflow run
for the approved commit SHA: `fetch-ci-artifact.sh` queries only successful workflow
runs, so the paired `check` job must have passed for the same commit before the artifact
can be downloaded by the runbook.

This means the approved commit has passed:

- formatting and Markdown linting
- registry and manifest validation
- active Agent package validation
- anti-drift policy checks
- secret and runtime-state scanning
- CI-safe deployment preflight
- QiWe package tests
- sidecar Rust tests
- warning-denied Clippy for both default production features and all staging/test
  features
- no-credential sidecar smoke checks

## Artifact Retention

GitHub Actions keeps at most two non-expired artifacts with this exact name:

- the current `master` build
- the previous `master` build for rollback

The `Artifacts` workflow publishes release artifacts. It is opt-in:

- run it manually through `workflow_dispatch`, or
- include `[publish-artifacts]` in the `master` commit message when an automatic
  publication is intentional.

The staging-only artifact is stricter: it is never built from the push path and can be
published only by a manual `workflow_dispatch` with `build_staging_sidecar=true`.

The `sidecar-artifact` job uploads the new artifact first, then runs
`pnpm artifact:prune:sidecar` with `actions: write` permission to delete older same-name
artifacts. This limits repository artifact storage without changing the server rollback
model: once an artifact has been downloaded and verified on the server, the server copy
under `/home/ubuntu/qintopia-agent-os-artifacts/<approved-target-sha>` remains separate
from GitHub artifact retention.

The workflow can also build `qintopia-agent-os-deploy-bundle`, which contains reviewed
operator files for M9-F: the Hermes MCP wrapper, systemd renderer, and deployment
runbooks. GitHub Actions and COS both keep the latest ten deploy bundle artifacts by
default, matching the sidecar runtime artifact retention policy.

`qintopia-agent-os-artifacts/<sha>` is an artifact download cache and audit path. The
active release model promotes verified payloads into immutable
`/home/ubuntu/qintopia-agent-os-releases/<sha>` directories and runs services through a
stable `/home/ubuntu/qintopia-agent-os-releases/current` symlink. GitHub Actions and COS
are the transport, while the server release directory is the runtime source.

## COS Distribution

GitHub Actions remains the source build environment and always publishes the GitHub
Actions artifact for audit and fallback. Tencent COS is the target production
distribution layer. CI upload to COS is enabled only through explicit repository
variables and uses COS Global Acceleration for the GitHub-hosted runner path.

Object layout:

```text
cos://qintopia-agent-os-artifacts/qintopia-agent-os/sidecar/<commit-sha>/qintopia-message-sidecar-linux-x86_64-gnu/
  artifact-manifest.json
  SHA256SUMS
  qintopia-message-sidecar.tar.gz

cos://qintopia-agent-os-artifacts/qintopia-agent-os/deploy-bundle/<commit-sha>/qintopia-agent-os-deploy-bundle/
  artifact-manifest.json
  SHA256SUMS
  qintopia-agent-os-deploy-bundle.tar.gz
```

Configured COS destination:

| Setting | Value                                    |
| ------- | ---------------------------------------- |
| Bucket  | `qintopia-agent-os-artifacts-1305166808` |
| Region  | `ap-shanghai`                            |
| Prefix  | `qintopia-agent-os`                      |

The `sidecar-artifact` workflow uploads the artifact directory to COS only when
`TENCENT_COS_UPLOAD_ENABLED=true` and these GitHub repository secrets are present:

- `TENCENT_COS_SECRET_ID`
- `TENCENT_COS_SECRET_KEY`

The upload CAM key should be scoped narrowly but must still cover how COSCLI works:
`config set` writes SecretKey auth into a temporary config file, while `config add`/`cp`
can require `HeadBucket` and `GetBucket` at bucket scope. Object write, object
head/options checks, and multipart upload operations should be limited to
`qintopia-agent-os/sidecar/*`.

The repository uses COSCLI directly rather than `TencentCloud/cos-action@v1` because the
published action metadata still targets `node12`. Keeping COS upload in a shell script
lets the CI workflow stay on Node.js 24-compatible actions while still using Tencent
Cloud's official COSCLI.

Each COSCLI call has a command-level timeout. Config commands default to 60 seconds;
upload and download commands default to 300 seconds. A timeout is treated as a failed
release transport check, not as a deployable artifact.

For GitHub-hosted runner uploads, the script sets COSCLI upload concurrency explicitly:
4 MB parts and 8 transfer threads by default. This keeps the current sidecar binary in
multipart mode instead of relying on COSCLI's larger default part size and a slow
single-stream upload to the Shanghai bucket.

The default COS payload is the compressed `qintopia-message-sidecar.tar.gz` bundle. The
server fetch script keeps the bundle in the artifact directory, extracts it, and then
uses the same `SHA256SUMS` file to verify the bundle, extracted binary, and manifest, so
systemd and Hermes still see `qintopia-message-sidecar` in the artifact directory.

Direct upload from GitHub-hosted runners to the Shanghai COS bucket has been too slow in
CI even after multipart tuning and compressed payloads. Treat this as a network path
issue, not an auth issue. COS Global Acceleration is required for direct GitHub Actions
to COS upload. The verified accelerated upload for commit
`b44e9688f17953c0ae74952c55466794865801d2` completed the COS upload step in about 14
seconds.

After each successful COS upload, the workflow runs
`deploy/sidecar/scripts/prune-cos-artifacts.sh` for both `sidecar` and `deploy-bundle`.
COS keeps the latest ten sidecar artifact SHA directories for
`qintopia-message-sidecar-linux-x86_64-gnu` and the latest ten deploy bundle SHA
directories for `qintopia-agent-os-deploy-bundle`, matching the GitHub Actions artifact
retention count. This retention is implemented in CI because bucket lifecycle rules are
time-based and cannot express "latest N builds".

Optional GitHub repository variables can override the workflow defaults:

- `TENCENT_COS_BUCKET`, defaulting to `qintopia-agent-os-artifacts-1305166808`
- `TENCENT_COS_REGION`, defaulting to `ap-shanghai`
- `TENCENT_COS_PREFIX`, defaulting to `qintopia-agent-os`
- `TENCENT_COS_ENDPOINT`, empty by default; use `cos.accelerate.myqcloud.com` only after
  bucket Global Acceleration is enabled
- `TENCENT_COS_UPLOAD_ENABLED`, defaulting to `false`

Server-side fetch command for CVM Role mode:

```bash
export TENCENT_COS_BUCKET="qintopia-agent-os-artifacts-1305166808"
export TENCENT_COS_REGION="ap-shanghai"
export TENCENT_COS_PREFIX="qintopia-agent-os"
export TENCENT_COS_AUTH_MODE="CvmRole"
export TENCENT_COS_CVM_ROLE_NAME="<cvm-role-name>"
deploy/sidecar/scripts/fetch-cos-artifact.sh \
  --sha <approved-target-sha> \
  --output-dir /home/ubuntu/qintopia-agent-os-artifacts/<approved-target-sha>
```

For Tencent Cloud Lighthouse app servers, CVM Role is not available. Use a server-local
read-only COS SecretId/SecretKey file instead:

```bash
set -a
. /etc/qintopia/cos-artifacts.env
set +a
deploy/sidecar/scripts/fetch-cos-artifact.sh \
  --sha <approved-target-sha> \
  --output-dir /home/ubuntu/qintopia-agent-os-artifacts/<approved-target-sha>
```

M9-F operator files should come from the deploy bundle, not a server-side git checkout:

```bash
set -a
. /etc/qintopia/cos-artifacts.env
set +a
deploy/sidecar/scripts/fetch-cos-artifact.sh \
  --artifact-type deploy-bundle \
  --sha <approved-deploy-bundle-sha> \
  --output-dir /tmp/qintopia-agent-os-deploy-bundle/<approved-deploy-bundle-sha>
```

The environment file contains:

```bash
export TENCENT_COS_SECRET_ID="<read-only-secret-id>"
export TENCENT_COS_SECRET_KEY="<read-only-secret-key>"
```

It does not require Node.js, pnpm, Rust, Docker, or direct source edits on the server.
After verification, assemble the runtime artifact and deploy bundle payload into
`/home/ubuntu/qintopia-agent-os-releases/<approved-release-sha>` and repoint services to
`/home/ubuntu/qintopia-agent-os-releases/current`.

## GitHub Artifact Fallback

GitHub Actions artifacts are still uploaded and pruned to the latest ten builds for CI
audit and emergency fallback. For the private repository, downloading GitHub Actions
artifacts requires GitHub API read access through the Qintopia Agent OS deployer GitHub
App or a short-lived `GITHUB_TOKEN`.

Use `deploy/sidecar/scripts/fetch-ci-artifact.sh` only when COS is unavailable and the
server can reliably reach GitHub artifact download endpoints. Do not replace the COS
path with `scp`, direct server edits, or a long-lived bot credential.

The same GitHub App can also read repository contents after the App installation has
`Contents: read`. Use `deploy/sidecar/scripts/github-app-git.sh` for server-side
`git fetch`, `git ls-remote`, or future clone operations. Keep the remote URL as plain
HTTPS and let the wrapper provide a short-lived token through `GIT_ASKPASS`.

## Verification

Both COS and GitHub fallback fetch scripts automatically run:

```bash
sha256sum -c SHA256SUMS
```

It also verifies `artifact-manifest.json` against the requested commit SHA, artifact
name, target triple, and binary checksum before marking the binary executable. The
executable bit is set after checksum verification because zipped GitHub Actions
artifacts do not preserve file modes.

Before systemd is repointed, also run:

```bash
ARTIFACT_DIR=/home/ubuntu/qintopia-agent-os-artifacts/<approved-target-sha>
export QINTOPIA_SIDECAR_BIN="${ARTIFACT_DIR}/qintopia-message-sidecar"
"${ARTIFACT_DIR}/qintopia-message-sidecar" check
deploy/sidecar/scripts/postgres-schema-preflight.sh
deploy/sidecar/scripts/operations-control-plane-smoke.sh
deploy/sidecar/scripts/xiaoman-activity-acceptance-smoke.sh
```

`postgres-schema-preflight.sh` is read-only. It verifies the expected schemas, tables,
schema_change_log versions, functions, and seeded AgentOS operations capabilities. If it
fails, stop before systemd changes and apply the missing migrations only after owner
approval. Load the database URL from environment files; do not pass it as a command-line
argument.

`operations-control-plane-smoke.sh` is a fixture smoke. Run it without the production
database environment so it validates command behavior and guardrails, not live data. For
production DB confidence, use `postgres-schema-preflight.sh`, DB-backed capability
listing, and worker check-only or dry-run commands.

Production environment files remain outside git and are loaded only during approved M9
verification or service start.

## Release/Current Target

M10 should turn the verified artifact into a release payload before any service restart:

```text
/home/ubuntu/qintopia-agent-os-releases/<approved-sha>/
  manifest.json
  sidecar/
    qintopia-message-sidecar
    SHA256SUMS
  runtime/
    postgres/
      migrations/
  agents/
  skills/
  workflows/
  mcp/
  deploy/
/home/ubuntu/qintopia-agent-os-releases/current -> <approved-sha>
/home/ubuntu/qintopia-agent-os-releases/previous -> <previous-approved-sha>
```

The switch sequence should be: download to a staging/cache path, verify manifest and
checksums, assemble the immutable release directory, update `previous`, atomically
switch `current`, then restart only approved services. Rollback switches `current` back
to `previous` and restarts those same services.

Hermes profile directories remain live runtime state. Do not replace whole profile
directories from CI. Only reviewed non-secret files, plugins, scripts, policies, and MCP
wrappers should be linked or mounted from `current`.

## Beyond Sidecar

The sidecar artifact proves the binary release path. Agent OS also needs release
payloads for Hermes-managed capabilities:

| Payload                         | Purpose                                                                                     |
| ------------------------------- | ------------------------------------------------------------------------------------------- |
| `sidecar-runtime`               | Rust sidecar binary, checksums, migrations, and runtime manifest                            |
| `hermes-profile-bundle-<agent>` | reviewed non-secret files for a Hermes profile such as Erhua `SOUL.md`, config, MCP command |
| `skill-bundle-<skill>`          | reviewed Hermes plugin or skill package such as `skills/qiwe`                               |
| `workflow-bundle-<workflow>`    | reviewed scheduled or cross-Agent workflow scripts and manifests                            |

Hermes itself is not rebuilt by this repository. Hermes remains the runtime. These
payloads update what Hermes mounts or executes: profile files, plugins, scripts, MCP
commands, and sidecar services. Live Hermes state stays on the server and must not be
overwritten by artifacts.
