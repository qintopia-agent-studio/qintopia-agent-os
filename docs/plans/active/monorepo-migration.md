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

| Phase                       | Status   | Exit criteria                                                                                                      |
| --------------------------- | -------- | ------------------------------------------------------------------------------------------------------------------ |
| M0 repository bootstrap     | Complete | git initialized on `master`, pnpm workspace installed, root rules/docs/checks/changelog in place                   |
| M1 inventory                | Complete | local repos and server runtime assets classified as `adopt`, `template`, `runtime-only`, `deprecated`, or `remove` |
| M2 registry contract        | Complete | registry schemas and package manifest templates exist and validate                                                 |
| M3 docs migration           | Complete | stable architecture, operations, product, and reports moved or linked without stale state in root docs             |
| M4 first skill adoption     | Complete | `skills/qiwe` adopted with README, manifest, fixtures, tests, and source reference                                 |
| M5 runtime sidecar adoption | Complete | sidecar split into runtime/mcp/workflows/deploy with tests preserved                                               |
| M5.5 anti-drift guardrails  | Complete | executable checks prevent deprecated, review-pool, and legacy deploy paths from becoming approved direction        |
| M6 agents adoption          | Complete | active profile templates migrated into `agents/*` with runtime-only state excluded and `pnpm agents:check` passing |
| M7 WorkTool decommission    | Complete | WorkTool references classified and either deprecated or final-migration cleanup items                              |
| M8 CI/CD deployment gate    | Complete | registry check, manifest check, format, markdown lint, package tests, smoke, and secret scan run in CI             |
| M9 server cutover           | Prepared | runbook exists; server deploy still waits for owner-approved window                                                |

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
- Started M7 WorkTool decommission:
  - confirmed local `../worktool` exists at `b95e746e0254894705bc63051937a3afbf4013c1`
    and is ahead of `origin/master` by 1 commit, so it remains audit-only
  - confirmed local `../worktool-hermes-plugin` exists at
    `04e95e1556cb820f5630a0f4781073cddf23c4f4`
  - confirmed server directories still exist for `/home/ubuntu/worktool-gateway`,
    `/home/ubuntu/.hermes/profiles/xiaoqin`,
    `/home/ubuntu/.hermes/profiles/xiaoqin/plugins/worktool-platform`, and
    `/opt/qiwe-openclaw-adapter`
  - confirmed read-only service scan found no matching `worktool`, `xiaoqin`, or
    `openclaw` systemd service or timer
  - added deprecated package records for `deprecated/worktool`,
    `deprecated/worktool-hermes-plugin`, and `deprecated/openclaw`
  - added `deprecated/worktool/decommission-plan.md`
  - registered the deprecated packages in `registry/deprecated.yaml`
  - extended `pnpm policy:check` to require these deprecated package records
- Started M6 active agents adoption:
  - confirmed server runtime profiles exist for `default`, `erhua`, `xiaoman`,
    `wenyuange`, `silaoshi`, `guanerye`, `huabaosi`, and deprecated `xiaoqin`
  - confirmed active Hermes user services for `default`, `erhua`, `xiaoman`,
    `wenyuange`, `silaoshi`, `guanerye`, and `huabaosi`
  - added agent package contracts for `agents/default`, `agents/erhua`,
    `agents/xiaoman`, `agents/wenyuange`, `agents/silaoshi`, `agents/guanerye`, and
    `agents/huabaosi`
  - kept `agents/huabaosi` as draft/review-pool for source disposition because Huabaosi
    shadow/Rust material remains unapproved
  - did not register `xiaoqin` as an active Agent package
  - registered the Agent packages in `registry/agents.yaml`
  - extended `pnpm policy:check` to require active Agent records and block
    `agents/xiaoqin`
- Continued M6.1 Agent profile templating:
  - performed read-only structure inventory for active profiles without copying `.env`,
    memories, sessions, auth files, logs, caches, state databases, or request dumps
  - added `profile.template.yaml`, `capabilities.md`, and `runtime-notes.md` for
    `default`, `erhua`, `xiaoman`, `wenyuange`, `silaoshi`, `guanerye`, and `huabaosi`
  - added `docs/operations/agent-capability-matrix.md`
  - linked the capability matrix from the documentation hub and operations index
  - extended `pnpm policy:check` to block live Hermes runtime files or directories under
    `agents/*`
- Completed M6 agents adoption:
  - added `tools/agents/check-agents.mjs`
  - added `pnpm agents:check` and wired it into `pnpm check`
  - required each active Agent package to include `README.md`, `agent.yaml`,
    `profile.template.yaml`, `capabilities.md`, `runtime-notes.md`, and
    `docs/source-snapshot.md`
  - required `profile.template.yaml` to declare dry-run expectations
  - kept `xiaoqin` out of active Agents and `huabaosi` in draft/review-pool
- Completed M8 CI/CD deployment gate:
  - added `tools/security/check-secrets.mjs` and `pnpm secrets:check`
  - added `tools/deploy/preflight.mjs`, `pnpm deploy:preflight`, and
    `pnpm deploy:preflight:ci`
  - wired secret scanning and CI-safe deployment preflight into `pnpm check`
  - strengthened GitHub Actions to install Node.js, pnpm, Python, and Rust before
    running `pnpm check`
  - added `docs/engineering/ci-cd-gates.md` and linked it from engineering docs
  - kept deployment preflight non-mutating; actual server cutover remains M9
