# Engineering

This directory contains stable engineering rules for how Qintopia Agent OS is developed,
reviewed, migrated, and deployed.

## Documents

- [collaboration-model.md](collaboration-model.md): git-first workflow, programming
  agent read order, PR requirements, and CI/CD direction.
- [package-contract.md](package-contract.md): required package files, metadata fields,
  and placement rules.
- [migration-policy.md](migration-policy.md): migration dispositions, required migration
  records, and WorkTool cleanup policy.
- [anti-drift-policy.md](anti-drift-policy.md): executable guardrails that keep
  deprecated paths, review-pool work, and legacy deploy scripts from becoming approved
  direction accidentally.
- [server-change-policy.md](server-change-policy.md): allowed and disallowed server
  activity.

## Rules

- Keep transient status out of root README, `AGENTS.md`, and `CLAUDE.md`.
- Put active migration state in
  [../plans/active/monorepo-migration.md](../plans/active/monorepo-migration.md).
- Put source inventories and runtime evidence under [../operations/](../operations/).
- Add executable checks when package changes make that practical.
- Add anti-drift policy checks when a migration boundary must be enforced for all future
  contributors.
