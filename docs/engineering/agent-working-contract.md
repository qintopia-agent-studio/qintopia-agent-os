# Working contract

These are retained operating constraints, not a new permission grant. Read the sections
relevant to the change before editing. Original conditions and exceptions remain
binding; summaries in AGENTS.md do not replace them.

Backtick paths in root-source rules are repository-relative; paths in Sidecar-source
rules retain their original `runtime/sidecar/` base. New navigation links are relative
to this document.

[Migration inventory](../plans/active/agents-guidance/README.md) records the baseline
and source locations.

<a id="root-001"></a>

## Map — root-001

<!-- preserved-rule: root-001 -->

- Human entrypoint: `README.md`
- Agent-facing rules: `AGENTS.md`
- Claude Code rules: `CLAUDE.md`
- Documentation hub: `docs/README.md`
- Architecture overview: `docs/architecture/agent-os-overview.md`
- Product scope: `docs/product/agent-os-prd.md`
- Agent OS design: `docs/agent-os/README.md`
- Runtime baseline: `docs/operations/runtime-baseline.md`
- Production current status: `docs/operations/production-current-status.md`
- Production evidence runbook: `docs/operations/xiaoman-production-evidence-runbook.md`
- Xiaoman weekly minimum loop runbook:
  `docs/operations/xiaoman-weekly-minimum-loop-runbook.md`
- Collaboration model: `docs/engineering/collaboration-model.md`
- Migration policy: `docs/engineering/migration-policy.md`
- Server change policy: `docs/engineering/server-change-policy.md`
- Programming agent guardrails: `docs/engineering/programming-agent-guardrails.md`
- Change routing index: `docs/engineering/change-routing-index.md`
- Current roadmap: `docs/plans/active/current-roadmap.md`
- Xiaoman character-universe daily report migration:
  `docs/plans/active/xiaoman-character-universe-daily-report.md`
- Source document inventory: `docs/operations/source-document-inventory.md`
- Registry indexes: `registry/`
- Agent packages: `agents/`
- Skill packages: `skills/`
- Workflow packages: `workflows/`
- MCP adapters: `mcp/`
- Runtime templates: `runtime/`
- Deployment scripts and manifests: `deploy/`
- Engineering docs: `docs/engineering/`
- Operations docs: `docs/operations/`
- Fixtures and replay data: `fixtures/`
- Historical POC material: `deprecated/`
- Local business testing guide: `docs/testing/README.md`

<!-- /preserved-rule: root-001 -->

<a id="root-002"></a>

## Worktree Lifecycle — root-002

<!-- preserved-rule: root-002 -->

- Reuse the project root or an existing suitable worktree by default, developing on a
  feature branch. A new task, branch, or PR alone does not justify another worktree.

<!-- /preserved-rule: root-002 -->

<a id="root-003"></a>

## Worktree Lifecycle — root-003

<!-- preserved-rule: root-003 -->

- Create one only for explicit user-requested isolation, parallel work requiring
  independent file states, uncommitted work that blocks safe progress, or simultaneous
  execution of different versions. Inspect `git worktree list`, branches, and local
  changes first; unrelated dirty files alone do not require isolation.

<!-- /preserved-rule: root-003 -->

<a id="root-004"></a>

## Worktree Lifecycle — root-004

<!-- preserved-rule: root-004 -->

- Before creating one, state the concrete reason, path, branch, and cleanup condition.
  Proceed within existing task authorization without adding a step-by-step approval
  gate. Put manually managed worktrees in the target repository's ignored
  `.worktrees/<task>/`, not long-lived `/tmp` or `/private/tmp` directories. Preserve
  user-specified or app-managed directory conventions.

<!-- /preserved-rule: root-004 -->

<a id="root-005"></a>

## Worktree Lifecycle — root-005

<!-- preserved-rule: root-005 -->

- At handoff, check actual PR merge status (including squash merges), local changes,
  untracked and ignored configuration/evidence, and running previews. Retire only this
  task's completed, recoverable worktrees; retain pending acceptance work with its
  purpose recorded. Archive local material under ignored `.local-workspace/`; never
  commit credentials or business snapshots with source changes.

<!-- /preserved-rule: root-005 -->

<a id="root-006"></a>

## Worktree Lifecycle — root-006

<!-- preserved-rule: root-006 -->

- PR merge, task archival, and worktree cleanup are separate actions. Preserve all local
  work before restoring the ordinary project entrypoint to current `master` without
  disrupting another task. Never force-overwrite work or delete historical branches as
  an implicit side effect.

<!-- /preserved-rule: root-006 -->

<a id="root-007"></a>

## Worktree Lifecycle — root-007

<!-- preserved-rule: root-007 -->

