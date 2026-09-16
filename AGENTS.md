# Project Instructions

<!-- guidance-scope: . -->

## Project map

This is the Qintopia Agent OS capability/plugin monorepo. Organize work by the
capability being changed; implementation language does not define package ownership.

- [Human setup](README.md) and [documentation hub](docs/README.md).
- [Architecture](docs/architecture/agent-os-overview.md) and
  [product scope](docs/product/agent-os-prd.md).
- [Agent OS design](docs/agent-os/README.md).
- [Current roadmap](docs/plans/active/current-roadmap.md).
- [Runtime baseline](docs/operations/runtime-baseline.md) and
  [production status](docs/operations/production-current-status.md).
- [Change routing](docs/engineering/change-routing-index.md).
- [Engineering policy](docs/engineering/README.md).

Package placement:

| Directory     | Owns                                                      |
| ------------- | --------------------------------------------------------- |
| `agents/`     | Profiles, prompts, memory policy and allowed capabilities |
| `skills/`     | Reusable capabilities and channel integration             |
| `workflows/`  | Governed cross-Agent business processes                   |
| `mcp/`        | MCP adapters                                              |
| `runtime/`    | Runtime templates, contracts and validation               |
| `deploy/`     | Release, installation, smoke and rollback                 |
| `fixtures/`   | Replay and acceptance inputs                              |
| `registry/`   | Adopted package indexes                                   |
| `deprecated/` | Historical POC material, not new dependencies             |

## Task entry

Before non-trivial work, read the roadmap, change routing index and target package
README/manifest. Read the architecture or product documents when their boundaries are
involved. Use the nearest directory instructions in addition to this file.

| Work                                          | Required local rules                 |
| --------------------------------------------- | ------------------------------------ |
| Release, deployment or rollback               | [Deploy](deploy/AGENTS.md)           |
| Official Hermes core or Profile compatibility | [Hermes](runtime/hermes/AGENTS.md)   |
| Rust runtime, persistence or workers          | [Sidecar](runtime/sidecar/AGENTS.md) |
| QiWe ingress, reply or media                  | [QiWe](skills/qiwe/AGENTS.md)        |

Sibling instructions are not inherited automatically. For cross-directory work, read
every applicable entry and the topic contracts linked from change routing. Keep detailed
policy in its owning topic; do not copy it into multiple entries.

[Working contract](docs/engineering/agent-working-contract.md) retains the complete
worktree lifecycle, source maps, validation and collaboration clauses. Read its
applicable sections before worktree operations or changes to engineering policy.

Use the existing suitable checkout by default. Inspect branches, worktrees and local
changes before creating isolation. A new task or PR alone does not justify another
worktree. State the reason, path, branch and cleanup condition when isolation is needed;
preserve unrelated work and ignored evidence during cleanup.

Before editing, report the files read, intended changes, relevant validation and
production boundaries. Locate behavior through call sites, tests, logs and existing
contracts before inventing abstractions.

## Common commands

Run these from the repository root. Use `pnpm`; use the user-configured RTK wrapper when
it is present. Search with `rg` or `rg --files`.

| Purpose                    | Command                    |
| -------------------------- | -------------------------- |
| Install                    | `pnpm install`             |
| Format check               | `pnpm format:check`        |
| Markdown                   | `pnpm lint:md`             |
| Repository checks          | `pnpm check`               |
| Automatic PR tier          | `pnpm check:pr:auto`       |
| Quick PR tier              | `pnpm check:pr:quick`      |
| Heavy PR tier              | `pnpm check:pr:heavy`      |
| PR readiness               | `pnpm pr:doctor`           |
| PR body                    | `pnpm pr:check-body`       |
| Guidance and collaboration | `pnpm collaboration:check` |

[Business testing](docs/testing/agent-guide.md) defines the `pnpm test:*` harness. Run
targeted tests first. Let the automatic PR tier select broader checks; do not silently
replace required checks with a narrow test. Heavy checks require the reviewed disposable
local PostgreSQL setup. Package guides own detailed commands.

## General rules

