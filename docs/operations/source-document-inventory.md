# Source Document Inventory

Inventory date: 2026-07-03

This inventory summarizes the server and local documents reviewed for the monorepo
documentation pass. It is not a full migration record. Detailed source hashes and
package manifests should be added when a source is actually adopted.

## Server: `/home/ubuntu/qintopia-agent-os`

Branch at inventory time: `codex/rust-agent-os-baseline`

| Source path                                           | Disposition    | Notes                                                |
| ----------------------------------------------------- | -------------- | ---------------------------------------------------- |
| `docs/README.md`                                      | review-pool    | Useful collaborator index, but server-authored       |
| `docs/agent-os/README.md`                             | adopt-input    | Good architecture framing for Agent OS control plane |
| `docs/agent-os/architecture.md`                       | adopt-input    | Current layered architecture and object model        |
| `docs/agent-os/agents.md`                             | adopt-input    | Agent descriptions need owner review                 |
| `docs/agent-os/sidecar-pattern.md`                    | adopt-input    | Sidecar boundary reference                           |
| `docs/agent-os/development-framework.md`              | adopt-input    | Development approach reference                       |
| `docs/agent-os/extension-guide.md`                    | adopt-input    | Future package onboarding input                      |
| `docs/agent-os/guardrails.md`                         | adopt-input    | Production guardrail input                           |
| `docs/agent-os/source-repository.md`                  | adopt-input    | Supports monorepo source-of-truth direction          |
| `docs/agent-os/huabaosi-production-rust-migration.md` | review-pool    | Rust/Huabaosi direction is not approved by default   |
| `docs/agent-os/rust-dev-workflow.md`                  | review-pool    | Rust workflow evidence, not a decided standard       |
| `docs/operations/*huabaosi*`                          | review-pool    | Read-only capture and recovery evidence              |
| `docs/superpowers/plans/*huabaosi*`                   | review-pool    | Execution plans need owner review                    |
| `docs/superpowers/specs/*huabaosi*`                   | review-pool    | Specs need owner review                              |
| `docs/operations/server-layout.md`                    | runtime-only   | Server layout evidence                               |
| `docs/architecture/overview.md`                       | deprecated-ref | Older transition view                                |
| `docs/architecture/rust-migration.md`                 | review-pool    | Rust migration exploration                           |

## Server: `/home/ubuntu/qintopia-msg-sidecar`

Branch at inventory time: `codex/huabaosi-localization-shadow`

| Source path                                             | Disposition  | Notes                                        |
| ------------------------------------------------------- | ------------ | -------------------------------------------- |
| `docs/data-design/README.md`                            | adopt-input  | Data design process and schema discipline    |
| `docs/data-design/2026-06-24-agent-os-data-layer-v2.md` | adopt-input  | Agent OS data layer reference                |
| `docs/data-design/2026-06-29-erhua-training-memory.md`  | adopt-input  | Erhua trainer memory design input            |
| `docs/operations/context-mcp.md`                        | adopt-input  | Context MCP and source policy reference      |
| `docs/operations/message-store-mcp.md`                  | adopt-input  | Controlled raw message search boundary       |
| `docs/operations/server-deployment.md`                  | runtime-only | Deployment evidence and future runbook input |
| `docs/operations/huabaosi-*-shadow.md`                  | review-pool  | Shadow evidence; not production direction    |
| `docs/operations/huabaosi-*-readonly.md`                | review-pool  | Read-only capture evidence                   |

## Local: `../qintopia-agent-os`

| Source path                                                          | Disposition    | Notes                                                  |
| -------------------------------------------------------------------- | -------------- | ------------------------------------------------------ |
| `docs/README.md`                                                     | adopt-input    | Stable local documentation index                       |
| `docs/architecture.md`                                               | adopt-input    | Current production path and deprecated paths           |
| `docs/product/qintopia-agent-os-prd.md`                              | adopt-input    | Product framing; remove inactive scope before adoption |
| `docs/agent-os/domain-model.md`                                      | adopt-input    | Domain model input                                     |
| `docs/agent-os/agent-contracts.md`                                   | adopt-input    | Agent contract input                                   |
| `docs/agent-os/acceptance-tests.md`                                  | adopt-input    | Acceptance test input                                  |
| `docs/plans/active/agent-os-global-rollout-plan-2026-06-28.md`       | adopt-input    | Migration and rollout input                            |
| `docs/plans/completed/phase0-production-stabilization-2026-06-28.md` | adopt-input    | Production output leak fix evidence                    |
| `docs/reports/server-agent-runtime-inventory-2026-06-29.md`          | adopt-input    | Server runtime migration categories                    |
| `docs/reports/agent-os-internal-sync-2026-07-03.html`                | report-ref     | Internal sync artifact                                 |
| `docs/agent-os/*worktool*`                                           | deprecated-ref | WorkTool historical material                           |
| `docs/agent-os/*openclaw*`                                           | deprecated-ref | OpenClaw historical material                           |
| `docs/agent-os/*kanban*`                                             | deprecated-ref | Hermes Kanban historical material                      |

## Local: `../qintopia-message-sidecar`

| Source path                                                    | Disposition  | Notes                                  |
| -------------------------------------------------------------- | ------------ | -------------------------------------- |
| `docs/data-design/README.md`                                   | adopt-input  | Data design process                    |
| `docs/data-design/2026-06-30-operations-control-plane.md`      | adopt-input  | Operations control plane input         |
| `docs/data-design/2026-07-02-operations-human-actor-guards.md` | adopt-input  | Human actor guard input                |
| `docs/operations/agentos-operations-control-plane.md`          | adopt-input  | Operations control plane runbook input |
| `docs/operations/context-mcp.md`                               | adopt-input  | Context MCP policy                     |
| `docs/operations/message-store-mcp.md`                         | adopt-input  | Message store MCP boundary             |
| `docs/operations/server-deployment.md`                         | runtime-only | Deployment runbook input               |

## Local: `../qiwei-hermes-plugin`

| Source path                                        | Disposition  | Notes                                       |
| -------------------------------------------------- | ------------ | ------------------------------------------- |
| `docs/architecture.md`                             | adopt-input  | QiWe platform plugin architecture input     |
| `docs/operations/server-access.md`                 | runtime-only | Server access evidence; do not copy secrets |
| `docs/plans/active/qiwe-hermes-platform-plugin.md` | adopt-input  | First skill adoption input                  |

## Adoption Notes

- Use local documents as the default canonical input when they conflict with unreviewed
  server-side exploration.
- Treat server-side Rust and Huabaosi shadow documents as review-pool material.
- Treat WorkTool, WorkTool Hermes plugin, OpenClaw, and Hermes Kanban documents as
  deprecated references unless a specific audit task needs them.
- Convert server deployment evidence into runbooks only through git-reviewed docs.
