# Qintopia Agent OS Monorepo Migration Plan

Owner: TBD Updated: 2026-07-03

## Goal

Move Qintopia Agent OS development to one git-managed capability/plugin monorepo. The
migration must make collaboration predictable for human engineers, Codex, Claude Code,
and future programming agents.

## Non-Negotiable Boundaries

- Do not edit production server docs or code directly.
- Do not copy live `.hermes` runtime directories wholesale into git.
- Do not commit secrets, live `.env` files, private chat logs, member profile raw text,
  tokens, or table ids.
- Do not treat server-side experiments as approved architecture.
- Do not continue WorkTool as a future product path.
- Do not build new workflows on Hermes Kanban.

## Source Repositories And Runtime Inputs

| Source                               | Current role                                                       | Migration disposition                                        |
| ------------------------------------ | ------------------------------------------------------------------ | ------------------------------------------------------------ |
| `../qintopia-agent-os`               | product docs, Hermes profile templates, qintopia-tools, reports    | inventory, then adopt docs, agents, skills, workflows        |
| `../qintopia-message-sidecar`        | Rust sidecar, Postgres migrations, MCP/context, operations workers | inventory, then split across runtime, mcp, workflows, deploy |
| `../qiwei-hermes-plugin`             | Hermes QiWe platform plugin for 二花                               | first adopt candidate: `skills/qiwe/`                        |
| `../worktool`                        | historical Android WorkTool POC                                    | deprecated or remove after reference audit                   |
| `../worktool-hermes-plugin`          | historical WorkTool Hermes plugin                                  | deprecated or remove after reference audit                   |
| server `.hermes/profiles/*`          | live runtime profiles, scripts, plugins, cron, cache, secrets      | inventory only; convert selected files to templates          |
| server `qintopia-agent-os` branch    | server-side docs and Rust exploration                              | review pool; not approved by default                         |
| server `qintopia-msg-sidecar` branch | live sidecar deployment source                                     | inventory and reconcile with local main                      |

## Target Package Map

| Target                              | Inputs                                                                      |
| ----------------------------------- | --------------------------------------------------------------------------- |
| `agents/erhua`                      | 二花 profile prompt, allowed skills, QiWe boundaries, trainer memory policy |
| `agents/xiaoman`                    | 小满 activity profile and activity signal boundaries                        |
| `agents/huabaosi`                   | 画报司 profile and visual asset capability contract                         |
| `agents/wenyuange`                  | knowledge and evidence retrieval profile                                    |
| `agents/silaoshi`                   | daily operations / community operations profile                             |
| `agents/guanerye`                   | engineering automation profile                                              |
| `skills/qiwe`                       | `../qiwei-hermes-plugin`                                                    |
| `skills/qintopia-weather`           | weather lookup and scheduled broadcast scripts                              |
| `skills/qintopia-tools`             | governed business wrappers after capability split review                    |
| `skills/feishu-base`                | Feishu Base-specific adapters and fixtures                                  |
| `skills/postgres-context`           | Postgres context lookup tools                                               |
| `skills/knowledge-retrieval`        | knowledge and evidence retrieval tools                                      |
| `workflows/xiaoman-activity-signal` | Xiaoman activity signal and handoff flow                                    |
| `workflows/visual-asset-request`    | Xiaoman -> Huabaosi visual asset request                                    |
| `workflows/activity-promotion`      | parent workflow spanning evidence, visual, and group send                   |
| `mcp/context-server`                | sidecar context MCP server                                                  |
| `runtime/sidecar`                   | sidecar Rust service and workers                                            |
| `runtime/postgres`                  | migrations and schema runbooks                                              |
| `runtime/hermes`                    | Hermes profile templates and render rules                                   |
| `deploy`                            | systemd templates, release scripts, smoke, rollback                         |
| `deprecated/worktool`               | WorkTool POC material with audit value only                                 |
| `deprecated/worktool-hermes-plugin` | WorkTool Hermes plugin audit material only                                  |
| `deprecated/hermes-kanban`          | legacy Kanban docs and schemas with audit value                             |

## Migration Phases

