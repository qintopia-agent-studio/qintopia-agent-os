# Qintopia Agent OS Monorepo Migration Plan

Owner: PatrickLiveCool Updated: 2026-07-04

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

| Phase                       | Status                   | Exit criteria                                                                                                                                                                                                |
| --------------------------- | ------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| M0 repository bootstrap     | Complete                 | git initialized on `master`, pnpm workspace installed, root rules/docs/checks/changelog in place                                                                                                             |
| M1 inventory                | Complete                 | local repos and server runtime assets classified as `adopt`, `template`, `runtime-only`, `deprecated`, or `remove`                                                                                           |
| M2 registry contract        | Complete                 | registry schemas and package manifest templates exist and validate                                                                                                                                           |
| M3 docs migration           | Complete                 | stable architecture, operations, product, and reports moved or linked without stale state in root docs                                                                                                       |
| M4 first skill adoption     | Complete                 | `skills/qiwe` adopted with README, manifest, fixtures, tests, and source reference                                                                                                                           |
| M5 runtime sidecar adoption | Complete                 | sidecar split into runtime/mcp/workflows/deploy with tests preserved                                                                                                                                         |
| M5.5 anti-drift guardrails  | Complete                 | executable checks prevent deprecated, review-pool, and legacy deploy paths from becoming approved direction                                                                                                  |
| M6 agents adoption          | Complete                 | active profile templates migrated into `agents/*` with runtime-only state excluded and `pnpm agents:check` passing                                                                                           |
| M7 WorkTool decommission    | Complete                 | WorkTool references classified and either deprecated or final-migration cleanup items                                                                                                                        |
| M8 CI/CD deployment gate    | Complete                 | registry check, manifest check, format, markdown lint, package tests, smoke, and secret scan run in CI                                                                                                       |
| M9 server cutover           | Partial cutover complete | CI artifact build, DB preflight/migrations, and three approved sidecar services cut over; legacy worker and MCP references still need M9-F; COS distribution is being introduced for future artifact fetches |
| M10 release model           | Planned                  | versioned release directories and `current`/`previous` symlinks replace direct artifact paths and server-local profile/plugin copies                                                                         |

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
  - pruned existing GitHub sidecar artifacts down to the latest two builds and added
    `pnpm artifact:prune:sidecar` so future `master` artifact uploads keep only the
    current build plus one rollback build
- Completed M9.2/M9.3 repository-side cutover preparation without server mutation:
  - recorded the latest verified pre-M9.3 candidate SHA and previous production sidecar
    SHA in the M9 runbook
  - added `deploy/sidecar/docs/systemd-cutover-plan.md` for the monorepo-native sidecar
    systemd target shape, apply sequence, and rollback sequence
  - added `deploy/sidecar/scripts/render-systemd-units.sh` to render review-only target
    unit files for an approved artifact SHA
  - added `pnpm deploy:systemd:check` and wired it into `pnpm check`
  - kept copying unit files, daemon reloads, service restarts, and legacy runtime
    cleanup blocked until the owner-approved M9 window

### 2026-07-04

- Ran M9-A read-only server drift check:
  - server target checkout was initially missing
  - current sidecar service family remains active from
    `/home/ubuntu/qintopia-msg-sidecar`
  - active sidecar service family still points to the old release binary path
  - WorkTool, Xiaoqin WorkTool, and OpenClaw units remain inactive/disabled
  - current nginx config still references legacy port `18557`
  - root filesystem remains about 50% used with about 29G available
- Ran M9-B artifact dry-run without mutating systemd or restarting services:
  - created `/home/ubuntu/qintopia-agent-os-monorepo`
  - checked out `1a5351d2d20ae58f0718b24876e4487f8af1d935`
  - downloaded and verified the sidecar CI artifact from workflow run `28693411837`
  - stored the verified artifact under
    `/home/ubuntu/qintopia-agent-os-artifacts/1a5351d2d20ae58f0718b24876e4487f8af1d935`
  - confirmed `sha256sum -c SHA256SUMS`, `sidecar check`, embedding worker check-only,
    identity worker check-only with profile env, operations fixture smoke, and Xiaoman
    fixture smoke pass
