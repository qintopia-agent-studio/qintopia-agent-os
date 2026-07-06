# Engineering

This directory contains stable engineering rules for how Qintopia Agent OS is developed,
reviewed, migrated, and deployed.

## Documents

- [collaboration-model.md](collaboration-model.md): git-first workflow, programming
  agent read order, PR requirements, and CI/CD direction.
- [programming-agent-guardrails.md](programming-agent-guardrails.md): concrete branch,
  documentation-first, language/toolchain, package, production-boundary, and
  stop-condition rules for Codex, Claude Code, and similar agents.
- [package-contract.md](package-contract.md): required package files, metadata fields,
  and placement rules.
- [migration-policy.md](migration-policy.md): migration dispositions, required migration
  records, and WorkTool cleanup policy.
- [anti-drift-policy.md](anti-drift-policy.md): executable guardrails that keep
  deprecated paths, review-pool work, and legacy deploy scripts from becoming approved
  direction accidentally.
- [ci-cd-gates.md](ci-cd-gates.md): repository checks, secret scanning, deployment
  preflight, and CI requirements.
- [pr-agent-review.md](pr-agent-review.md): AI-assisted PR review scope, trigger model,
  required secret, and merge boundary.
- [server-change-policy.md](server-change-policy.md): allowed and disallowed server
  activity.

## Rules

- Keep transient status out of root README, `AGENTS.md`, and `CLAUDE.md`.
- Put current direction in
  [../plans/active/current-roadmap.md](../plans/active/current-roadmap.md).
- Put historical migration evidence in
  [../plans/completed/monorepo-migration.md](../plans/completed/monorepo-migration.md).
- Put source inventories and runtime evidence under [../operations/](../operations/).
- Add executable checks when package changes make that practical.
- Add anti-drift policy checks when a migration boundary must be enforced for all future
  contributors.
