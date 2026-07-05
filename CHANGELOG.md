# Changelog

All notable changes to this repository are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
repository uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html) when packages
become versioned.

## [Unreleased]

### Added

- Initialized the Qintopia Agent OS capability/plugin monorepo on the `master` branch.
- Added pnpm workspace configuration for Agent OS domains.
- Added root documentation for human, Codex, and Claude Code collaborators.
- Added Chinese counterparts for root collaboration docs.
- Added repository-wide formatting and Markdown lint tooling with Prettier and
  markdownlint-cli2.
- Added Changesets for future package-level release notes.
- Added migration plan at `docs/plans/active/monorepo-migration.md`.
- Added `.editorconfig`, `.gitattributes`, Prettier config, Markdown lint config, and
  expanded `.gitignore`.
- Added `CONTRIBUTING.md`, PR template, CODEOWNERS, and GitHub Actions CI for repository
  checks.
- Added documentation hubs at `docs/README.md` and `docs/README.zh-CN.md`.
- Added architecture, engineering, and operations indexes for the docs tree.
- Added an Agent OS architecture overview, collaboration model, package contract,
  migration policy, server change policy, and source document inventory.
- Added product scope, Agent OS domain model, Agent contracts, acceptance tests, runtime
  baseline, and reports index to complete the M3 documentation migration.
- Added M1 migration inventory records for local sources, server sources, runtime
  profiles, and Agent OS services.
- Added M2 registry schemas, domain indexes, package manifest templates, and
  `pnpm registry:check`.
- Added M4A `skills/qiwe` package metadata, registry entry, and server backup review.
- Imported the QiWe plugin source snapshot into `skills/qiwe` with docs, fixtures, and
  tests.
- Added M5 sidecar package contracts for `runtime/sidecar`, `runtime/postgres`,
  `mcp/context-server`, `mcp/message-store`, `workflows/activity-promotion`, and
  `deploy/sidecar`.
- Imported the reviewed sidecar source snapshot into the M5 package split, including
  Rust source, config templates, fixtures, migrations, data-design docs, MCP docs,
  workflow docs, deploy runbook, and smoke scripts.
- Added `pnpm test:sidecar` and `pnpm smoke:sidecar`.
- Added a sidecar monorepo cutover plan under `deploy/sidecar/docs/`.
- Added `pnpm policy:check` and M5.5 anti-drift policy documentation.
- Added deprecated package records and a decommission plan for WorkTool, WorkTool Hermes
  plugin, and OpenClaw.
- Added M6 Agent package contracts for `default`, `erhua`, `xiaoman`, `wenyuange`,
  `silaoshi`, `guanerye`, and `huabaosi`.
- Added M6.1 Agent profile templates, per-Agent capability notes, runtime notes, and an
  Agent capability matrix.
- Added `pnpm agents:check` for active Agent package and profile template validation.
- Added `pnpm secrets:check` for secret and runtime-state scanning.
- Added `pnpm deploy:preflight` and `pnpm deploy:preflight:ci` for non-mutating
  deployment gate validation.
- Added CI/CD gate documentation for repository checks, secret scanning, deployment
  preflight, and production-adjacent PR evidence.
- Added M5 runtime sidecar adoption closure documentation.
- Added `pnpm fmt:sidecar` and `pnpm check:sidecar` sidecar validation commands.
- Added M7 read-only decommission evidence for WorkTool, Xiaoqin WorkTool, and OpenClaw.
- Added M9 server cutover runbook for monorepo checkout, sidecar service cutover,
  deprecated runtime cleanup, acceptance, and rollback.
- Added the approved GitHub remote and expanded CODEOWNERS for Agent OS monorepo
  collaboration.
- Added M9 read-only server preflight findings and migration blockers.
- Recorded server bot SSH access for the Agent OS monorepo remote.
- Added M9.1 sidecar CI artifact build, manifest, checksum, upload, server fetch, and
  artifact-only smoke workflow.
- Added M9.3 sidecar systemd cutover plan and a non-mutating renderer for review-only
  monorepo-native unit files.
- Added `deploy/sidecar/scripts/postgres-schema-preflight.sh` and
  `pnpm deploy:postgres:schema:preflight` as a read-only M9 database schema gate.
- Added M9-D active service cutover evidence for the three owner-approved sidecar
  services now running from the monorepo checkout and verified CI artifact.
- Added `deploy/sidecar/scripts/github-app-git.sh` for GitHub App based private repo
  fetches without storing installation tokens in git remotes or config.
- Added `docs/operations/server-directory-plan.md` to define the target server
  filesystem, release/current model, Hermes runtime boundary, and legacy cleanup
  candidates.
- Added `deploy/sidecar/docs/m9f-legacy-reference-removal.md` and
  `pnpm deploy:m9f:check` for repository-side M9-F readiness validation.