| Phase                       | Status      | Exit criteria                                                                                                      |
| --------------------------- | ----------- | ------------------------------------------------------------------------------------------------------------------ |
| M0 repository bootstrap     | Complete    | git initialized on `master`, pnpm workspace installed, root rules/docs/checks/changelog in place                   |
| M1 inventory                | Complete    | local repos and server runtime assets classified as `adopt`, `template`, `runtime-only`, `deprecated`, or `remove` |
| M2 registry contract        | Complete    | registry schemas and package manifest templates exist and validate                                                 |
| M3 docs migration           | Complete    | stable architecture, operations, product, and reports moved or linked without stale state in root docs             |
| M4 first skill adoption     | Complete    | `skills/qiwe` adopted with README, manifest, fixtures, tests, and source reference                                 |
| M5 runtime sidecar adoption | In progress | sidecar split into runtime/mcp/workflows/deploy with tests preserved                                               |
| M5.5 anti-drift guardrails  | Complete    | executable checks prevent deprecated, review-pool, and legacy deploy paths from becoming approved direction        |
| M6 agents adoption          | Not started | active profile templates migrated into `agents/*` with runtime-only state excluded                                 |
| M7 WorkTool decommission    | Not started | WorkTool references classified and either deprecated or removed                                                    |
| M8 CI/CD deployment gate    | Not started | registry check, manifest check, format, markdown lint, package tests, smoke, and secret scan run in CI             |
| M9 server cutover           | Not started | server deploys reviewed commit SHA from this repo with smoke and rollback                                          |

## Progress Log

### 2026-07-03

- Created monorepo directory structure with `.gitkeep` files.
- Added root `README.md`, `AGENTS.md`, `CLAUDE.md` and Chinese counterparts.
- Initialized git on `master`.
- Added pnpm workspace baseline and installed common tooling:
  - `prettier`
  - `markdownlint-cli2`
  - `@changesets/cli`
- Added `.editorconfig`, `.gitattributes`, Prettier config, Markdown lint config, and
  expanded `.gitignore`.
- Moved migration status out of root rule files into this plan.
- Added `CONTRIBUTING.md`, `CHANGELOG.md`, Changesets config, PR template, CODEOWNERS,
  and GitHub Actions CI for `pnpm check`.
- Ran `pnpm format` and `pnpm check`; both pass.
- Performed read-only server document inventory for `/home/ubuntu/qintopia-agent-os` and
  `/home/ubuntu/qintopia-msg-sidecar`.
- Added `docs/README.md` and `docs/README.zh-CN.md` as documentation hubs.
- Added architecture, engineering, and operations indexes.
- Added the current Agent OS architecture overview, collaboration model, package
  contract, migration policy, server change policy, and source document inventory.
- Linked root README, `AGENTS.md`, and `CLAUDE.md` to the documentation hub without
  moving transient migration state back into those files.
- Completed M3 docs migration by adding product scope, Agent OS domain/contract/test
  docs, runtime baseline, reports index, and updated read paths for collaborators.
- Completed M1 inventory first pass:
  - `docs/operations/inventory/local-sources.yaml`
  - `docs/operations/inventory/server-sources.yaml`
  - `docs/operations/inventory/runtime-assets.yaml`
- Confirmed local `../qintopia-agent-os` is dirty and must not be used as a clean
  adoption source until reviewed.
- Confirmed server `.hermes/hermes-agent` is a dirty runtime checkout and must stay in
  review-pool until patch extraction.
- Classified WorkTool, WorkTool Hermes plugin, Xiaoqin WorkTool runtime, and OpenClaw
  legacy paths as deprecated inventory inputs.
- Completed M2 registry contract:
  - added package manifest and registry index JSON Schemas
  - added domain registry indexes
  - added manifest templates for agents, skills, workflows, MCP, runtime, deploy, and
    deprecated packages
  - added `pnpm registry:check` and wired it into `pnpm check`
- Started M4 first skill adoption with `skills/qiwe` metadata:
  - added `skills/qiwe/README.md`
  - added `skills/qiwe/manifest.yaml`
  - registered `skills/qiwe` in `registry/skills.yaml`
  - recorded read-only review of the server QiWe backup file
  - verified the current source repository with
    `python3 -m unittest discover -s tests -v`
