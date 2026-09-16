# Programming Agent Guardrails

This document tells Codex, Claude Code, and similar programming agents how to work in
this repository without drifting away from the Agent OS architecture.

## Start Protocol

Before editing, an agent must read:

1. `README.md` or `README.zh-CN.md`
2. `AGENTS.md` or `AGENTS.zh-CN.md`
3. `docs/README.md` or `docs/README.zh-CN.md`
4. `docs/plans/active/current-roadmap.md`
5. `docs/engineering/change-routing-index.md`
6. the target package README or manifest

For runtime, server, deployment, Hermes profile, Feishu, QiWe, or database work, also
read the relevant document under `docs/operations/` or `docs/engineering/`.

## Branch Rule

Do not develop directly on `master`.

- Create a branch before editing.
- Keep each branch scoped to one package, domain, or documented plan.
- CI may run on `master` after merge, but local feature work should not happen there.

## Commit Message Rule

Use Conventional Commits for every commit. Allowed types are:

```text
build chore ci docs feat fix perf refactor revert style test
```

Use `feat` for new capabilities, `fix` for bug fixes, `docs` for documentation-only
changes, `ci` for CI/check gates, `test` for tests or fixtures, `refactor` for
behavior-preserving code movement, `build` for dependency or artifact tooling, and
`chore` for maintenance. Do not invent custom types. Commit messages are checked by the
local `commit-msg` hook and by CI.

## Documentation-First Rule

For new features, behavior changes, migrations, or runtime changes:

1. Write or update the relevant doc first.
2. State the domain, goal, scope, production boundary, and validation path.
3. Then implement code.
4. Update the doc again if implementation changes the design.

Small typo fixes and purely mechanical formatting do not need a design note.

## Language And Toolchain Rule

Allowed implementation families are the ones already used by this repository:

- TypeScript or JavaScript
- Python
- Rust
- shell scripts
- SQL
- YAML, JSON, and Markdown for configuration and docs

Do not introduce Java, Gradle, Maven, Kotlin, Go, Swift, C#, PHP, Ruby, Elixir, or
another new language/toolchain stack without an explicit owner-approved architecture
decision.

Implementation language is always package-internal. Do not create top-level language
directories.

## Package Rule

New behavior belongs in a package:

- Agent profile behavior: `agents/<agent>/`
- reusable capability: `skills/<capability>/`
- cross-Agent process: `workflows/<workflow>/`
- MCP adapter/server: `mcp/<adapter>/`
- runtime templates/checks: `runtime/<area>/`
- release/smoke/rollback: `deploy/<area>/`
- historical or retired material: `deprecated/<topic>/`

Every package should have a README, manifest, validation command, owner/risk metadata,
and production-boundary note.

## Production Boundary Rule

Do not change production behavior casually. Explicitly call out whether the PR touches:

- external message sends
- database writes or migrations
- Hermes profile runtime
- systemd, nginx, deploy scripts, or release promotion
- Feishu, QiWe, Tencent COS, GitHub Actions, or other external integrations
- secrets or runtime config

Server edits must go through reviewed artifacts and runbooks. Do not hot-edit server
code, docs, `.hermes` files, or systemd units.

## PR Review Automation Rule

PR-Agent is an advisory reviewer only.

- Treat PR-Agent comments as review input, not approval.
- Do not use PR-Agent output to bypass CI, CODEOWNERS, branch protection, or owner
  review.
- If PR-Agent suggests an architecture direction that conflicts with repository docs,
  follow the repository docs and update the docs only through a normal PR.
- See `docs/engineering/pr-agent-review.md` for the workflow boundary.

## PR Creation Rule

Programming agents must create PRs through the repository-owned GitHub CLI flow, not by
handing a human a prefilled GitHub compare URL.

Use:

```bash
pnpm pr:doctor
pnpm pr:create -- --body-file <completed-pr-body.md>
```

