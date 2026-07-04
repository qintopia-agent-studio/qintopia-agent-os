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

Validate that the M9 sidecar systemd renderer can produce monorepo-native units:

```bash
pnpm deploy:systemd:check
```

The renderer is non-mutating. It writes review files to `dist/` by default and refuses
to write directly into `/etc/systemd/system`.

## M9-F Readiness

Validate the repository-side M9-F legacy-reference removal package:

```bash
pnpm deploy:m9f:check
```

The check is non-mutating. It verifies the six already-active legacy worker units render
away from `/home/ubuntu/qintopia-msg-sidecar`, and that the Hermes `mcp-context` wrapper
can run from a verified artifact, `release/current`, or explicit `QINTOPIA_SIDECAR_BIN`.

`deploy:m9f:check` is a migration-phase harness. After M9 is complete, remove this
script or fold its stable assertions into `deploy:preflight` and `deploy:systemd:check`.

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

The command keeps the latest two artifacts named
`qintopia-message-sidecar-linux-x86_64-gnu`: the current deployment candidate and one
rollback candidate. Older same-name artifacts are deleted through the GitHub Actions
Artifacts API.