- Completed M4 first skill adoption by importing `../qiwei-hermes-plugin@6f69794` into
  `skills/qiwe`:
  - imported plugin source, `plugin.yaml`, docs, scripts, fixtures, and tests
  - excluded `.git`, source README/AGENTS, caches, `.DS_Store`, and runtime state
  - added `skills/qiwe/docs/source-snapshot.md`
  - added `pnpm test:qiwe` and wired it into `pnpm check`
  - verified package-local tests with `pnpm test:qiwe`
- Started M5 runtime sidecar adoption with package contracts, not full source import:
  - added `runtime/sidecar` for the Rust service and worker runtime contract
  - added `runtime/postgres` for migrations, schema notes, and the Agent OS fact source
  - added `mcp/context-server` for answer-context, knowledge, and evidence routing
  - added `mcp/message-store` for controlled message and discussion-evidence lookup
  - added `workflows/activity-promotion` for the Agent OS operations control-plane flow
  - added `deploy/sidecar` for git-based rollout, smoke, and rollback contracts
  - registered the new packages in `registry/runtime.yaml`, `registry/mcp.yaml`,
    `registry/workflows.yaml`, and `registry/deploy.yaml`
  - confirmed local `../qintopia-message-sidecar` is clean at
    `eda2652f21999e4f32699463413372accbd3b76e`
  - confirmed server `/home/ubuntu/qintopia-msg-sidecar` is clean but on
    `codex/huabaosi-localization-shadow@b16c247a19ec751c08de75ae2d312f35b765f317`; treat
    that branch as review-pool until owner approval
- Continued M5 by importing the reviewed local sidecar source snapshot:
  - imported Rust crate files into `runtime/sidecar`
  - imported migrations and data-design docs into `runtime/postgres`
  - imported context MCP and message-store MCP docs into `mcp/*/docs`
  - imported operations control-plane docs into `workflows/activity-promotion/docs`
  - imported deployment runbook and scripts into `deploy/sidecar`
  - excluded `.git`, `target`, `vendor`, real `.env`, credentials, and server-only state
  - added `runtime/sidecar/docs/source-snapshot.md`
  - patched migration path resolution so the sidecar reads `runtime/postgres/migrations`
    by default inside the monorepo
  - patched no-credential deploy smokes to run against `runtime/sidecar`
  - added `pnpm test:sidecar` and `pnpm smoke:sidecar`
  - wired sidecar unit tests and no-credential sidecar smokes into `pnpm check`
  - verified `cargo fmt --check --manifest-path runtime/sidecar/Cargo.toml`
  - verified `cargo test --manifest-path runtime/sidecar/Cargo.toml` with 172 tests
  - verified `deploy/sidecar/scripts/operations-control-plane-smoke.sh`
  - verified `deploy/sidecar/scripts/xiaoman-activity-acceptance-smoke.sh`
- Marked `deploy/sidecar/scripts/server-deploy.sh` as a legacy source snapshot, not the
  current monorepo-native production deploy entrypoint.
- Added `deploy/sidecar/docs/monorepo-cutover-plan.md` to capture the M9 server cutover
  sequence, preconditions, validation, rollback, and Huabaosi shadow branch boundary.
- Completed M5.5 anti-drift guardrails:
  - added `tools/policy/check-anti-drift.mjs`
  - added `pnpm policy:check` and wired it into `pnpm check`
  - added `docs/engineering/anti-drift-policy.md`
  - enforce WorkTool/Xiaoqin deprecation boundaries in inventory and active package
    sources
  - enforce Huabaosi shadow work as review-pool unless owner-approved
  - enforce sidecar deploy script as a legacy snapshot before M9 cutover
  - enforce Postgres migration and data-design note consistency
  - verified `pnpm policy:check`

## Update Rule

Every migration PR must update:

- this progress log when the PR changes migration state
- `CHANGELOG.md` for user-visible repository changes
- package manifest/README when a package is adopted or its contract changes

## Immediate Next Actions

1. Reconcile local sidecar `main@eda2652` with the server Huabaosi shadow branch as a
   review-pool input, not an approved roadmap item.
2. Decide whether to move next into M6 agents adoption or M7 WorkTool decommission.
3. Add deploy smoke and rollback notes before any production wiring changes for
   `skills/qiwe`.
