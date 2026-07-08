# Change Routing Index

This is the fast localization index for Codex, Claude Code, and human engineers. Use it
after reading the root rules and before editing. Its job is to answer: "I want to change
X. Where do I start?"

## How To Use This Index

1. Identify the user-visible behavior or runtime surface you want to change.
2. Find the closest row in [Common Change Requests](#common-change-requests).
3. Read the listed package README or manifest before editing.
4. Update documentation or manifest first when behavior, runtime, migration, or
   production-adjacent boundaries change.
5. Run the listed validation commands, then `pnpm check:light`; run `pnpm check` when
   runtime, database, QiWe, workflow, or deploy behavior changes.

Do not start from implementation language. Start from Agent, skill, workflow, MCP,
runtime, deploy, fixture, or registry ownership.

## Common Change Requests

| Change request                              | Start here                                                                                                                                                                   | Then inspect                                                                                                                                                                                  | Validation                                                                                                            |
| ------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| Change Erhua reply wording or group rules   | `agents/erhua/README.md`, `agents/erhua/capabilities.md`, `workflows/erhua-consultation/README.md`                                                                           | `skills/qiwe/`, `skills/qintopia-tools/variants/erhua/`, `skills/knowledge-retrieval/`, `skills/postgres-context/`, `fixtures/qiwe/`                                                          | `pnpm test:qiwe`, `pnpm agents:check`, `pnpm workflows:check`                                                         |
| Change Erhua weather behavior               | `skills/qintopia-weather/README.md`, `skills/qintopia-weather/manifest.yaml`                                                                                                 | `skills/qintopia-weather/__init__.py`, `skills/qintopia-weather/tests/`, provider contract in `mcp/weather-provider/`, Hermes registration shell in `skills/qintopia-tools/variants/erhua/`   | `pnpm skills:qintopia-weather:check`, `pnpm skills:qintopia-tools:check`, `pnpm mcp:adapters:check`                   |
| Change weather provider/API integration     | `mcp/weather-provider/README.md`, `mcp/weather-provider/manifest.yaml`                                                                                                       | `skills/qintopia-weather/__init__.py`, provider fixtures in `fixtures/weather/`, runtime secret notes in the target deployment docs                                                           | `pnpm mcp:adapters:check`, `pnpm skills:qintopia-weather:check`                                                       |
| Change Erhua trainer memory                 | `skills/postgres-context/README.md`, `runtime/postgres/docs/data-design/2026-06-29-erhua-training-memory.md`                                                                 | `skills/postgres-context/fixtures/`, `runtime/sidecar/src/context_tools.rs`, `mcp/postgres/README.md`, `agents/erhua/runtime-notes.md`                                                        | `pnpm skills:postgres-context:check`, `pnpm test:sidecar`, `pnpm agents:profile-bundles:check`                        |
| Change complaint or service recovery intake | `skills/operations-intake/README.md`, `workflows/erhua-consultation/README.md`                                                                                               | `skills/operations-intake/__init__.py`, `skills/operations-intake/tests/`, Hermes registration shell in `skills/qintopia-tools/variants/*/`                                                   | `pnpm skills:operations-intake:check`, `pnpm skills:qintopia-tools:check`                                             |
| Change customer lead or disclosure handoff  | `skills/operations-intake/README.md`, future Xiaoqin Agent design docs                                                                                                       | `skills/operations-intake/__init__.py`, `skills/operations-intake/tests/`, `workflows/silaoshi-daily-ops/`, `workflows/visual-asset-request/`                                                 | `pnpm skills:operations-intake:check`, `pnpm workflows:check`                                                         |
| Change scheduled jobs or cron-like behavior | `agents/<agent>/runtime-notes.md`, `runtime/hermes/README.md`, `workflows/silaoshi-daily-ops/README.md`                                                                      | `runtime/sidecar/src/daily_digest_publisher.rs`, `runtime/sidecar/config/agentos/`, `deploy/sidecar/scripts/render-systemd-units.sh`, `runtime/systemd/README.md`                             | `pnpm runtime:hermes:check`, `pnpm deploy:systemd:check`, `pnpm check:runtime`                                        |
| Change WenYuanGe document lookup path       | `agents/wenyuange/README.md`, `skills/knowledge-retrieval/README.md`, `skills/knowledge-retrieval/manifest.yaml`                                                             | `skills/knowledge-retrieval/__init__.py`, `skills/knowledge-retrieval/tests/`, registration shell in `skills/qintopia-tools/variants/wenyuange/`, sidecar context docs when changing MCP path | `pnpm skills:knowledge-retrieval:check`, `pnpm skills:qintopia-tools:check`, `pnpm test:sidecar` when sidecar changes |
| Change Dify raw tool exposure               | `skills/knowledge-retrieval/README.md`, `skills/qintopia-tools/variants/wenyuange/README.md`                                                                                 | raw-tool registration gates in `skills/qintopia-tools/variants/*/__init__.py`, `skills/qintopia-tools/variants/*/plugin.yaml`, implementation in `skills/knowledge-retrieval/`                | `pnpm skills:knowledge-retrieval:check`, `pnpm skills:qintopia-tools:check`                                           |
| Change Postgres table structure             | `runtime/postgres/README.md`, `runtime/postgres/docs/data-design/README.md`                                                                                                  | add design note under `runtime/postgres/docs/data-design/`, add migration under `runtime/postgres/migrations/`, check sidecar usages in `runtime/sidecar/src/`                                | `pnpm policy:check`, `pnpm test:sidecar`, `pnpm deploy:postgres:schema:preflight` when preparing deployment           |
| Change database-backed context APIs         | `skills/postgres-context/README.md`, `mcp/postgres/README.md`, `mcp/context-server/README.md`                                                                                | `skills/postgres-context/fixtures/`, `runtime/sidecar/src/context_tools.rs`, `runtime/sidecar/src/message_search.rs`, `runtime/sidecar/src/member_profile.rs`                                 | `pnpm skills:postgres-context:check`, `pnpm mcp:adapters:check`, `pnpm test:sidecar`                                  |
| Change QiWe webhook parsing or send guards  | `skills/qiwe/README.md`, `skills/qiwe/docs/architecture.md`                                                                                                                  | `skills/qiwe/passive_pipeline.py`, `skills/qiwe/qiwe_events.py`, `skills/qiwe/adapter.py`, `skills/qiwe/tests/`, `fixtures/qiwe/`                                                             | `pnpm test:qiwe`                                                                                                      |
| Change activity solitaire or Feishu mapping | `skills/qiwe/docs/plans/active/qiwe-hermes-platform-plugin.md`, `skills/qiwe/docs/examples/`, `skills/qiwe/solitaire/`                                                       | `skills/qiwe/solitaire/feishu_writer.py`, `skills/qiwe/scripts/verify_activity_mapping.py`, `runtime/postgres/docs/data-design/`                                                              | `pnpm test:qiwe`                                                                                                      |
| Change Xiaoman activity signal behavior     | `agents/xiaoman/README.md`, `workflows/xiaoman-activity-signal/README.md`                                                                                                    | `runtime/sidecar/src/xiaoman_activity.rs`, `runtime/sidecar/fixtures/xiaoman_activity_records.json`, `fixtures/xiaoman/`                                                                      | `pnpm workflows:check`, `pnpm smoke:sidecar`, `pnpm test:sidecar`                                                     |
| Change visual asset request flow            | `workflows/visual-asset-request/README.md`, `workflows/activity-promotion/README.md`, `agents/huabaosi/README.md`                                                            | `runtime/sidecar/src/operations.rs`, `runtime/sidecar/src/evidence.rs`, `fixtures/operations/`                                                                                                | `pnpm workflows:check`, `pnpm smoke:sidecar`                                                                          |
| Change Si Laoshi operations behavior        | `agents/silaoshi/README.md`, `workflows/silaoshi-daily-ops/README.md`                                                                                                        | `runtime/sidecar/src/daily_digest_publisher.rs`, `runtime/sidecar/config/agentos/`, `runtime/systemd/README.md`                                                                               | `pnpm workflows:check`, `pnpm check:runtime`                                                                          |
| Change Huabaosi Feishu Base read behavior   | `agents/huabaosi/README.md`, `skills/feishu-base/README.md`, `mcp/feishu/README.md`                                                                                          | `skills/feishu-base/__init__.py`, `skills/feishu-base/tests/`, runtime env notes in `skills/feishu-base/docs/source-snapshot.md`                                                              | `pnpm skills:feishu-base:check`                                                                                       |
| Change MCP command packaging                | `mcp/qintopia-collab/README.md`, `deploy/sidecar/README.md`, `docs/operations/release-current-model.md`                                                                      | `tools/deploy/build-deploy-bundle.mjs`, `deploy/sidecar/scripts/hermes/qintopia-context-mcp`, `deploy/manifests/`                                                                             | `pnpm mcp:collab:check`, `pnpm artifact:deploy-bundle`, `pnpm deploy:release-model:check`                             |
| Change systemd units                        | `runtime/systemd/README.md`, `deploy/sidecar/docs/systemd-cutover-plan.md`                                                                                                   | `deploy/sidecar/scripts/render-systemd-units.sh`, `tools/deploy/check-release-model.mjs`, `deploy/rollback/README.md`                                                                         | `pnpm deploy:systemd:check`, `pnpm deploy:release-model:check`                                                        |
| Change release manifest or rollback model   | `deploy/manifests/README.md`, `deploy/rollback/README.md`, `docs/operations/release-current-model.md`                                                                        | `deploy/manifests/release-manifest.template.yaml`, `tools/deploy/check-deploy-contracts.mjs`, `tools/deploy/check-release-model.mjs`                                                          | `pnpm deploy:contracts:check`, `pnpm deploy:release-model:check`                                                      |
| Change COS artifact upload/download         | `docs/operations/cos-artifact-distribution.md`, `deploy/sidecar/README.md`                                                                                                   | `deploy/sidecar/scripts/upload-cos-artifact.sh`, `deploy/sidecar/scripts/fetch-cos-artifact.sh`, `deploy/sidecar/scripts/prune-cos-artifacts.sh`, GitHub Actions workflows                    | `pnpm deploy:cos:check`, `pnpm deploy:preflight:ci`                                                                   |
| Change CI checks or docs-only behavior      | `docs/engineering/ci-cd-gates.md`, `tools/ci/README.md`                                                                                                                      | `.github/workflows/`, `package.json`, `tools/ci/check-ci-contracts.mjs`, `tools/policy/`                                                                                                      | `pnpm tools:ci:check`, `pnpm check:light`                                                                             |
| Add a new Agent                             | `agents/_template/agent.yaml`, `docs/agent-os/agent-contracts.md`, `docs/operations/agent-capability-matrix.md`                                                              | `registry/agents.yaml`, target `agents/<agent>/`, profile bundle docs in `runtime/hermes/`, restart rules in `deploy/restart-target-rules.yaml`, deploy runner allowlist and smoke            | `pnpm agents:check`, `pnpm registry:check`, `pnpm agents:profile-bundles:check`, `pnpm deploy:runner:check`           |
| Add a new skill                             | `skills/_template/manifest.yaml`, `docs/engineering/package-contract.md`                                                                                                     | `registry/skills.yaml`, target `skills/<capability>/`, relevant fixtures under `fixtures/`                                                                                                    | package check plus `pnpm registry:check`, `pnpm check:light`                                                          |
| Add a new workflow                          | `workflows/_template/workflow.yaml`, `docs/engineering/package-contract.md`                                                                                                  | `registry/workflows.yaml`, target `workflows/<workflow>/`, replay fixtures under `fixtures/`                                                                                                  | `pnpm workflows:check`, `pnpm registry:check`                                                                         |
| Add a new MCP adapter                       | `mcp/_template/manifest.yaml`, `docs/engineering/package-contract.md`                                                                                                        | `registry/mcp.yaml`, target `mcp/<adapter>/`, runtime secret boundary docs                                                                                                                    | `pnpm mcp:adapters:check`, `pnpm registry:check`                                                                      |
| Add runtime/deploy templates                | `runtime/_template/manifest.yaml`, `deploy/_template/manifest.yaml`, `docs/operations/release-current-model.md`                                                              | `registry/runtime.yaml`, `registry/deploy.yaml`, `runtime/<area>/`, `deploy/<area>/`, corresponding render checks                                                                             | `pnpm runtime:contracts:check`, `pnpm deploy:contracts:check`                                                         |
| Touch production server behavior            | `docs/engineering/server-change-policy.md`, `docs/operations/m9-server-cutover-runbook.md`, `docs/operations/release-current-model.md`, target deploy/runtime package README | `deploy/rollback/README.md`, `deploy/smoke/README.md`, `deploy/manifests/release-manifest.template.yaml`, `deploy/restart-target-rules.yaml`, deploy request schemas                          | `pnpm check`, plus documented smoke and rollback plan                                                                 |

## Directory Ownership

| Path                    | Owns                                                                                                   | Does not own                                                             |
| ----------------------- | ------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------ |
| `agents/<agent>/`       | Agent profile contract, allowed skills, prompt/profile templates, capability notes, runtime exclusions | live `.hermes` state, secrets, generated memory, direct production edits |
| `skills/<capability>/`  | reusable Agent capability, plugin contract, package-local tests, fixtures                              | cross-Agent orchestration, systemd, raw runtime state                    |
| `workflows/<workflow>/` | business process, acceptance scenarios, human gates, cross-package coordination                        | low-level channel parsing, database schema ownership                     |
| `mcp/<adapter>/`        | MCP server or adapter contract, tool boundary, permission model                                        | app secrets, unrestricted SQL, live service config                       |
| `runtime/postgres/`     | schema migrations, data-design docs, Postgres fact-source contract                                     | Feishu table design as source of truth, live DB dumps                    |
| `runtime/sidecar/`      | Rust sidecar source, workers, context tools, message capture, operations control plane                 | profile prompts, Hermes live state, direct server release decisions      |
| `runtime/hermes/`       | profile bundle rules and dry-run bundle templates                                                      | live `SOUL.md`, `.env`, sessions, logs, memory databases                 |
| `runtime/systemd/`      | target systemd template contract and render expectations                                               | installing units on the server without runbook approval                  |
| `runtime/nginx/`        | future Agent OS-owned ingress route templates                                                          | TLS secrets, server-only nginx snippets                                  |
| `deploy/`               | artifact assembly, release manifests, smoke, rollback, server runbooks                                 | unreviewed production code changes                                       |
| `fixtures/`             | sanitized replay inputs and expected outputs                                                           | raw chat logs, private records, production exports                       |
| `registry/`             | machine-readable package discovery and manifest validation                                             | detailed package behavior docs                                           |
| `tools/`                | repository-owned validation, inventory, artifact, and CI helpers                                       | product runtime behavior unless explicitly a package tool                |
| `docs/engineering/`     | stable collaboration, package, migration, CI, routing, and guardrail rules                             | transient migration status                                               |
| `docs/operations/`      | server/runtime evidence, runbooks, inventories, release model                                          | canonical product behavior when package docs disagree                    |
| `deprecated/`           | historical POC and retired paths for audit                                                             | new product direction                                                    |

## Agent Entry Points

| Agent     | Start here                   | Behavior usually lives in                                                                                                                         | Runtime caveat                                            |
| --------- | ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------- |
| Erhua     | `agents/erhua/README.md`     | `skills/qiwe/`, `skills/qintopia-tools/variants/erhua/`, `workflows/erhua-consultation/`, `skills/postgres-context/`, `skills/operations-intake/` | group replies and profile changes are production-adjacent |
| Xiaoman   | `agents/xiaoman/README.md`   | `workflows/xiaoman-activity-signal/`, `runtime/sidecar/src/xiaoman_activity.rs`, `workflows/activity-promotion/`                                  | activity writes need idempotency and review states        |
| WenYuanGe | `agents/wenyuange/README.md` | `skills/knowledge-retrieval/`, qintopia-tools registration shell, `mcp/context-server/`, `runtime/sidecar/src/knowledge.rs`                       | raw Dify tools stay scoped and filtered                   |
| Si Laoshi | `agents/silaoshi/README.md`  | `workflows/silaoshi-daily-ops/`, daily digest sidecar code, systemd schedules                                                                     | scheduled or external actions need explicit review        |
| Huabaosi  | `agents/huabaosi/README.md`  | `skills/feishu-base/`, `workflows/visual-asset-request/`, `workflows/activity-promotion/`                                                         | external visual publishing is out of scope until approved |
| Guanerye  | `agents/guanerye/README.md`  | runtime/deploy docs, validation tools, rollback docs                                                                                              | production changes require owner-approved runbooks        |
| Default   | `agents/default/README.md`   | routing and escalation docs, operations workflows                                                                                                 | do not silently broaden routing authority                 |

Xiaoqin is not an active Agent package. Future Xiaoqin work needs a new non-WorkTool
Agent design and owner-approved registry entry.

## Restart Target Routing

Release deploys derive restart targets from `deploy/restart-target-rules.yaml`, not from
PR body text. PR checks may preview impact, but production deploy requests are computed
again from each restart target's latest server deploy result with status `succeeded` to
the current Release tag. A successful GitHub workflow alone may be a dry-run, and a
successful deploy may have restarted only some targets. When workflow logs cannot prove
a target-specific live baseline, the workflow falls back to the previous published
Release tag for that target.

When adding an Agent, add `runtime.restart_target` and `runtime.systemd_user_service` to
`agents/<agent>/agent.yaml`, then add the matching deploy request schema entry, smoke
restart case, restart rule, and contract tests in the same PR. Do not add a new profile
package without this deployable restart contract.

## File-Level Anchors

Use these anchors after choosing the package:

- Profile metadata: `agents/<agent>/agent.yaml`
- Profile capability notes: `agents/<agent>/capabilities.md`
- Profile runtime exclusions: `agents/<agent>/runtime-notes.md`
- Future profile bundle template: `agents/<agent>/profile.template.yaml`
- Hermes bundle rules: `runtime/hermes/README.md`
- Skill manifest: `skills/<skill>/manifest.yaml`
- Hermes plugin manifest: `skills/<skill>/plugin.yaml` when present
- Workflow manifest: `workflows/<workflow>/workflow.yaml`
- Runtime manifest: `runtime/<area>/manifest.yaml`
- Deploy manifest: `deploy/<area>/manifest.yaml`
- Registry domain index: `registry/<domain>.yaml`
- Data design notes: `runtime/postgres/docs/data-design/`
- SQL migrations: `runtime/postgres/migrations/`
- Sidecar CLI/config: `runtime/sidecar/src/config.rs`
- Context and evidence tools: `runtime/sidecar/src/context_tools.rs`,
  `runtime/sidecar/src/evidence.rs`
- Operations workflow engine: `runtime/sidecar/src/operations.rs`
- QiWe parser and adapter: `skills/qiwe/`
- Deploy bundle builder: `tools/deploy/build-deploy-bundle.mjs`
- Release/current checks: `tools/deploy/check-release-model.mjs`

## Validation Shortcuts

| Touched area                | Minimum command                                                                       |
| --------------------------- | ------------------------------------------------------------------------------------- |
| docs and registry only      | `pnpm check:light`                                                                    |
| Agent packages              | `pnpm agents:check` and `pnpm agents:profile-bundles:check`                           |
| Skill package metadata      | relevant `pnpm skills:*:check` and `pnpm registry:check`                              |
| Postgres context contract   | `pnpm skills:postgres-context:check`, then `pnpm test:sidecar` when behavior changes  |
| Workflow package metadata   | `pnpm workflows:check`                                                                |
| QiWe behavior               | `pnpm test:qiwe`                                                                      |
| Sidecar runtime             | `pnpm fmt:sidecar`, `pnpm check:sidecar`, `pnpm test:sidecar`                         |
| Postgres schema             | `pnpm policy:check`, `pnpm test:sidecar`, deployment preflight when preparing release |
| Deploy/runtime templates    | `pnpm deploy:systemd:check`, `pnpm deploy:release-model:check`                        |
| Production-adjacent changes | `pnpm check`                                                                          |
