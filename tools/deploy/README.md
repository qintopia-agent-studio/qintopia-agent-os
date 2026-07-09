# Deploy Tools

`tools/deploy/preflight.mjs` is a non-mutating deployment gate. It does not deploy code
and does not connect to the server.

Use the CI-safe gate as part of repository validation:

```bash
pnpm deploy:preflight:ci
```

Use the local preflight before an approved deployment window:

```bash
pnpm deploy:preflight
```

Local mode additionally requires:

- current branch is `master`
- worktree is clean
- deployment policy, CI/CD gate docs, and sidecar cutover plan exist
- repository checks include registry, manifest, policy, secret, tests, smoke, and deploy
  preflight gates

## Systemd Cutover Preview

Validate that the sidecar systemd renderer can produce reviewable units:

```bash
pnpm deploy:systemd:check
```

The renderer is non-mutating. It writes review files to `dist/` by default and refuses
to write directly into `/etc/systemd/system`.

## Release/Current Model

Validate the stable release/current service and Hermes MCP wrapper model:

```bash
pnpm deploy:release-model:check
```

Validate the production deploy request runner contracts:

```bash
pnpm deploy:runner:check
```

The deploy runner is the server-side pull model for manual production deployments:
GitHub writes a schema-validated request to COS, and the server runner promotes reviewed
artifacts into `release/current`.

The check is non-mutating. It verifies the worker units render through
`qintopia-agent-os-releases/current`, avoid `/home/ubuntu/qintopia-msg-sidecar`, and
that the Hermes `mcp-context` wrapper can run from a verified artifact,
`release/current`, or explicit `QINTOPIA_SIDECAR_BIN`.

## Deploy Contract Checks

Validate deploy package metadata and production-adjacent smoke boundaries:

```bash
pnpm deploy:contracts:check
```

The check is non-mutating. It also protects the aggregate Xiaoman production preflight
smoke so it remains a composition of read-only observation scripts and does not grow
apply smoke, deploy, release, Feishu write, QiWe, or external-send behavior.

## GitHub App Git Access

Validate the GitHub App git wrapper without credentials:

```bash
pnpm deploy:github-app-git:check
```

Run git commands against the private repository with a short-lived installation token:

```bash
GITHUB_APP_ID=4214034 \
GITHUB_APP_INSTALLATION_ID=144332887 \
GITHUB_APP_PRIVATE_KEY_PATH=/etc/qintopia/github-app/qintopia-agent-os-deployer.pem \
deploy/sidecar/scripts/github-app-git.sh -- \
  ls-remote https://github.com/qintopia-agent-studio/qintopia-agent-os.git refs/heads/master
```

The wrapper uses a temporary `GIT_ASKPASS` helper and never writes the token into the
remote URL, git config, or command arguments.

## Artifact Build

Build the sidecar CI artifact layout locally:

```bash
pnpm artifact:sidecar
```

The command writes `dist/sidecar-artifacts/qintopia-message-sidecar-linux-x86_64-gnu`
with the release binary, `artifact-manifest.json`, and `SHA256SUMS`. `dist/` is ignored
by git.

The CI artifact job uses Rust 1.75.0 to match `runtime/sidecar/Cargo.toml`
`rust-version`. Server deployment downloads the uploaded artifact and does not require
Node.js, pnpm, Rust, or Docker on the production host.

## Artifact Retention

Prune old GitHub Actions sidecar artifacts:

```bash
GITHUB_TOKEN="replace-with-actions-write-token" \
GITHUB_REPOSITORY="qintopia-agent-studio/qintopia-agent-os" \
pnpm artifact:prune:sidecar
```

The command keeps the latest ten artifacts named
`qintopia-message-sidecar-linux-x86_64-gnu` by default. Override the count with
`QINTOPIA_ARTIFACT_KEEP_COUNT` or `--keep <count>`. Older same-name artifacts are
deleted through the GitHub Actions Artifacts API.