- Found and fixed an M9 artifact download security issue:
  - the previous fetch script passed `Authorization: Bearer ...` through curl argv
  - updated `deploy/sidecar/scripts/fetch-ci-artifact.sh` to use a temporary curl config
    file and unset `GITHUB_TOKEN` before curl runs
  - added deploy preflight checks to prevent this regression
- Upgraded the M9 artifact download credential path:
  - `deploy/sidecar/scripts/fetch-ci-artifact.sh` now prefers a GitHub App installation
    token generated from server-local app credentials
  - `GITHUB_TOKEN` remains a fallback for one-off or emergency artifact downloads
  - the normal release path no longer requires creating a personal access token for each
    deployment
- Added `deploy/sidecar/scripts/postgres-schema-preflight.sh` and
  `pnpm deploy:postgres:schema:preflight`:
  - the script is read-only and checks required schemas, tables, functions,
    `schema_change_log` versions, and seeded operations capabilities
  - production initially failed the gate because
    `202606300007_operations_control_plane.sql` and
    `202607020001_operations_human_actor_guards.sql` had not been applied
- Ran M9-C production database migration without changing systemd:
  - applied the missing AgentOS operations migrations from
    `/home/ubuntu/qintopia-agent-os-monorepo/runtime/postgres/migrations`
  - verified `deploy/sidecar/scripts/postgres-schema-preflight.sh` passes against
    production
  - verified the DB-backed operations capability seed reports `capability_count=4`
  - verified sidecar check, embedding and identity check-only, and worker dry-run paths
    for member profile, graph projection, event signal, raw archive, daily digest, daily
    digest publisher, workflow sync, workbench event, and group-message send
  - kept active systemd service wiring unchanged; current production services still run
    from `/home/ubuntu/qintopia-msg-sidecar/target/release`
  - confirmed external adapter production readiness is still intentionally blocked by
    missing allowlist/config entries for group targets, reviewers, confirmers, owners,
    and attachment hosts
- Completed M9-D active service cutover:
  - used GitHub App credentials with App ID `4214034` and installation `144332887`
    through the server-local key at
    `/etc/qintopia/github-app/qintopia-agent-os-deployer.pem`
  - checked out `/home/ubuntu/qintopia-agent-os-monorepo` at
    `c70378408c53de5f4166e8b9bde45b15a97cabb0`
  - downloaded and verified the CI artifact from workflow run `28700602736`
  - stored the artifact under
    `/home/ubuntu/qintopia-agent-os-artifacts/c70378408c53de5f4166e8b9bde45b15a97cabb0`
  - backed up previous systemd units to
    `/home/ubuntu/qintopia-agent-os-backups/m9-systemd-20260704T084453Z`
  - copied and restarted only the owner-approved active services:
    `qintopia-message-sidecar.service`, `qintopia-message-embedding-worker.service`, and
    `qintopia-message-identity-worker.service`
  - fixed the first restart failure by adding
    `QINTOPIA_SIDECAR_MIGRATIONS_DIR=/home/ubuntu/qintopia-agent-os-monorepo/runtime/postgres/migrations`
    to rendered service units
  - verified checksum, sidecar check, embedding check-only, identity check-only,
    Postgres schema preflight, and fixture smokes passed after the cutover
  - kept operations timers, real external send/workbench adapters, and
    WorkTool/Xiaoqin/OpenClaw cleanup disabled or deferred
- Started M9-E GitHub App repository fetch migration:
  - confirmed the App installation now has `Actions: read`, `Contents: read`, and
    `Metadata: read`
  - verified Contents API access to the private repository
  - verified server-side `git ls-remote` against the private repository with a temporary
    `GIT_ASKPASS` helper and the server-local GitHub App key
  - added `deploy/sidecar/scripts/github-app-git.sh` so future server `git fetch`
    operations do not depend on the bot SSH alias or stored tokens