The body file must start from `.github/PULL_REQUEST_TEMPLATE.md` and fill every required
section. CI runs `pnpm pr:check-body` on pull requests and rejects empty template
bodies.

If `gh` is missing, run `pnpm pr:bootstrap` to print supported installation commands. On
supported environments, `pnpm pr:bootstrap -- --install` may install GitHub CLI.
Authentication still requires `gh auth login` only when the actual PR flow reports an
authentication failure.

In the Codex desktop environment, do not run extra GitHub authentication checks before
creating a PR. Use `pnpm pr:create` directly after PR readiness checks; only handle
authentication when the actual push or PR creation command fails.

### Conversational QiWe extension runner

The default-disabled Space programming-extension runner is narrower than an ordinary
interactive programming session:

- it claims only `space_programming_extension_request` work assigned to
  `programming_agent` through the local operations-intake socket;
- its claim and Codex child environment contain no Space, room, member, actor or message
  identifiers and no production, database, channel, deployment, COS, or GitHub
  credentials;
- it works from exact `origin/master` in a disposable worktree and accepts only the
  append-only QiWe event-mapping paths enforced by the repository low-risk classifier;
- when the existing mapping DSL is insufficient, it may add one declarative
  `*.primitive.json` recipe that composes only the fixed parser kernel, alongside the
  mapping, synthetic fixture, and canonical expectation; it may never add executable
  parser source or a new kernel operation;
- it may add one fixed-format `*.mapping.md` summary that references only the same
  mapping, fixture, expectation, definition key, and declarative scope;
- the complete append-only bundle is capped at five files;
- it runs the repository-owned fixed validation set and uses `pnpm pr:create` for a
  PR-only handoff;
- its parent startup environment must not contain a GitHub token; only after Codex has
  exited and the allowed paths, complete committed diff, fixed validation, low-risk
  classification, and clean worktree state pass may it invoke the fixed short-lived
  token helper, authenticate the first remote fetch, and continue only if fetched
  `origin/master` still equals the audited local base;
- it uses the `qintopia-programming-agent/` branch prefix and a `feat(qiwe):` commit and
  PR title, then stops after creating the ordinary PR.

A successful PR handoff leaves the originating work item in `awaiting_publish` with
phase `pr_created`; PR creation is not extension completion. The broker binds the
original work-item Space and request digest to the generated mapping key, exact mapping
source digest, candidate commit, and PR number. Current-Space status exposes only the PR
number and short candidate/mapping fingerprints, never the PR URL, branch, complete
commit SHA, or external response.

After a Release is active, the trusted status path advances the request only when the
current sidecar's embedded registry contains that exact mapping source digest and the
service has a valid deploy-injected `QINTOPIA_DEPLOYED_COMMIT_SHA`. It then reports
`release_phase=released` and `phase=ready_to_replan`. The trusted public status wrapper
uses an internal same-Space operation to retrieve the retained intent, reruns the
bounded planner, and idempotently creates the ordinary shadow proposal. The original
intent is not included in the model-facing status response, the original request id
cannot continue from another Space, and administrator confirmation remains mandatory.

The runner must not merge any PR, publish a Release, deploy, send, retry an expired
claim or accept caller-supplied commands, paths, URLs, labels, branches or validation
steps. Separate child-process environments alone do not isolate credentials from code
running under the same Unix UID. Production dispatch must remain disabled until Codex
runs under a dedicated OS identity or equivalent container with no read access to
production env, Hermes, COS, database, server, or GitHub credentials; the PR
orchestration boundary must hold the GitHub token outside that sandbox.

Manual owner review is mandatory after this handoff. Low-risk classification constrains
the generated diff but never authorizes merge or publication. Any new permission,
dependency, migration, authentication, encryption, send path, deployment change, or file
outside the mapping/fixture/expectation/optional primitive/optional mapping-summary
allowlist stops for owner review.

The restricted recipe contract is documented in `qiwe-restricted-parser-primitives.md`.
A provider encoding outside that fixed kernel is an owner-reviewed runtime extension,
not an automatic exception.