- For cross-repository work, resolve the target Git root and read its `AGENTS.md`. Codex
  project grouping does not change filesystem ancestry or Git boundaries. When operating
  on Green PMS, follow its worktree policy and `main`/PR requirements too.

<!-- /preserved-rule: root-007 -->

<a id="root-008"></a>

## Commands — root-008

<!-- preserved-rule: root-008 -->

- Install dependencies: `pnpm install`

<!-- /preserved-rule: root-008 -->

<a id="root-009"></a>

## Commands — root-009

<!-- preserved-rule: root-009 -->

- Format: `pnpm format`

<!-- /preserved-rule: root-009 -->

<a id="root-010"></a>

## Commands — root-010

<!-- preserved-rule: root-010 -->

- Pre-commit quick checks: `.husky/pre-commit`

<!-- /preserved-rule: root-010 -->

<a id="root-011"></a>

## Commands — root-011

<!-- preserved-rule: root-011 -->

- Repository check: `pnpm check`

<!-- /preserved-rule: root-011 -->

<a id="root-012"></a>

## Commands — root-012

<!-- preserved-rule: root-012 -->

- Markdown lint: `pnpm lint:md`

<!-- /preserved-rule: root-012 -->

<a id="root-013"></a>

## Commands — root-013

<!-- preserved-rule: root-013 -->

- Local business testing: read `docs/testing/agent-guide.md`; use the `pnpm test:*`
  entries backed by `node tools/testing/run.mjs` for setup, discovery, targeted runs,
  full runs, reports, and harness checks.

<!-- /preserved-rule: root-013 -->

<a id="root-026"></a>

## Commands — root-026

<!-- preserved-rule: root-026 -->

- PR readiness: `pnpm pr:doctor`

<!-- /preserved-rule: root-026 -->

<a id="root-027"></a>

## Commands — root-027

<!-- preserved-rule: root-027 -->

- PR body validation: `pnpm pr:check-body`

<!-- /preserved-rule: root-027 -->

<a id="root-028"></a>

## Commands — root-028

<!-- preserved-rule: root-028 -->

- Local PR quick tier: `pnpm check:pr:quick`

<!-- /preserved-rule: root-028 -->

<a id="root-029"></a>

## Commands — root-029

<!-- preserved-rule: root-029 -->

- Local PR heavy tier: `pnpm check:pr:heavy`

<!-- /preserved-rule: root-029 -->

<a id="root-030"></a>

## Commands — root-030

<!-- preserved-rule: root-030 -->

- Local PR auto tier: `pnpm check:pr:auto`

<!-- /preserved-rule: root-030 -->

<a id="root-064"></a>

## Commands — root-064

<!-- preserved-rule: root-064 -->

- PR creation: `pnpm pr:create -- --body-file <completed-pr-body.md>`

<!-- /preserved-rule: root-064 -->

<a id="root-070"></a>

## Commands — root-070

<!-- preserved-rule: root-070 -->

- If the local pnpm version shim cannot verify a registry signature, do not set
  `pmOnFail=ignore`. Confirm the exact `package.json` script first; when it is a fixed
  repository-local Node entrypoint, run that entrypoint directly and record the failed
  pnpm validation attempt. In Codex's restricted sandbox, pnpm 10/11 package-manager
  switching can also fail because Node fetch to npm and writes to the user-level
  `PNPM_HOME/.tools` cache are blocked; verify once with the reviewed non-sandbox
  boundary before treating the error as lockfile tampering.

<!-- /preserved-rule: root-070 -->

<a id="root-135"></a>

## Core Rules — root-135

<!-- preserved-rule: root-135 -->

- A `group_message_send` claim must clear `claimed_by`, `locked_at`, and
  `claim_expires_at` together when it records send-ready or policy-denied state. The
  transition must update exactly the locked work item before appending its audit event.

<!-- /preserved-rule: root-135 -->

<a id="root-138"></a>

## Core Rules — root-138

<!-- preserved-rule: root-138 -->

- Do not develop directly on `master`; create a feature branch first.

<!-- /preserved-rule: root-138 -->

<a id="root-140"></a>

## Core Rules — root-140

<!-- preserved-rule: root-140 -->

- Use Conventional Commits for commit messages. Allowed types are `build`, `chore`,
  `ci`, `docs`, `feat`, `fix`, `perf`, `refactor`, `revert`, `style`, and `test`.

<!-- /preserved-rule: root-140 -->

<a id="root-152"></a>

## Core Rules — root-152

<!-- preserved-rule: root-152 -->

- Do not hand humans a prefilled GitHub compare URL as the normal PR flow. Use
  `pnpm pr:doctor`, then `pnpm pr:create` with a completed PR body. If GitHub CLI is
  missing, run `pnpm pr:bootstrap`.