- Reconciled the live server directory direction after M9-D/M9-E:
  - recorded the target server filesystem shape in
    `docs/operations/server-directory-plan.md`
  - confirmed the current deployment is a transition model:
    `/home/ubuntu/qintopia-agent-os-monorepo` plus
    `/home/ubuntu/qintopia-agent-os-artifacts/<sha>`
  - set the target model to immutable `/home/ubuntu/qintopia-agent-os-releases/<sha>`
    directories with stable `current` and `previous` symlinks
  - confirmed `/home/ubuntu/.hermes` remains live Hermes runtime state, not a release
    directory
  - identified remaining legacy references to `/home/ubuntu/qintopia-msg-sidecar` from
    six `qintopia-agentos-*` workers and Hermes `mcp-context` processes
  - classified old server directories such as `/home/ubuntu/qintopia-agent-os`,
    `/home/ubuntu/qintopia-hermes-runtime`, `/home/ubuntu/qintopia-migration`,
    `qintopia-worklog-guard-*`, WorkTool, Xiaoqin, and OpenClaw as archive or cleanup
    candidates after runtime references are removed
- Started M9-F repository-side preparation without server mutation:
  - added `deploy/sidecar/docs/m9f-legacy-reference-removal.md`
  - added `tools/deploy/check-m9f-readiness.mjs` and `pnpm deploy:m9f:check`
  - updated the Hermes `mcp-context` wrapper so its default path no longer points back
    to `/home/ubuntu/qintopia-msg-sidecar`
  - kept the actual server worker repoint and Hermes profile config change blocked on a
    later approved migration window
  - marked `pnpm deploy:m9f:check` as temporary migration scaffolding to remove or fold
    into stable deploy checks after M9 is complete
- Started COS-first artifact distribution work:
  - kept existing GitHub Actions artifact upload for CI audit and emergency fallback
  - added COS upload and download scripts for sidecar artifacts
  - wired the `sidecar-artifact` workflow to upload to the Shanghai COS bucket when CI
    upload secrets are configured
  - added `docs/operations/cos-artifact-distribution.md` with bucket, credential, server
    env, upload, and download runbook
  - changed future server artifact fetch direction from GitHub artifact endpoints to
    Tencent COS, while preserving manifest and checksum verification
  - recorded the COS bucket `qintopia-agent-os-artifacts-1305166808`, region
    `ap-shanghai`, and default prefix `qintopia-agent-os`
  - verified the production host is Tencent Cloud Lighthouse, so the server read path
    uses `/etc/qintopia/cos-artifacts.env` with a local read-only CAM SecretKey instead
    of CVM Role
  - hardened COS upload/download scripts so transfer commands use temporary COSCLI
    config files instead of passing SecretKey values through `cp` arguments, and emit
    non-secret diagnostics when COSCLI fails
  - confirmed the latest CI failure is at COSCLI bucket configuration before file
    upload; the next likely fix is completing the CI upload CAM policy for bucket
    probe/list plus prefix-scoped object and multipart upload permissions
  - corrected the COSCLI model after verification: `config set` writes SecretKey auth
    into the temporary config, `config add` only records the bucket alias, and `cp` uses
    the temporary config without credential arguments
  - checked the follow-up CI log for run `28730023511`: COS upload passed checksum
    verification, uploaded `artifact-manifest.json` and `SHA256SUMS`, then hung while
    uploading the `qintopia-message-sidecar` binary until the 20 minute job timeout
    canceled the step
  - added bounded COSCLI execution so config commands default to 60 seconds and transfer
    commands default to 300 seconds, with sanitized diagnostics on timeout
  - confirmed the timeout diagnostic shows the binary upload is slow rather than
    unauthenticated: the 24.8 MB sidecar binary reached only about 15.9% after 300
    seconds from the GitHub-hosted runner to the Shanghai bucket
  - tuned CI COS uploads to use smaller 4 MB parts and 8 transfer threads so the current
    sidecar binary can use COSCLI multipart concurrency instead of a slow single stream
  - confirmed multipart tuning alone is not enough: the next CI run still timed out
    after 300 seconds and uploaded only about 4.8 MB of the 24.8 MB raw binary
  - changed COS artifact transport to upload `qintopia-message-sidecar.tar.gz` by
    default, while keeping the extracted server artifact path and `SHA256SUMS`
    verification unchanged
  - documented why the repository uses COSCLI directly instead of
    `TencentCloud/cos-action@v1`: the official action still targets `node12`, while this
    workflow stays on Node.js 24-compatible action runtimes
  - confirmed compressed bundle upload from GitHub-hosted runners to the Shanghai COS
    bucket is still unusably slow: CI run `28731484765` uploaded only about 479 KB of an
    8.47 MB bundle after 300 seconds
  - changed COS upload to explicit opt-in with `TENCENT_COS_UPLOAD_ENABLED=true`; CI now
    still builds and retains the GitHub Actions artifact when COS upload is disabled
  - added optional `TENCENT_COS_ENDPOINT` support for COSCLI `config add -e` so the next
    direct GitHub Actions to COS attempt can use Tencent COS Global Acceleration after
    the bucket-side setting is enabled
  - verified COS Global Acceleration for the GitHub-hosted runner path: CI run
    `28732022713` attempt 2 uploaded the compressed sidecar bundle and metadata to COS
    for commit `b44e9688f17953c0ae74952c55466794865801d2` in about 14 seconds
  - added CI-side COS artifact pruning so COS keeps the latest two sidecar SHA
    directories, matching the GitHub Actions artifact retention policy

