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
