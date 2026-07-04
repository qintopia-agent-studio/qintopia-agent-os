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