### 2026-07-05

- Corrected the deployment direction after reviewing the M9 and COS docs:
  - routine server releases must use COS artifacts, not server-side `git fetch` or
    `git checkout`
  - server GitHub access is reserved for deploy runner bootstrap, deploy runner
    upgrades, diagnostics, or emergency fallback
  - M9-F should validate COS artifact download before any worker or Hermes MCP repoint
- Added `docs/operations/release-current-model.md` to make the M10 target explicit:
  - immutable `/home/ubuntu/qintopia-agent-os-releases/<approved-sha>` directories
  - stable `current` and `previous` symlinks
  - sidecar, Hermes profile, skill, workflow, and MCP payload categories
  - symlink-based rollback
  - Hermes live-state exclusion boundary
- Updated M9 runbook, M9-F docs, sidecar cutover docs, COS distribution docs, and server
  directory plan so they no longer present server repository pulls as the normal runtime
  release path.
- Ran the first server-side read-only COS fetch validation for artifact
  `0782f6d0f3f46d1285444f9a21f1669791be1d5e` without changing checkout, systemd,
  symlinks, or services:
  - copied the committed COS fetch scripts into `/tmp/qintopia-cos-fetch-runner`
  - attempted download into
    `/tmp/qintopia-agent-os-cos-readonly/0782f6d0f3f46d1285444f9a21f1669791be1d5e`
  - confirmed server env values are present and COSCLI `v1.0.8` runs
  - confirmed `qintopia-message-sidecar`, `qintopia-message-embedding-worker`, and
    `qintopia-message-identity-worker` remained active
  - COS returned `403` on
    `HEAD https://qintopia-agent-os-artifacts-1305166808.cos.accelerate.myqcloud.com/`
    before object download, so the current server read-only CAM key still lacks bucket
    root probe permission or an equivalent bucket-level allow statement
  - documented the required server read-only CAM policy in
    `docs/operations/cos-artifact-distribution.md`
- Completed the server-side read-only COS fetch validation after the server read-only
  CAM policy was corrected:
  - downloaded `artifact-manifest.json`, `SHA256SUMS`, and
    `qintopia-message-sidecar.tar.gz` from COS into
    `/tmp/qintopia-agent-os-cos-readonly/0782f6d0f3f46d1285444f9a21f1669791be1d5e`
  - extracted `qintopia-message-sidecar` and verified `sha256sum -c SHA256SUMS`
  - confirmed manifest fields: `commit_sha=0782f6d0f3f46d1285444f9a21f1669791be1d5e`,
    `artifact_name=qintopia-message-sidecar-linux-x86_64-gnu`, `target=linux-x86_64-gnu`
  - ran the downloaded binary `check` with production env and confirmed NATS JetStream
    plus Postgres checks passed
  - confirmed the production `qintopia-message-sidecar`,
    `qintopia-message-embedding-worker`, and `qintopia-message-identity-worker` services
    remained active and still pointed to the approved production artifact
    `c70378408c53de5f4166e8b9bde45b15a97cabb0`
  - did not update server checkout, systemd units, symlinks, Hermes profile config, or
    running service targets
