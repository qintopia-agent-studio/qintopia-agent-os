# Contributing

## Development Flow

1. Create a branch from `master`.
2. Read `README.md`, `AGENTS.md`, and the target package README or manifest.
3. Keep the change scoped to one package/domain when possible.
4. Run local validation.
5. Update `CHANGELOG.md` for repository-level changes.
6. Update `docs/plans/active/monorepo-migration.md` when migration progress changes.
7. Open a PR with validation results and production-boundary notes.

## Local Setup

```bash
pnpm install
```

## Checks

```bash
pnpm check
```

Use package-specific checks in addition to repository-level checks when a package has
its own toolchain.

## Changelog Policy

Use `CHANGELOG.md` for repository-level changes that collaborators need to know.

When versioned packages are added, use Changesets for package release notes:

```bash
pnpm changeset
```

Do not use changelog entries as migration status. Migration progress belongs in
`docs/plans/active/monorepo-migration.md`.

## Production Boundary

PRs must say whether they touch any of these:

- external message sends
- database writes or migrations
- Hermes profile runtime
- systemd, nginx, or deploy scripts
- Feishu, QiWe, or other external integrations
- secrets or runtime configuration

Server changes must deploy an approved commit SHA. Do not edit code or docs directly on
the server.