## Hermes Profile Rule

Hermes profile live state is not source code.

Keep these outside git:

- `.env`
- sessions, logs, cache, pairing, auth, locks
- generated memory and state databases
- private chat logs and raw member profile data

Treat reviewed profile distribution files such as `SOUL.md`, skills, cron, and MCP
declarations as future bundle inputs, not as live server state to copy wholesale.

## Stop Conditions

Stop and ask for owner confirmation before:

- introducing a new programming language or build system
- enabling real external sends
- broadening Feishu/QiWe permissions
- replacing or symlinking `SOUL.md` or `config.yaml` for a live Hermes profile
- deleting archives or rollback material permanently
- reviving WorkTool, OpenClaw, Hermes Kanban, or current WorkTool-bound Xiaoqin runtime

## Operating rules

These constraints supplement the scoped AGENTS.md summaries. Conditions and historical
exceptions remain binding. Backtick paths from root rules are repository-relative;
Sidecar rules retain their original `runtime/sidecar/` path base.

### Map

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

### Worktree Lifecycle

- Reuse the project root or an existing suitable worktree by default, developing on a
  feature branch. A new task, branch, or PR alone does not justify another worktree.

- Create one only for explicit user-requested isolation, parallel work requiring
  independent file states, uncommitted work that blocks safe progress, or simultaneous
  execution of different versions. Inspect `git worktree list`, branches, and local
  changes first; unrelated dirty files alone do not require isolation.

- Before creating one, state the concrete reason, path, branch, and cleanup condition.
  Proceed within existing task authorization without adding a step-by-step approval
  gate. Put manually managed worktrees in the target repository's ignored
  `.worktrees/<task>/`, not long-lived `/tmp` or `/private/tmp` directories. Preserve
  user-specified or app-managed directory conventions.

- At handoff, check actual PR merge status (including squash merges), local changes,
  untracked and ignored configuration/evidence, and running previews. Retire only this
  task's completed, recoverable worktrees; retain pending acceptance work with its
  purpose recorded. Archive local material under ignored `.local-workspace/`; never
  commit credentials or business snapshots with source changes.

- PR merge, task archival, and worktree cleanup are separate actions. Preserve all local
  work before restoring the ordinary project entrypoint to current `master` without
  disrupting another task. Never force-overwrite work or delete historical branches as
  an implicit side effect.

- For cross-repository work, resolve the target Git root and read its `AGENTS.md`. Codex
  project grouping does not change filesystem ancestry or Git boundaries. When operating
  on Green PMS, follow its worktree policy and `main`/PR requirements too.

### Commands

- Install dependencies: `pnpm install`

- Format: `pnpm format`

- Pre-commit quick checks: `.husky/pre-commit`

- Repository check: `pnpm check`

- Markdown lint: `pnpm lint:md`

- Local business testing: read `docs/testing/agent-guide.md`; use the `pnpm test:*`
  entries backed by `node tools/testing/run.mjs` for setup, discovery, targeted runs,
  full runs, reports, and harness checks.

- PR readiness: `pnpm pr:doctor`

- PR body validation: `pnpm pr:check-body`

- Local PR quick tier: `pnpm check:pr:quick`

- Local PR heavy tier: `pnpm check:pr:heavy`

- Local PR auto tier: `pnpm check:pr:auto`

- PR creation: `pnpm pr:create -- --body-file <completed-pr-body.md>`

- If the local pnpm version shim cannot verify a registry signature, do not set
  `pmOnFail=ignore`. Confirm the exact `package.json` script first; when it is a fixed
  repository-local Node entrypoint, run that entrypoint directly and record the failed
  pnpm validation attempt. In Codex's restricted sandbox, pnpm 10/11 package-manager
  switching can also fail because Node fetch to npm and writes to the user-level
  `PNPM_HOME/.tools` cache are blocked; verify once with the reviewed non-sandbox
  boundary before treating the error as lockfile tampering.