- Prepared M9-F decision materials without server mutation:
  - pushed the COS-first documentation and validation commits to `origin/master`
  - confirmed GitHub Actions run `28736441689` started for
    `021b42f786037659339f412ff83f69064637617f`; at the time of this note, both `check`
    and `sidecar-artifact` were still in progress, so this docs-only commit is not a
    production runtime candidate yet
  - re-ran M9-F read-only server preflight and confirmed these six workers are still
    active/enabled from `/home/ubuntu/qintopia-msg-sidecar`:
    `qintopia-agentos-member-profile-worker.service`,
    `qintopia-agentos-graph-projection-worker.service`,
    `qintopia-agentos-raw-archive-worker.service`,
    `qintopia-agentos-event-signal-worker.service`,
    `qintopia-agentos-daily-digest-worker.service`, and
    `qintopia-agentos-daily-digest-publisher.service`
  - confirmed live `mcp-context` processes still run from
    `/home/ubuntu/qintopia-msg-sidecar/target/release/qintopia-message-sidecar`
  - confirmed Hermes `erhua` and `wenyuange` profile configs still reference
    `/home/ubuntu/qintopia-msg-sidecar/scripts/hermes/qintopia-context-mcp`
  - selected `0782f6d0f3f46d1285444f9a21f1669791be1d5e` as the M9-F artifact candidate
    because its CI passed and its COS artifact already passed server-side read-only
    validation
  - verified that `0782f6d0f3f46d1285444f9a21f1669791be1d5e` contains the M9-F
    `qintopia-context-mcp` wrapper and systemd renderer needed for the target shape
  - rendered M9-F target unit previews locally for the six workers and diffed them
    against current server units; the expected diff changes each worker
    `WorkingDirectory` from `/home/ubuntu/qintopia-msg-sidecar` to
    `/home/ubuntu/qintopia-agent-os-monorepo`, changes `ExecStart` to
    `/home/ubuntu/qintopia-agent-os-artifacts/0782f6d0f3f46d1285444f9a21f1669791be1d5e/qintopia-message-sidecar`,
    and adds `QINTOPIA_DEPLOYED_COMMIT_SHA` plus `QINTOPIA_SIDECAR_MIGRATIONS_DIR`
  - blocked direct M9-F execution because the live server deploy checkout remains at
    `94244504440a4f8fdb2eec07fd37b54db97fe368`, where
    `deploy/sidecar/scripts/hermes/qintopia-context-mcp` still defaults to the old
    `/home/ubuntu/qintopia-msg-sidecar` checkout
  - next required decision before the M9-F execution window: perform a separately
    approved deploy-runner upgrade, or use a reviewed release-managed wrapper path, so
    Hermes MCP does not repoint back to the legacy checkout
- Clarified the M9-F deploy runner boundary:
  - runtime release remains COS artifact first; server git is not the normal release
    path
  - deploy runner and wrapper files are a separate approved change from the runtime
    artifact SHA
  - the accepted options are deploy-runner checkout upgrade, release-managed wrapper, or
    a dedicated reviewed wrapper directory with backup and checksum evidence
- Ran a server `/tmp` read-only wrapper preflight without changing checkout, systemd,
  symlinks, Hermes profile config, or services:
  - copied the current `deploy/sidecar/scripts/hermes/qintopia-context-mcp` to
    `/tmp/qintopia-m9f-wrapper-preflight/qintopia-context-mcp`
  - confirmed wrapper SHA256:
    `4fab76af29320a513cd30395970c926f45e75178f67d2c8dfc4d9a7e709479d6`
  - confirmed the wrapper contains zero `/home/ubuntu/qintopia-msg-sidecar` references
  - confirmed the COS-readonly artifact binary exists at
    `/tmp/qintopia-agent-os-cos-readonly/0782f6d0f3f46d1285444f9a21f1669791be1d5e/qintopia-message-sidecar`
  - ran the wrapper with `QINTOPIA_SIDECAR_BIN` pointing at that `/tmp` artifact and
    `/etc/qintopia/message-sidecar.env`; it exited `0` with no stderr, proving the
    reviewed wrapper can resolve the verified artifact without falling back to the old
    checkout
  - M9-F is still not approved for mutation; this preflight only clears the wrapper
    resolution risk