- Added Tencent COS artifact distribution scripts, CI upload wiring, and
  `docs/operations/cos-artifact-distribution.md` so production servers can download
  verified sidecar artifacts from COS instead of GitHub artifact endpoints.
- Recorded the configured COS artifact bucket `qintopia-agent-os-artifacts-1305166808`
  in `ap-shanghai`; only COS upload/download credentials remain outside git.
- Added `docs/operations/release-current-model.md` for the M10 immutable release
  directory, `current`/`previous` symlink, rollback, and Hermes mount model.
- Added the server read-only COS CAM policy required for Lighthouse artifact downloads.
- Added the M9-F deploy runner and wrapper boundary to separate runtime artifact
  releases from reviewed operator script upgrades.
- Added `qintopia-agent-os-deploy-bundle`, a CI-built operator artifact containing the
  M9-F Hermes MCP wrapper, systemd renderer, runbooks, and Postgres migrations.

### Changed

- Moved migration status out of root README and agent rule files; root docs now link to
  the migration plan instead of embedding transient state.
- Linked the English and Chinese root READMEs and connected root collaboration files to
  the docs hub.
- Updated collaborator reading paths to include product scope, Agent OS design, and
  runtime baseline documents.
- Marked M1 inventory complete in the migration plan and linked the inventory from the
  documentation hub.
- Marked M2 registry contract complete and wired registry validation into `pnpm check`.
- Marked M4 first skill adoption in progress.
- Marked M4 first skill adoption complete.
- Added `pnpm test:qiwe` to the repository check path.
- Marked M5 runtime sidecar adoption in progress and registered the new sidecar package
  contracts in the domain registries.
- Wired sidecar tests and no-credential sidecar smokes into `pnpm check`.
- Marked the migrated sidecar deploy script as a legacy snapshot rather than the current
  monorepo-native production deploy entrypoint.
- Wired anti-drift policy checks into `pnpm check`.
- Marked M7 WorkTool decommission in progress and registered deprecated audit packages.
- Marked M6 active Agents adoption in progress and registered the active Agent package
  contracts.
- Extended anti-drift policy checks to block live Hermes runtime state under `agents/*`.
- Marked M6 Agent adoption complete.
- Wired secret scanning and CI-safe deployment preflight into `pnpm check`.
- Strengthened GitHub Actions CI to install Node.js, pnpm, Python, and Rust before
  running the full repository check path.
- Marked M8 CI/CD deployment gate complete.
- Marked M5 runtime sidecar adoption complete.
- Marked the adopted M5 runtime, MCP, workflow, and deploy package records as active
  monorepo contracts while keeping server cutover and Huabaosi shadow adoption out of M5
  scope.
- Marked M7 WorkTool decommission classification complete while deferring all server
  cleanup to final migration.
- Marked M9 server cutover prepared while keeping all server mutations blocked until an
  owner-approved migration window.
- Updated M9 runbook to deploy a CI-built sidecar artifact instead of requiring Node.js,
  pnpm, or Rust builds on the production server.
- Pinned sidecar CI checks and artifact builds to Rust 1.75.0 to match the sidecar
  `rust-version`, with `rustfmt` installed for the sidecar format gate.
- Upgraded GitHub Actions workflow actions to Node.js 24-compatible major versions.
- Optimized CI wall-clock time by running the `sidecar-artifact` upload in parallel with
  `pnpm check` on `master` pushes, while keeping deployment gated to successful workflow
  runs for the approved commit SHA.
- Removed broad Cargo target caching and deferred Rust dependency caching until it can
  run cleanly with the pinned Rust 1.75.0 toolchain.
- Added sidecar CI artifact pruning so GitHub keeps only the latest two
  `qintopia-message-sidecar-linux-x86_64-gnu` artifacts.
- Wired `pnpm deploy:systemd:check` into repository validation so M9 unit rendering
  stays artifact-based and does not drift back to server-local builds.
- Hardened `deploy/sidecar/scripts/fetch-ci-artifact.sh` so GitHub API credentials are
  passed through a temporary curl config file instead of process arguments.
- Extended deploy preflight to guard artifact credential handling and require the M9
  Postgres schema preflight path.
- Extended deploy preflight to block COS SecretId/SecretKey values from being passed
  through COSCLI transfer command arguments.
- Changed the M9 artifact download path to prefer GitHub App installation tokens, with
  `GITHUB_TOKEN` kept only as a fallback for emergency or one-off downloads.
- Updated M9 migration state after production database migrations passed; the database
  gate cleared before the active systemd service family was repointed, while real
  external adapter enablement remained blocked on reviewed allowlists/config.
- Updated the M9 migration state after the active systemd service family cutover passed;
  production remains pinned to the approved artifact SHA until a later approved repoint.
