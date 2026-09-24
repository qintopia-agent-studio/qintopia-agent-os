# Project Instructions

## Start here

Qintopia Agent OS is a capability/plugin monorepo. Start from the capability being
changed, not its implementation language.

Read [README.md](README.md), the [roadmap](docs/plans/active/current-roadmap.md),
[change routing](docs/engineering/change-routing-index.md), and the affected package's
README/manifest before editing. Consult the
[architecture](docs/architecture/agent-os-overview.md) and
[product scope](docs/product/agent-os-prd.md) when changing their boundaries.

| Task                                                  | Detailed rules                                                             |
| ----------------------------------------------------- | -------------------------------------------------------------------------- |
| Engineering, worktrees, package placement, validation | [Programming guardrails](docs/engineering/programming-agent-guardrails.md) |
| PRs and release preparation                           | [Collaboration model](docs/engineering/collaboration-model.md)             |
| Deployment and rollback                               | [Runner guide](deploy/runner/README.md)                                    |
| Rust, SQLx and Sidecar workers                        | [Sidecar instructions](runtime/sidecar/AGENTS.md)                          |
| Hermes Profiles and upgrades                          | [Hermes](runtime/hermes/README.md)                                         |
| Cron ownership and migration                          | [Cron source of truth](docs/operations/hermes-cron-source-of-truth.md)     |
| QiWe ingress, replies and media                       | [QiWe](skills/qiwe/README.md)                                              |

Cross-package work requires reading each affected package's rules. Historical conditions
and exceptions remain binding until separately reviewed. Current operational facts
belong in [production status](docs/operations/production-current-status.md) and reports.

## Working rules

- 中文沟通与文档统一用“模拟”描述测试身份、事件、数据和环境，不用“合成”表示模拟；既有代码标识保留原文。

- Do not develop directly on `master`; preserve unrelated changes and local evidence.
- Document first for features, behavior changes, migrations and runtime changes.
- CI changes require a proposal reviewed against the owner's criteria and explicit owner
  agreement before editing. Follow the
  [CI change approval rule](docs/engineering/programming-agent-guardrails.md#ci-change-approval-rule);
  development, repair, or merge authorization alone does not approve new CI changes.
- Report the files read, intended changes, validation and production boundaries before
  editing. Localize behavior through existing code, tests and contracts.
- Use a suitable existing checkout by default. Read the guardrails' worktree lifecycle
  before creating or removing isolation; a new task or PR alone is not a reason.
- Do not introduce Java or another unapproved stack without the required architecture
  decision. Organize by capability; follow the
  [package contract](docs/engineering/package-contract.md) and
  [migration policy](docs/engineering/migration-policy.md).
- Use Conventional Commits. Do not manually edit root `CHANGELOG.md` in ordinary PRs;
  Release Please owns routine changelog and version advancement.

## Commands and completion

Use `pnpm` and the configured RTK wrapper; search with `rg`. Run targeted validation
first, then `pnpm check:pr:auto` for the applicable broader tier. Use disposable local
PostgreSQL for heavy integration checks. The
[testing guide](docs/testing/agent-guide.md) and package README files define detailed
commands and prerequisites.

- Formatting and Markdown: `pnpm format:check`, `pnpm lint:md`.
- Collaboration and PR readiness: `pnpm collaboration:check`, `pnpm pr:doctor`.
- Validate the completed PR body with `pnpm pr:check-body`.

Report what changed, actual validation results and remaining acceptance items. Respect
the user's browser-tool preference; unavailable verification is not a pass. Record
production/deployment/preflight/CI integration failures in an indexed dated report and
update the relevant runbook or check. Keep detailed procedures in their owning docs.

## Production boundary

Read the [server change policy](docs/engineering/server-change-policy.md) before server
work. The server is a deployment target: no source hot edits or single-file overwrites.
Use reviewed immutable releases, exact artifact identity and concrete rollback steps.

- Keep credentials, private messages and runtime databases outside Git and retained
  evidence. Preserve Profile credentials, sessions, memories, jobs and channel state.
- Resolve server identity from the versioned inventory and administrator-provided
  details. Authentication failure does not authorize searching for private keys.
- Follow each package's owner approval, allowlist, feature and persistent-enable gates.
  A successful check does not authorize merge, release publication or production action.
- A lost acknowledgement does not prove an external action failed. Preserve uncertain
  outcomes and workflow-specific no-retry/no-replay rules.
- Service liveness and configuration readiness do not prove user-visible success. Apply
  the owning workflow's acceptance criteria and report unverified behavior.