- Corrected the M9-F plan after server GitHub git transport remained unreliable:
  - confirmed GitHub API token issuance works from the server, while GitHub git
    `ls-remote` can time out
  - removed server `git fetch` as a prerequisite for M9-F
  - added a CI-built `qintopia-agent-os-deploy-bundle` containing the Hermes MCP
    wrapper, systemd renderer, M9-F runbooks, and Postgres migrations
  - extended COS upload, download, and prune scripts to support both `sidecar` runtime
    artifacts and `deploy-bundle` operator artifacts
  - updated the M9-F target so worker units use the COS runtime artifact for `ExecStart`
    and the COS deploy bundle payload as `WorkingDirectory` and migration source
- Split the CI deploy bundle build into an independent `deploy-bundle-artifact` job:
  - the deploy bundle now builds without waiting for the Rust sidecar artifact job
  - M9-F can validate reviewed operator files from COS even when runtime artifact
    publishing is being debugged separately
  - the runtime artifact SHA and deploy bundle SHA remain separate approvals
- Completed M9-F deploy bundle read-only validation:
  - reran CI for `55d9f4e9b0e5d1feed254f370e6eb17cc9408750` after the COS upload CAM
    policy was corrected
  - confirmed `check`, `sidecar-artifact`, and `deploy-bundle-artifact` all passed
  - confirmed GitHub Actions uploaded and pruned both COS artifact families
  - downloaded the `55d9f4e` deploy bundle from COS on the server into `/tmp`
  - verified `SHA256SUMS`, deploy bundle manifest, and wrapper absence of
    `/home/ubuntu/qintopia-msg-sidecar`
  - rendered the six M9-F worker unit files to `/tmp` with runtime artifact
    `0782f6d0f3f46d1285444f9a21f1669791be1d5e` and deploy bundle payload `55d9f4e`
  - confirmed the rendered six M9-F units use the runtime artifact for `ExecStart`, the
    deploy bundle payload for `WorkingDirectory`, and the deploy bundle migrations
    directory for `QINTOPIA_SIDECAR_MIGRATIONS_DIR`
  - confirmed the six live M9-F target services remain active and still point to
    `/home/ubuntu/qintopia-msg-sidecar`; no production mutation was made
- Corrected the M9-F execution plan to avoid introducing
  `/home/ubuntu/qintopia-agent-os-deploy-bundles/<sha>` as a production runtime path:
  - deploy bundles are release assembly inputs, not long-lived service
    `WorkingDirectory` targets
  - the next M9-F mutation should assemble an immutable
    `/home/ubuntu/qintopia-agent-os-releases/<release-sha>` directory from the verified
    runtime artifact and deploy bundle payload
  - worker units should point to `/home/ubuntu/qintopia-agent-os-releases/current`
    instead of directly to artifact or deploy-bundle cache paths
- Optimized CI workflow shape:
  - repository CI now always runs the light gate but skips Python/Rust runtime checks
    for docs-only and Markdown-only changes
  - artifact publication moved to a separate `Artifacts` workflow
  - artifact publication is opt-in through `workflow_dispatch` or an explicit
    `[publish-artifacts]` commit marker
- Published and validated the first opt-in release/current candidate for
  `13a3957369ad80ea8b6e93d4c67c6ef120ecffd6` without production repoint:
  - manually triggered GitHub Actions `Artifacts` run `28740196040` with sidecar, deploy
    bundle, and COS upload enabled
  - confirmed `sidecar-artifact` passed in `5m51s`, `deploy-bundle-artifact` passed in
    `37s`, and both COS artifact families ran latest-two pruning
  - the server transitional checkout still lacks the current COS fetch script, so the
    approved `fetch-cos-artifact.sh` and `install-coscli.sh` from the pushed commit were
    copied only into `/tmp/qintopia-agent-os-bootstrap-13a3957`; the server checkout was
    not updated with `git fetch`
  - downloaded and verified the sidecar artifact into
    `/tmp/qintopia-agent-os-cos-readonly/13a3957369ad80ea8b6e93d4c67c6ef120ecffd6` and
    the deploy bundle into
    `/tmp/qintopia-agent-os-deploy-bundle-readonly/13a3957369ad80ea8b6e93d4c67c6ef120ecffd6`
  - assembled the immutable release candidate
    `/home/ubuntu/qintopia-agent-os-releases/13a3957369ad80ea8b6e93d4c67c6ef120ecffd6`
    with `production_repointed=false`; no `current` symlink exists yet and no service
    was restarted
  - rendered six M9-F worker unit previews into
    `/tmp/qintopia-m9f-rendered-units-current-13a3957369ad80ea8b6e93d4c67c6ef120ecffd6`
  - corrected the render command so `ExecStart`, `WorkingDirectory`, and
    `QINTOPIA_SIDECAR_MIGRATIONS_DIR` all point through
    `/home/ubuntu/qintopia-agent-os-releases/current`, preserving symlink rollback
  - confirmed the rendered diff for the six workers changes only the old
    `/home/ubuntu/qintopia-msg-sidecar` paths to release/current paths and adds
    `QINTOPIA_DEPLOYED_COMMIT_SHA` plus `QINTOPIA_SIDECAR_MIGRATIONS_DIR`
