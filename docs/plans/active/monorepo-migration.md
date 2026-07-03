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
| M4 first skill adoption     | In progress | `skills/qiwe` adopted with README, manifest, fixtures, tests, and source reference                                 |
| M5 runtime sidecar adoption | Not started | sidecar split into runtime/mcp/workflows/deploy with tests preserved                                               |
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

## Update Rule

Every migration PR must update:

- this progress log when the PR changes migration state
- `CHANGELOG.md` for user-visible repository changes
- package manifest/README when a package is adopted or its contract changes

## Immediate Next Actions

1. Complete M4B by importing `../qiwei-hermes-plugin` source, docs, fixtures, and tests
   into `skills/qiwe` while excluding caches and runtime state.
2. Keep QiWe unit tests passing from the package location.
3. Add inventory validation after M2 registry checks have settled.
