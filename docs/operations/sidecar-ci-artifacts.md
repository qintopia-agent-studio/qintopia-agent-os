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

## Server Download

For the private repository, downloading GitHub Actions artifacts requires GitHub API
read access. Use the Qintopia Agent OS deployer GitHub App as the default credential
path. A short-lived `GITHUB_TOKEN` or GitHub CLI session is only a fallback for
emergency or one-off migration work.

Server-side fetch command with GitHub App credentials:

```bash
export GITHUB_APP_ID="<github-app-id>"
export GITHUB_APP_INSTALLATION_ID="<installation-id>"
export GITHUB_APP_PRIVATE_KEY_PATH="/etc/qintopia/github-app/qintopia-agent-os-deployer.pem"
deploy/sidecar/scripts/fetch-ci-artifact.sh \
  --sha <approved-target-sha> \
  --output-dir /home/ubuntu/qintopia-agent-os-artifacts/<approved-target-sha>
```

The script generates a GitHub App JWT from the server-local private key, exchanges it
for a one-hour installation token, writes GitHub API headers to a temporary curl config
file, and keeps token material out of process arguments. Do not change it back to
`curl -H "Authorization: Bearer ..."` because that exposes credentials through process
arguments on the server.

GitHub API metadata requests use a shorter timeout. The artifact zip download uses a
separate longer timeout and retry profile because GitHub artifact downloads can be slow
or interrupted on the production server network. Override `GITHUB_DOWNLOAD_MAX_TIME`
only for the download phase when needed.

The script requires only:

- `curl`
- `jq`
- `unzip`
- `sha256sum`
- `python3`
- `openssl`

It does not require Node.js, pnpm, Rust, Docker, or direct source edits on the server.
The GitHub App private key remains outside git and should be readable only by the
deployment operator or service account.

## Verification

The fetch script automatically runs:

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