- Completed the M9-F worker repoint mutation window for the six already-active AgentOS
  workers:
  - backed up current unit files and systemd views to
    `/home/ubuntu/qintopia-agent-os-backups/m9f-systemd-20260705T122149Z`
  - switched `/home/ubuntu/qintopia-agent-os-releases/current` to
    `13a3957369ad80ea8b6e93d4c67c6ef120ecffd6`
  - installed only these six rendered worker units:
    `qintopia-agentos-member-profile-worker.service`,
    `qintopia-agentos-graph-projection-worker.service`,
    `qintopia-agentos-raw-archive-worker.service`,
    `qintopia-agentos-event-signal-worker.service`,
    `qintopia-agentos-daily-digest-worker.service`, and
    `qintopia-agentos-daily-digest-publisher.service`
  - ran `systemctl daemon-reload` and restarted the six workers one by one
  - verified all six are active/enabled, have zero restarts after the cutover, and their
    `/proc/<pid>/exe` paths resolve to
    `/home/ubuntu/qintopia-agent-os-releases/13a3957369ad80ea8b6e93d4c67c6ef120ecffd6/sidecar/qintopia-message-sidecar`
  - verified the six unit files no longer reference `/home/ubuntu/qintopia-msg-sidecar`
    and now use `WorkingDirectory`, `ExecStart`, and migrations from
    `/home/ubuntu/qintopia-agent-os-releases/current`
  - confirmed the operations timers remain inactive/not installed and no real external
    send or workbench adapter path was enabled
  - ran the release binary `check` with production env; NATS JetStream and Postgres
    checks passed
  - did not change Hermes MCP config, did not archive/delete legacy directories, and did
    not repoint the three previously migrated `qintopia-message-*` services during this
    window
- Completed the remaining M9-F release/current runtime repoint:
  - backed up the affected Hermes MCP profile configs to
    `/home/ubuntu/qintopia-agent-os-backups/m9f-hermes-mcp-20260705T123637Z`
  - replaced only the `qintopia-context` MCP command paths in Erhua and Wenyuange from
    `/home/ubuntu/qintopia-msg-sidecar/scripts/hermes/qintopia-context-mcp` to
    `/home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/hermes/qintopia-context-mcp`
  - restarted only `hermes-gateway-erhua.service` and `hermes-gateway-wenyuange.service`
    and verified both remained active with `NRestarts=0`
  - backed up the three `qintopia-message-*` unit files to
    `/home/ubuntu/qintopia-agent-os-backups/m9f-message-systemd-20260705T123732Z`
  - rendered the three message service units from the release renderer into
    `/tmp/qintopia-m9f-message-render-13a3957369ad80ea8b6e93d4c67c6ef120ecffd6`
  - installed and restarted `qintopia-message-sidecar.service`,
    `qintopia-message-embedding-worker.service`, and
    `qintopia-message-identity-worker.service` with `WorkingDirectory`, `ExecStart`, and
    `QINTOPIA_SIDECAR_MIGRATIONS_DIR` all pointing through
    `/home/ubuntu/qintopia-agent-os-releases/current`
  - verified all nine sidecar/worker services were active with zero restarts after this
    window and their executable paths resolved to
    `/home/ubuntu/qintopia-agent-os-releases/13a3957369ad80ea8b6e93d4c67c6ef120ecffd6/sidecar/qintopia-message-sidecar`
  - ran the release binary `check` with production env; NATS JetStream and Postgres
    checks passed
  - ran embedding worker `--check-only` and identity worker
    `--check-only --batch-size 5`; both passed
  - verified active process references to `/home/ubuntu/qintopia-msg-sidecar` and
    `/home/ubuntu/qintopia-agent-os-artifacts/c70378408c53de5f4166e8b9bde45b15a97cabb0`
    were both `0`
  - verified operations timers remained inactive/not installed and WorkTool/OpenClaw
    services remained inactive/disabled
  - did not archive/delete legacy directories, did not enable external send paths, and
    did not enable real workbench adapter paths