<!-- /preserved-rule: root-152 -->

<a id="root-153"></a>

## Core Rules — root-153

<!-- preserved-rule: root-153 -->

- In the Codex desktop environment, do not run extra GitHub authentication checks before
  creating a PR. Use `pnpm pr:create` directly after PR readiness checks; only handle
  authentication when the actual push or PR creation command fails.

<!-- /preserved-rule: root-153 -->

<a id="root-154"></a>

## Core Rules — root-154

<!-- preserved-rule: root-154 -->

- In Codex sandboxed command execution, a repo-owned Node PR script can fail when its
  child `gh` process reaches `api.github.com` even though a top-level `gh pr ...`
  command works. Treat that as sandbox network permission, not an auth failure; rerun
  the repo-owned PR entrypoint with network approval instead of re-authenticating `gh`.

<!-- /preserved-rule: root-154 -->

<a id="root-155"></a>

## Core Rules — root-155

<!-- preserved-rule: root-155 -->

- PR-Agent must not automatically edit PR descriptions. The completed repository PR
  template is author-owned because CI validates its required sections.

<!-- /preserved-rule: root-155 -->

<a id="root-156"></a>

## Core Rules — root-156

<!-- preserved-rule: root-156 -->

- Before merging any PR, read the complete PR Reviewer Guide, submitted reviews,
  conversation comments, and inline review threads for the latest head SHA. A green
  PR-Agent check is not sufficient. Resolve every security concern and recommended
  review item in code or record an explicit disposition, then wait for replacement CI
  and review results before merge.

<!-- /preserved-rule: root-156 -->

<a id="root-157"></a>

## Core Rules — root-157

<!-- preserved-rule: root-157 -->

- Do not introduce Java, Gradle, Maven, Kotlin, Go, Swift, C#, PHP, Ruby, Elixir, or a
  new language/toolchain stack without an explicit owner-approved architecture decision.

<!-- /preserved-rule: root-157 -->

<a id="root-168"></a>

## Core Rules — root-168

<!-- preserved-rule: root-168 -->

- Do not copy secrets, live `.env` files, tokens, table ids, private chat logs, raw
  member profiles, or server-only runtime state into git.

<!-- /preserved-rule: root-168 -->

<a id="root-169"></a>

## Core Rules — root-169

<!-- preserved-rule: root-169 -->

- WorkTool is not a Qintopia Agent OS channel for new work. Treat WorkTool and the
  WorkTool Hermes plugin as deprecated or audit-only material.

<!-- /preserved-rule: root-169 -->

<a id="root-170"></a>

## Core Rules — root-170

<!-- preserved-rule: root-170 -->

- Hermes Kanban is not the future task/orchestration backbone. Do not build new
  workflows on Hermes Kanban.

<!-- /preserved-rule: root-170 -->

<a id="root-201"></a>

## Core Rules — root-201

<!-- preserved-rule: root-201 -->

- Evidence and visual worker reports must derive `dry_run` from `apply_requested` so a
  `--dry-run` observation cannot report `dry_run=false`; preflight must fail closed on
  any mismatch rather than weakening that assertion.

<!-- /preserved-rule: root-201 -->

<a id="root-250"></a>

## Core Rules — root-250

<!-- preserved-rule: root-250 -->

- A content-hash conflict may reuse an existing pending `generated_image` only when its
  stable URI, source refs, and complete immutable worker metadata exactly match the new
  final JPEG result. Reviewed, stale, or modified artifacts must fail closed and must
  never be overwritten by retry processing.

<!-- /preserved-rule: root-250 -->

<a id="root-279"></a>

## Package Placement — root-279

<!-- preserved-rule: root-279 -->

- Agent profile, prompt, allowed skills, memory policy, and forbidden actions:
  `agents/<agent>/`.
- Reusable channel or business capability: `skills/<capability>/`.
- Cross-Agent business process: `workflows/<workflow>/`.
- MCP server or adapter: `mcp/<adapter>/`.
- Runtime template or render/check logic: `runtime/<runtime-area>/`.
- Release, smoke, rollback, or server install logic: `deploy/<area>/`.
- Historical POC or removed direction: `deprecated/<topic>/`.

<!-- /preserved-rule: root-279 -->

<a id="root-280"></a>

## Package Contract — root-280

<!-- preserved-rule: root-280 -->

Every adopted package should eventually include:

- `README.md`
- `manifest.yaml`, `agent.yaml`, or `workflow.yaml`
- `tests/` or `fixtures/`
- owner and risk level
- validation command
- production boundary
- rollback or decommission notes when relevant

Do not migrate a package as production-ready until these are present or there is a
documented exception.

<!-- /preserved-rule: root-280 -->

<a id="root-281"></a>

