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
- `artifact-manifest.json`: commit, branch, target, build time, runner, Rust toolchain,
  file size, and file checksum
- `SHA256SUMS`: checksum file for server-side verification

The initial target is `linux-x86_64-gnu`, matching the current production server:

```text
Linux x86_64, Ubuntu glibc
```

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
- no-credential sidecar smoke checks

## Artifact Retention

GitHub Actions keeps at most two non-expired artifacts with this exact name:

- the current `master` build
- the previous `master` build for rollback

The `sidecar-artifact` job uploads the new artifact first, then runs
`pnpm artifact:prune:sidecar` with `actions: write` permission to delete older same-name
artifacts. This limits repository artifact storage without changing the server rollback
model: once an artifact has been downloaded and verified on the server, the server copy
under `/home/ubuntu/qintopia-agent-os-artifacts/<approved-target-sha>` remains separate
from GitHub artifact retention.

`qintopia-agent-os-artifacts/<sha>` is the current transition path. The target release
model should promote verified payloads into immutable
`/home/ubuntu/qintopia-agent-os-releases/<sha>` directories and run services through a
stable `/home/ubuntu/qintopia-agent-os-releases/current` symlink. In that model, GitHub
Actions artifacts are the transport, while the server release directory is the runtime
source.

## COS Distribution

GitHub Actions remains the source build environment, but the production server should
download artifacts from Tencent Cloud COS. This avoids making the Tencent Cloud server
depend on unstable GitHub artifact download endpoints during a cutover window.

Object layout:

```text
cos://qintopia-agent-os-artifacts/qintopia-agent-os/sidecar/<commit-sha>/qintopia-message-sidecar-linux-x86_64-gnu/
  artifact-manifest.json
  SHA256SUMS
  qintopia-message-sidecar
```

Configured COS destination:

| Setting | Value                                    |
| ------- | ---------------------------------------- |
| Bucket  | `qintopia-agent-os-artifacts-1305166808` |
| Region  | `ap-shanghai`                            |
| Prefix  | `qintopia-agent-os`                      |

The `sidecar-artifact` workflow uploads the artifact directory to COS only when these
GitHub repository secrets are present:

- `TENCENT_COS_SECRET_ID`
- `TENCENT_COS_SECRET_KEY`

Optional GitHub repository variables can override the workflow defaults:

- `TENCENT_COS_BUCKET`, defaulting to `qintopia-agent-os-artifacts-1305166808`
- `TENCENT_COS_REGION`, defaulting to `ap-shanghai`
- `TENCENT_COS_PREFIX`, defaulting to `qintopia-agent-os`

Server-side fetch command:

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

If CVM Role is not available, use a read-only COS SecretId/SecretKey on the server:

```bash
export TENCENT_COS_SECRET_ID="<read-only-secret-id>"
export TENCENT_COS_SECRET_KEY="<read-only-secret-key>"
```

It does not require Node.js, pnpm, Rust, Docker, or direct source edits on the server.

## GitHub Artifact Fallback

GitHub Actions artifacts are still uploaded and pruned to the latest two builds for CI
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