- Completed M5 runtime sidecar adoption closure:
  - added `docs/plans/completed/m5-runtime-sidecar-adoption.md`
  - marked M5 package registry entries and manifests active for the adopted
    `eda2652f21999e4f32699463413372accbd3b76e` local sidecar source as monorepo
    contracts, not production cutover
  - added `pnpm fmt:sidecar` and `pnpm check:sidecar`
  - kept M9 server cutover, production systemd changes, and Huabaosi shadow branch
    adoption out of M5 scope
  - fixed M5 package docs to use monorepo-root validation commands
- Completed M7 WorkTool decommission classification without server mutation:
  - re-checked WorkTool, Xiaoqin WorkTool, and OpenClaw server state read-only
  - confirmed directories still exist for `/home/ubuntu/worktool-gateway`,
    `/home/ubuntu/.hermes/profiles/xiaoqin`,
    `/home/ubuntu/.hermes/profiles/xiaoqin/plugins/worktool-platform`, and
    `/opt/qiwe-openclaw-adapter`
  - confirmed disabled/inactive units exist for `worktool-gateway.service`,
    `hermes-gateway-xiaoqin-worktool.service`, `qiwe-openclaw-adapter.service`, and
    `openclaw-embedding-proxy.service`
  - confirmed root user `openclaw-gateway.service` is still enabled but inactive/dead
  - confirmed no listener on ports `18557` or `8787`, while current nginx config still
    references `127.0.0.1:18557`
  - deferred all server cleanup, archive, disable, and nginx changes to final migration
- Prepared M9 server cutover without server mutation:
  - added `docs/operations/m9-server-cutover-runbook.md`
  - linked the runbook from operations docs
  - kept server deploy, cleanup, archive, systemd, and nginx changes blocked on an
    owner-approved migration window
- Added the approved GitHub remote for M9:
  `git@github.com:qintopia-agent-studio/qintopia-agent-os.git`
- Updated repository code owners to `detroxryo`, `noraincode`, `PatrickLiveCool`, and
  `qiaopengjun5162`.
- Pushed local `master` to the approved GitHub remote.
- Ran M9 read-only server preflight:
  - server target checkout `/home/ubuntu/qintopia-agent-os-monorepo` is not present
  - server SSH identity could not read the private GitHub repo yet
  - server is missing Node.js and pnpm from `PATH`
  - server has Rust 1.96.1 available
  - root filesystem is about 91% used with about 5.6G available
  - current sidecar remains active from `/home/ubuntu/qintopia-msg-sidecar` at
    `codex/huabaosi-localization-shadow@b16c247a19ec751c08de75ae2d312f35b765f317`
- Added the server bot SSH alias `github-qintopia-agent-os`; read-only repo access now
  returns `master@c621b1e119127d951f6b2c10cd2cb01aa46da569`.
- Started M9.1 CI artifact release path:
  - added `tools/deploy/build-sidecar-artifact.mjs`
  - added `pnpm artifact:sidecar`
  - added CI `sidecar-artifact` job with Rust 1.75.0 and `actions/upload-artifact@v4`
  - added `deploy/sidecar/scripts/fetch-ci-artifact.sh` for server-side artifact
    download, manifest verification, and checksum verification
  - updated no-credential sidecar smoke scripts so M9 can validate the downloaded binary
    through `QINTOPIA_SIDECAR_BIN` without server-side cargo
  - added `docs/operations/sidecar-ci-artifacts.md`
  - updated M9 runbook and sidecar cutover plan so the server deploys a CI-built
    artifact instead of rebuilding with local Node.js, pnpm, or Rust tooling
  - confirmed the server has `curl`, `jq`, `unzip`, `sha256sum`, and `tar`, and has
    about 29G available on `/`
- Hardened M9.1 CI artifact path:
  - moved GitHub Actions workflow jobs to Node.js 24-compatible action majors
  - changed sidecar artifact upload to run in parallel with `pnpm check` on `master`
    pushes, while requiring deployment downloads to come from a successful workflow run
    for the approved commit SHA
  - removed the broad hand-written Cargo target cache; deferred Rust dependency caching
    because the first Rust-specific cache trial produced post-step metadata cleanup
    noise under the pinned Rust 1.75.0 toolchain
  - kept pull-request CI focused on `pnpm check`; release artifact upload remains a
    `master` push output only

## Update Rule

Every migration PR must update:

- this progress log when the PR changes migration state
- `CHANGELOG.md` for user-visible repository changes
- package manifest/README when a package is adopted or its contract changes

## Immediate Next Actions

Non-complete phases after M5 closure:

- M9 server cutover: Prepared and requires owner-approved deployment window.

Recommended order:

1. Reconcile local sidecar `main@eda2652` with the server Huabaosi shadow branch as a
   review-pool input, not an approved roadmap item.
2. Validate `sidecar-artifact` on GitHub Actions for the approved target SHA.
3. Dry-run `deploy/sidecar/scripts/fetch-ci-artifact.sh --sha <target-sha>` on the
   server with a short-lived GitHub token.
4. Fill the target commit SHA and migration window in
   `docs/operations/m9-server-cutover-runbook.md`.
5. During M9, archive or remove WorkTool/Xiaoqin/OpenClaw directories and legacy units
   only after owner approval.
6. Add deploy smoke and rollback notes before any production wiring changes for
   `skills/qiwe`.