### Core Rules

- A `group_message_send` claim must clear `claimed_by`, `locked_at`, and
  `claim_expires_at` together when it records send-ready or policy-denied state. The
  transition must update exactly the locked work item before appending its audit event.

- Do not develop directly on `master`; create a feature branch first.

- Use Conventional Commits for commit messages. Allowed types are `build`, `chore`,
  `ci`, `docs`, `feat`, `fix`, `perf`, `refactor`, `revert`, `style`, and `test`.

- Do not hand humans a prefilled GitHub compare URL as the normal PR flow. Use
  `pnpm pr:doctor`, then `pnpm pr:create` with a completed PR body. If GitHub CLI is
  missing, run `pnpm pr:bootstrap`.

- In the Codex desktop environment, do not run extra GitHub authentication checks before
  creating a PR. Use `pnpm pr:create` directly after PR readiness checks; only handle
  authentication when the actual push or PR creation command fails.

- In Codex sandboxed command execution, a repo-owned Node PR script can fail when its
  child `gh` process reaches `api.github.com` even though a top-level `gh pr ...`
  command works. Treat that as sandbox network permission, not an auth failure; rerun
  the repo-owned PR entrypoint with network approval instead of re-authenticating `gh`.

- PR-Agent must not automatically edit PR descriptions. The completed repository PR
  template is author-owned because CI validates its required sections.

- Before merging any PR, read the complete PR Reviewer Guide, submitted reviews,
  conversation comments, and inline review threads for the latest head SHA. A green
  PR-Agent check is not sufficient. Resolve every security concern and recommended
  review item in code or record an explicit disposition, then wait for replacement CI
  and review results before merge.

- Do not introduce Java, Gradle, Maven, Kotlin, Go, Swift, C#, PHP, Ruby, Elixir, or a
  new language/toolchain stack without an explicit owner-approved architecture decision.

- Do not copy secrets, live `.env` files, tokens, table ids, private chat logs, raw
  member profiles, or server-only runtime state into git.

- WorkTool is not a Qintopia Agent OS channel for new work. Treat WorkTool and the
  WorkTool Hermes plugin as deprecated or audit-only material.

- Hermes Kanban is not the future task/orchestration backbone. Do not build new
  workflows on Hermes Kanban.

- Evidence and visual worker reports must derive `dry_run` from `apply_requested` so a
  `--dry-run` observation cannot report `dry_run=false`; preflight must fail closed on
  any mismatch rather than weakening that assertion.

- A content-hash conflict may reuse an existing pending `generated_image` only when its
  stable URI, source refs, and complete immutable worker metadata exactly match the new
  final JPEG result. Reviewed, stale, or modified artifacts must fail closed and must
  never be overwritten by retry processing.

### Package Placement

- Agent profile, prompt, allowed skills, memory policy, and forbidden actions:
  `agents/<agent>/`.
- Reusable channel or business capability: `skills/<capability>/`.
- Cross-Agent business process: `workflows/<workflow>/`.
- MCP server or adapter: `mcp/<adapter>/`.
- Runtime template or render/check logic: `runtime/<runtime-area>/`.
- Release, smoke, rollback, or server install logic: `deploy/<area>/`.
- Historical POC or removed direction: `deprecated/<topic>/`.

### Package Contract

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

### Migration Rules

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

### Server Change Policy

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

### Validation Expectations

Before a PR:

- Run package-level tests.
- Run fixture replay when available.
- Run registry/manifest checks when available.
- Validate the completed PR body with `pnpm pr:check-body` or `pnpm pr:doctor`.
- For runtime/deploy changes, include dry-run output and rollback notes.
- For user-facing HTML reports, run HTML parse and browser overflow checks.
- For production-adjacent changes, state whether the change touches external sends,
  database writes, profile runtime, secrets, Feishu, QiWe, or systemd.

### Documentation Rules

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

### First Read For New Agents

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

### Sidecar Map

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
