# Contributing

## Development Flow

1. Create a branch from `master`.
2. Read `README.md`, `AGENTS.md`, and the target package README or manifest.
3. Read `docs/plans/active/current-roadmap.md` and
   `docs/engineering/programming-agent-guardrails.md`.
4. Document first for new features, behavior changes, migrations, or runtime changes.
5. Keep the change scoped to one package/domain when possible.
6. Run local validation.
7. Commit with a Conventional Commits message.
8. Update the current roadmap or a package doc when the future direction changes.
9. Validate PR readiness with `pnpm pr:doctor`.
10. Open a PR with `pnpm pr:create -- --body-file <completed-pr-body.md>`.

Do not develop directly on `master`.

Do not introduce Java, Gradle, Maven, Kotlin, Go, Swift, C#, PHP, Ruby, Elixir, or a new
language/toolchain stack without an explicit owner-approved architecture decision.

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

## Commit Message Policy

Use Conventional Commits. Allowed types are:

```text
build chore ci docs feat fix perf refactor revert style test
```

Do not invent custom types. Use the type that matches the primary change:

- `feat`: new product, package, runtime, or workflow capability
- `fix`: bug fix, broken validation, runtime path issue, or incorrect behavior
- `docs`: documentation-only change
- `ci`: GitHub Actions, CI scripts, or commit/check gates
- `test`: tests or fixtures only
- `refactor`: behavior-preserving code reshaping
- `chore`: repository maintenance without product behavior change
- `build`: dependency, packaging, or artifact build system change

Local commits are checked by the Husky `commit-msg` hook. CI runs
`pnpm commitlint:check` against PR commits.

## Pull Request Policy

Do not use a prefilled GitHub compare URL as the normal PR creation path. Use the
repository-owned CLI flow so Codex, Claude Code, and human contributors produce the same
PR shape:

```bash
pnpm pr:doctor
pnpm pr:create -- --body-file <completed-pr-body.md>
```

If GitHub CLI is missing, run:

```bash
pnpm pr:bootstrap
```

`pnpm pr:bootstrap -- --install` may install `gh` on supported environments.
Authentication still requires `gh auth login`.

Start the PR body from `.github/PULL_REQUEST_TEMPLATE.md`. Fill Summary, Planning,
Domain, Validation, Production Boundary, Architecture / Tooling Boundary, and Changelog.
Include production-boundary notes for every runtime, deploy, external integration, or
database-adjacent change. CI rejects empty template bodies with `pnpm pr:check-body`.

## Changelog And Release Policy

Use Conventional Commits as the primary input for repository-level changelog entries.
Release Please owns routine root `CHANGELOG.md` updates through a release PR.

Do not edit root `CHANGELOG.md` in ordinary feature or fix PRs. If a changelog
correction is required, make it in the Release Please PR or in a dedicated docs PR that
explains why the generated entry was wrong.

Release flow:

1. Feature and fix PRs merge into `master`.
2. Release Please opens or updates a release PR that updates `CHANGELOG.md` and the
   release manifest.
3. The owner reviews and merges the release PR when a version is ready.
4. Release Please creates a draft GitHub Release.
5. The owner manually publishes that draft Release.
6. The existing `release.published` workflow builds artifacts, uploads them to COS, and
   creates the production deploy request.

The production deploy trigger remains manual Release publication. Merging a release PR
prepares a version; it does not deploy production by itself.

When versioned packages are added, use Changesets for package release notes:

```bash
pnpm changeset
```

Do not use changelog entries as active migration status. The completed monorepo
migration record is archived at `docs/plans/completed/monorepo-migration.md`; current
work belongs in `docs/plans/active/current-roadmap.md` or a package-specific plan.

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