## Migration Rules — root-281

<!-- preserved-rule: root-281 -->

Migration is inventory-first:

1. Identify the current source path.
2. Record whether it is `adopt`, `template`, `runtime-only`, `deprecated`, or `remove`.
3. Preserve source hash or commit reference.
4. Add package metadata.
5. Add focused tests or fixtures.
6. Only then wire it into registry and deployment.

Server runtime directories under `.hermes/profiles/*` must be treated as live runtime
state. They can produce inventory records, templates, or diffs; they must not be copied
wholesale into this repository.

<!-- /preserved-rule: root-281 -->

<a id="root-282"></a>

## Server Change Policy — root-282

<!-- preserved-rule: root-282 -->

The server is a deployment target, not an editing workspace.

Allowed server activity:

- read-only inventory
- service status checks
- log inspection
- smoke checks
- deploying an approved commit SHA through a runbook
- emergency rollback with a follow-up patch and owner record

Resolve the production endpoint and identity from the versioned
`docs/operations/inventory/server-sources.yaml` record and connection details supplied
by the administrator. SSH hostnames, usernames, aliases, key paths, and known-hosts
locations are not stored in this repository. When SSH is required, contact the
administrator for the current authorized connection details; do not infer, hard-code, or
search for private keys. An authentication failure is not authorization to inspect,
copy, or change private keys.

Disallowed server activity:

- editing docs directly
- editing code directly
- editing `.hermes` runtime files directly
- scp overwrites of single source files
- committing unreviewed experiments on the server and treating them as product direction

<!-- /preserved-rule: root-282 -->

<a id="root-283"></a>

## Validation Expectations — root-283

<!-- preserved-rule: root-283 -->

Before a PR:

- Run package-level tests.
- Run fixture replay when available.
- Run registry/manifest checks when available.
- Validate the completed PR body with `pnpm pr:check-body` or `pnpm pr:doctor`.
- For runtime/deploy changes, include dry-run output and rollback notes.
- For user-facing HTML reports, run HTML parse and browser overflow checks.
- For production-adjacent changes, state whether the change touches external sends,
  database writes, profile runtime, secrets, Feishu, QiWe, or systemd.

<!-- /preserved-rule: root-283 -->

<a id="root-284"></a>

## Documentation Rules — root-284

<!-- preserved-rule: root-284 -->

- Keep decisions in git, not only in chat.
- For every production, deploy, preflight, or CI integration failure, add or update a
  dated, indexed record under `docs/reports/` in the same PR. Include the observed
  evidence, root cause, resolution, validation, remaining boundary, and follow-up owner
  action. Update affected runbooks, package READMEs, manifests, or checks in that same
  PR; do not leave the repair documented only in a report or chat.
- Prefer short, focused docs over one large manual.
- Mark server-side exploration as unapproved until owner review confirms it.
- Avoid formalistic phrasing when writing internal engineering docs.
- Keep technical reports concrete: current state, evidence, risk, next action.

<!-- /preserved-rule: root-284 -->

<a id="root-285"></a>

## First Read For New Agents — root-285

<!-- preserved-rule: root-285 -->

1. `README.md`
2. `AGENTS.md`
3. `docs/README.md`
4. `docs/architecture/agent-os-overview.md`
5. `docs/plans/active/current-roadmap.md`
6. `docs/engineering/programming-agent-guardrails.md`
7. `docs/engineering/change-routing-index.md`
8. `docs/product/agent-os-prd.md` for product scope changes
9. `docs/agent-os/README.md` for Agent OS design changes
10. `docs/plans/completed/monorepo-migration.md` for historical migration evidence
11. Target package README or manifest
12. Relevant docs under `docs/engineering/` or `docs/operations/`

Report what you read, what you plan to touch, validation commands, and production
boundaries before making broad changes.

<!-- /preserved-rule: root-285 -->

<a id="sidecar-001"></a>

## Map — sidecar-001

<!-- preserved-rule: sidecar-001 -->

- Human setup and usage: `README.md`
- Source snapshot: `docs/source-snapshot.md`
- Database migrations: `../postgres/migrations/`
- Versioned data design docs: `../postgres/docs/data-design/`
- Server deployment scripts: `../../deploy/sidecar/scripts/`
- Current cutover runbook: `../../docs/operations/m9-server-cutover-runbook.md`
- Target server directory plan: `../../docs/operations/server-directory-plan.md`
- Legacy standalone deployment snapshot:
  `../../deploy/sidecar/docs/server-deployment.md`
- Sidecar entrypoint: `src/main.rs`
- NATS consumer loop: `src/consumer.rs`
- Postgres persistence: `src/db.rs`
- Event protocol parsing: `src/event.rs`

<!-- /preserved-rule: sidecar-001 -->
