# Collaboration Model

This repository is designed for human engineers and programming agents such as Codex and
Claude Code. The collaboration model is intentionally git-first and CI-backed.

## Default Flow

1. Branch from `master`.
2. Read the root entrypoint, agent instructions, and relevant package documentation.
3. Read [programming-agent-guardrails.md](programming-agent-guardrails.md) and the
   current roadmap before broad work.
4. Document first for new features, behavior changes, migrations, or runtime changes.
5. Localize the change to one domain or package when possible.
6. Update package docs or manifests when the package contract changes.
7. Run focused validation, then repository checks.
8. Commit with a Conventional Commits message such as `feat: add weather skill` or
   `fix: resolve qintopia-tools skill path`.
9. Update the current roadmap or package docs when future direction changes.
10. Open a PR with validation results and production-boundary notes.
11. Deploy only reviewed commit SHAs through a documented runbook.

Do not develop directly on `master`. CI runs on `master` after merge; feature work
belongs on a branch.

## Programming Agent Read Order

For Codex, Claude Code, or another programming agent:

1. `README.md` or `README.zh-CN.md`
2. `AGENTS.md` or `AGENTS.zh-CN.md`
3. `CLAUDE.md` or `CLAUDE.zh-CN.md` when using Claude Code
4. [Documentation hub](../README.md)
5. [architecture/agent-os-overview.md](../architecture/agent-os-overview.md)
6. [../plans/active/current-roadmap.md](../plans/active/current-roadmap.md)
7. [programming-agent-guardrails.md](programming-agent-guardrails.md)
8. [change-routing-index.md](change-routing-index.md)
9. The target package README or manifest
10. The completed migration archive when historical migration evidence is needed

Agents should report what they read, what they plan to touch, validation commands, and
production boundaries before broad changes.

## PR Requirements

Every PR should state whether it touches:

- external message sends
- database writes or migrations
- Hermes profile runtime
- systemd, nginx, or deploy scripts
- Feishu, QiWe, or other external integrations
- secrets or runtime configuration

Docs-only PRs should still run `pnpm check`.

## Commit Message Requirements

Use Conventional Commits. Allowed commit types are:

```text
build chore ci docs feat fix perf refactor revert style test
```

Choose the type by the primary intent of the commit. Do not create project-specific or
free-form types. The local `commit-msg` hook and CI both run commitlint.

## Changelog And Release Preparation

Routine feature and fix PRs should not edit root `CHANGELOG.md`. Release Please uses
merged Conventional Commits to maintain a release PR with the generated changelog and
release manifest changes.

This repository treats release infrastructure as operator-visible product behavior.
Release Please is configured to include `ci:`, `build:`, `docs:`, and `chore:` entries
when they affect release, deploy, collaboration, or operating procedures. Pure `test:`
and `style:` commits stay hidden from release notes.

When a version is ready, the owner reviews and merges the Release Please PR. That merge
prepares the version and creates a draft GitHub Release. It does not deploy production.
Production deployment still requires manually publishing the draft GitHub Release, which
triggers the `release.published` deploy workflow.

If the Release Please PR stays open while more feature PRs merge, Release Please updates
the same release PR. Avoid editing root `CHANGELOG.md` or
`.release-please-manifest.json` from ordinary PRs to keep that release PR conflict-free.

## Language And Toolchain Boundary

The repository currently allows TypeScript or JavaScript, Python, Rust, shell, SQL,
YAML, JSON, and Markdown. Do not add Java, Gradle, Maven, Kotlin, Go, Swift, C#, PHP,
Ruby, Elixir, or another implementation/toolchain stack without an explicit
owner-approved architecture decision.

Do not create top-level language buckets. Organize by Agent OS capability and keep the
implementation language inside the owning package.

## CI/CD Gate

The current CI gate runs the repository check path:

- formatting and Markdown linting
- manifest and registry validation
- package-level checks and fixture-backed smoke checks
- anti-drift policy checks
- secret and runtime-state scanning
- deployment preflight checks

Release jobs should deploy a reviewed commit SHA. They should not copy local uncommitted
files or hot-edit server checkouts.

See [ci-cd-gates.md](ci-cd-gates.md) for the executable gate list.
