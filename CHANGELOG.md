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

### Changed

- Moved migration status out of root README and agent rule files; root docs now link to
  the migration plan instead of embedding transient state.
- Linked the English and Chinese root READMEs and connected root collaboration files to
  the docs hub.
