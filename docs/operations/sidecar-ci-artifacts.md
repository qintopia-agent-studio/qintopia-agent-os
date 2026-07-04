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

The artifact job must run only after `pnpm check` succeeds. This means the artifact has
already passed:

- formatting and Markdown linting
- registry and manifest validation
- active Agent package validation
- anti-drift policy checks
- secret and runtime-state scanning
- CI-safe deployment preflight
- QiWe package tests
- sidecar Rust tests
- no-credential sidecar smoke checks

## Server Download

For the private repository, downloading GitHub Actions artifacts requires GitHub API
read access. Use a short-lived token or GitHub CLI session managed outside git.

Server-side fetch command:

```bash
export GITHUB_TOKEN="<short-lived-token>"
deploy/sidecar/scripts/fetch-ci-artifact.sh \
  --sha <approved-target-sha> \
  --output-dir /home/ubuntu/qintopia-agent-os-artifacts/<approved-target-sha>
```

The script requires only:

- `curl`
- `jq`
- `unzip`
- `sha256sum`

It does not require Node.js, pnpm, Rust, Docker, or direct source edits on the server.

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
deploy/sidecar/scripts/operations-control-plane-smoke.sh
deploy/sidecar/scripts/xiaoman-activity-acceptance-smoke.sh
```

Production environment files remain outside git and are loaded only during approved M9
verification or service start.