- Hardened the M9 systemd renderer and deploy preflight so all rendered sidecar service
  units include `QINTOPIA_SIDECAR_MIGRATIONS_DIR`.
- Documented the GitHub App `Contents: read` path as the replacement direction for the
  server bot SSH alias.
- Updated migration planning to treat M9-D as a partial cutover, with M9-F required to
  remove remaining `/home/ubuntu/qintopia-msg-sidecar` worker and MCP references.
- Clarified that direct `qintopia-agent-os-artifacts/<sha>` service paths are a
  transition model and that future releases should use immutable release directories
  with stable `current` and `previous` symlinks.
- Reframed the adopted standalone sidecar deployment docs as historical rollback
  evidence and pointed deploy contributors to the current M9 runbook plus M10
  release/current server plan.
- Updated the Hermes `mcp-context` wrapper so its default resolution path uses verified
  artifacts or release/current instead of the legacy standalone checkout.
- Documented `pnpm deploy:m9f:check` as temporary M9 scaffolding that must be removed or
  folded into stable deploy checks after M9 completes.
- Changed the M9 artifact distribution direction to COS-first: GitHub Actions remains
  the builder and audit artifact source, while Tencent COS becomes the default server
  download path.
- Changed COS CI upload configuration so bucket, region, and prefix use explicit
  non-secret workflow defaults while only SecretId and SecretKey come from GitHub
  Secrets.
- Clarified COS artifact distribution docs for Tencent Cloud Lighthouse servers and the
  upload CAM permissions COSCLI may require for object writes and multipart upload.
- Documented the official COSCLI command path for COS artifact distribution and the
  reason `TencentCloud/cos-action@v1` is not used in the Node.js 24 GitHub Actions
  workflow.
- Changed Tencent COS upload to explicit opt-in with `TENCENT_COS_UPLOAD_ENABLED=true`
  so `master` CI still produces the GitHub Actions artifact while the GitHub-hosted
  runner to COS network path remains unverified.
- Added optional `TENCENT_COS_ENDPOINT` support for COSCLI bucket configuration so a
  future direct GitHub Actions upload can use COS Global Acceleration after the bucket
  setting is enabled.
- Added CI-side Tencent COS artifact pruning so COS keeps the latest two sidecar
  artifact SHA directories, matching the GitHub Actions artifact retention policy.
- Corrected M9/M10 deployment docs so routine server releases use COS artifacts and
  release/current promotion instead of server-side `git fetch` or `git checkout`.
- Recorded the read-only server COS validation blocker: the current server key receives
  COS `403` on bucket-root `HEAD` before object download.
- Recorded the successful server-side read-only COS artifact validation for
  `0782f6d0f3f46d1285444f9a21f1669791be1d5e` after the server CAM policy was corrected.
- Recorded M9-F preflight and unit diff findings, including the deploy-runner wrapper
  blocker that must be resolved before worker or Hermes MCP repointing.
- Recorded the server `/tmp` wrapper preflight proving the reviewed Hermes MCP wrapper
  can resolve the COS-readonly artifact without falling back to
  `/home/ubuntu/qintopia-msg-sidecar`.
- Changed M9-F planning so wrapper, renderer, runbook, and migration files come from a
  COS deploy bundle instead of server-side `git fetch`.
- Extended COS upload, fetch, and prune scripts to support both `sidecar` and
  `deploy-bundle` artifact types with latest-two retention for each.

### Fixed

- Fixed the Tencent COS prune script file mode and added a deploy preflight guard that
  requires directly executed deployment shell scripts to be committed as executable.
- Fixed COSCLI installer stdout so CI upload scripts receive only the installed binary
  path, not checksum verification text.
- Hardened COS artifact upload and fetch scripts so SecretKey values are used only while
  writing temporary COSCLI config, transfer commands no longer pass credentials through
  `cp` arguments, and COSCLI failures include non-secret diagnostics.
- Added sanitized COSCLI failure output so CI can distinguish bucket configuration,
  permission, and object upload errors without printing COS credentials.
- Fixed COSCLI temporary config initialization so upload and fetch scripts create the
  config file before calling `config add` or `config set`.
- Corrected COSCLI authentication setup to use `config set --mode SecretKey` for
  temporary config auth and keep `cp` transfer commands credential-free.
- Added command-level COSCLI timeouts so COS upload/download hangs fail with sanitized
  diagnostics instead of waiting for the whole GitHub Actions job timeout.
- Tuned COSCLI CI uploads with smaller multipart parts and explicit transfer threads so
  sidecar release binaries do not rely on a slow single-stream upload path.
- Added a compressed `qintopia-message-sidecar.tar.gz` release bundle for COS transport;
  server fetch still extracts and verifies the original binary with `SHA256SUMS`.