## Update Rule

Every migration PR must update:

- this progress log when the PR changes migration state
- `CHANGELOG.md` for user-visible repository changes
- package manifest/README when a package is adopted or its contract changes

## Immediate Next Actions

Remaining follow-up after the M9-F release/current runtime cutover:

- Server deploy checkout remains a transition diagnostic checkout at `9424450`; do not
  use `git fetch` as the routine release path.
- COS artifact distribution, opt-in artifact publication, server-side COS download
  verification, immutable release candidate assembly, Hermes MCP context repoint, and
  all nine sidecar/worker service release/current cutovers have passed for
  `13a3957369ad80ea8b6e93d4c67c6ef120ecffd6`.
- `/home/ubuntu/qintopia-agent-os-releases/previous` is still not an effective rollback
  symlink because the prior production state used transition artifact paths instead of a
  release directory. The next release window should establish a real `previous` target
  before switching `current`.
- External adapter enablement: still blocked on reviewed allowlists/config for real
  group sends and real workbench integration.
- Deprecated runtime cleanup: WorkTool, Xiaoqin WorkTool, OpenClaw, and related nginx
  references remain deferred until the final cleanup window.
- Hermes profile/plugin files under `.hermes/profiles/*` are still live runtime state.
  Future profile and skill migrations must use reviewed release bundles or symlinks, not
  wholesale copies of `.hermes`.

Recommended order:

1. Download and verify the approved runtime sidecar artifact and deploy bundle from COS
   in a staging directory.
2. Assemble an immutable release directory:
   `/home/ubuntu/qintopia-agent-os-releases/<release-sha>`. The release should include:
   - `sidecar/qintopia-message-sidecar` from the runtime artifact
   - `runtime/postgres/migrations/` from the deploy bundle payload
   - reviewed `deploy/`, `docs/`, and wrapper files from the deploy bundle payload
   - a release manifest recording the runtime SHA and deploy bundle SHA
3. Validate the release directory without changing `current`, then update a real
   `/home/ubuntu/qintopia-agent-os-releases/previous` symlink to the old release target
   before atomically switching `/home/ubuntu/qintopia-agent-os-releases/current` to the
   new release.
4. Extend release packaging after runtime cutover:
   - `sidecar-runtime` release payload
   - `hermes-profile-bundle-<agent>` payloads for reviewed non-secret profile files
   - `skill-bundle-<skill>` payloads for Hermes plugins such as `skills/qiwe`
   - broader `qintopia-agent-os-releases/<sha>` contents beyond M9-F sidecar/operator
     files
5. Keep server-side GitHub access out of routine runtime releases. Use it only for
   deploy runner bootstrap, deploy runner upgrades, diagnostics, or emergency fallback.
6. Do not repoint production to a newer commit just because docs changed; use a new
   approved target SHA and artifact only when there is a production runtime change.
7. Do not enable real external send or real workbench adapter paths until production
   allowlists/config are reviewed and set.
8. After no process, unit, timer, cron, MCP command, or nginx route references legacy
   paths, archive and then clean up `/home/ubuntu/qintopia-msg-sidecar`,
   `/home/ubuntu/qintopia-agent-os`, `/home/ubuntu/qintopia-hermes-runtime`,
   `/home/ubuntu/qintopia-migration`, `qintopia-worklog-guard-*`, WorkTool, Xiaoqin, and
   OpenClaw paths only with owner approval.
9. Add deploy smoke and rollback notes before any production wiring changes for
   `skills/qiwe` or Erhua profile bundles.