- Do not develop directly on `master`; use a feature branch.
- Document first for features, behavior changes, migrations and runtime changes.
- Preserve unrelated changes, credentials and local evidence.
- Use Conventional Commits: `build`, `chore`, `ci`, `docs`, `feat`, `fix`, `perf`,
  `refactor`, `revert`, `style`, `test`.
- Do not introduce Java or another unapproved language/build stack without the explicit
  architecture decision required by the
  [programming guardrails](docs/engineering/programming-agent-guardrails.md).
- Do not introduce top-level language buckets. New abstractions need an owner, package
  contract and meaningful validation.
- New workflows must update the registry, workflow checker and restart routing.
- Keep Postgres/AgentOS as the system fact source. Feishu is a workbench and mirror;
  Hermes is the Agent runtime, not the business database.
- Do not build new workflows on WorkTool, OpenClaw, or Hermes Kanban. Historical
  material and proposed adoption remain subject to review.
- Follow the [package contract](docs/engineering/package-contract.md) and
  [migration policy](docs/engineering/migration-policy.md). Inventory before adoption;
  preserve source identity, classification, tests and production wiring boundaries.
- Do not manually edit root `CHANGELOG.md` in ordinary feature/fix PRs. Release Please
  owns routine changelog and version advancement.
- Follow the [collaboration model](docs/engineering/collaboration-model.md) for PR
  creation and release preparation. A low-risk classification or reviewer result does
  not authorize merge, release publication or deployment.
- [Historical compatibility rules](docs/operations/agent-guidance-history.md) retain
  their original conditions until separately reviewed; age is not deprecation.

## Production boundary

The server is a deployment target, not an editing workspace. Read the
[server change policy](docs/engineering/server-change-policy.md) before server work and
the deploy entry before release operations.

- No source hot edits or single-file production overwrites. Use reviewed immutable
  releases, validation evidence and concrete rollback procedures.
- Resolve server identity from the versioned inventory and administrator-provided
  connection details. Do not infer or search for private keys after authentication
  failure; that failure grants no new access.
- Do not copy live secrets, tokens, `.env` files, private messages, raw identity values
  or runtime databases into Git or retained public evidence.
- Keep profile runtime state distinct from distributable templates. Preserve
  credentials, sessions, memories, jobs and intentional channel enablement.
- External sends, permission expansion and live Profile replacement retain the explicit
  owner boundaries in the programming guardrails and owning package.
- Keep authentication, allowlists, feature gates, persistent enablement and owner
  approval checks at their actual enforcement boundaries. One is not a substitute for
  another.
- A timeout or lost acknowledgement does not prove that an external action failed.
  Preserve ambiguous outcomes and the owning workflow's no-replay/no-retry rules.
- Service `active`, cached `connected` and configuration readiness are not proof of
  user-visible success. Use the applicable acceptance gate and report what remains
  unverified.
- Release Please merge prepares a draft; owner publication remains the default
  production trigger. Preserve draft sequencing, current-master identity and exact-head
  checks in the release contract, including explicitly recorded exceptions.

## Completion criteria

Run package tests, fixtures and required registry/manifest checks. For production-
adjacent changes, retain dry-run and rollback evidence and identify external sends,
database writes, profiles, integrations and systemd boundaries.

Use the user's browser tool preference for user-visible verification. Report an
unavailable browser or test environment as a limitation, never as a pass.

Validate the completed PR body. Report changes, commands and results, material failed
attempts, remaining acceptance items and scope boundaries. Do not claim a production
workflow complete until its own evidence gate passes.

Record production/deployment/preflight/CI integration failures in an indexed dated
report and update the affected runbook or check in the same remediation PR. Keep current
operational facts in status/reports; detailed procedures belong to package or
operational documents, not this entrypoint.

For instruction changes, maintain the
[rule migration inventory](docs/plans/active/agents-guidance/README.md). Root guidance
has a 16 KiB limit and local entries 12 KiB each; do not evade this with long lines.
Preserve conditions and exceptions when moving rules. The checker establishes coverage
and navigation, not semantic equivalence or production safety.
