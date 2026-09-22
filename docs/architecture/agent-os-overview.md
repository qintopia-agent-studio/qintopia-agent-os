# Agent OS Architecture Overview

This document records the current architecture baseline for Qintopia Agent OS. It is
based on the monorepo package structure, completed migration evidence, and the
release/current production model.

## Architecture

```text
External channels
  QiWe / WeCom / Feishu / webhook / cron / manual operator input
        |
        v
Channel adapters and ingress guards
  auth, dedupe, normalization, channel safety, app-level boundary checks
        |
        v
Hermes runtime and Agent profiles
  Erhua / Xiaoman / Huabaosi / Wenyuange / Silaoshi / Guanerye
        |
        v
Agent OS control and data plane
  capabilities, work_items, artifacts, work_item_events, human_workbench_refs
        |
        v
Workers, MCP adapters, and sidecars
  release/current services, context lookup, message store, Feishu Base, Postgres,
  artifact generation
        |
        v
Human workbench and external systems
  Feishu review, QiWe replies, reports, deployment evidence, audit records
```

## Boundary Decisions

- Hermes is the Agent runtime. It should execute profiles and tools, but it should not
  become the business database.
- Agent OS owns shared identity links, scope, authority, governed knowledge versions,
  work items, and execution evidence. PMS remains authoritative for orders, stays,
  inventory, and financial facts; Agent OS consumes necessary references and
  projections.
- Feishu Base remains an application intake and operations surface. Feishu documents may
  own authored content; governed versions and access rules determine how Agents use it.
  Do not treat every Feishu resource as a mirror or move all source facts into Agent OS.
- Sidecars and workers handle slower or isolated work. They must not block the initial
  QiWe / WeCom acknowledgement path.
- Apply each capability's human-review requirements and valid explicit authorization. A
  standing authorization covers its agreed actions without another per-item approval; it
  does not authorize unrelated operations or bypass current scope and disclosure checks.
- Raw prompt handoff is not a reliable system interface. Use governed capabilities, work
  items, artifacts, events, and review records.
- WorkTool and Hermes Kanban are deprecated for future product development.
- Server-side Rust and Huabaosi shadow work is review-pool material until owner
  approval.

## Scoped Knowledge And Runtime Responsibilities

Community, building, and business-domain knowledge have explicit ownership and scope.
Their maintainers use conversation and UI entrypoints backed by the same governed
services. Buildings can consume applicable shared community content while preserving
maintenance permissions and restricted audiences. This does not require a physical
database or Runtime per building.

Agents interpret requests, advise, and invoke tools within their roles. Hermes hosts
models, sessions, channels, tools, and local runtime context; Profile memory does not
replace shared business authority. Recurring triggers retain the existing
[Hermes cron ownership](../operations/hermes-cron-source-of-truth.md).

The
[community business model](../agent-os/community-business-model.md#9-社区楼栋与领域知识的分层维护)
records the current knowledge design, implementation gaps, and staged acceptance. Scope
configuration and retrieval foundations exist; full knowledge inheritance and shared
conversation/UI editing are not yet connected. 岸岸 is a planned room Agent and is not
yet an independently registered Agent package or Hermes Profile.

## Control Plane Objects

The target control plane should make Agent work observable and recoverable:

- `capabilities`: registered skills, workflows, adapters, and risk boundaries.
- `work_items`: durable units of work that can be assigned, retried, reviewed, and
  audited.
- `artifacts`: generated or collected outputs with evidence and provenance.
- `work_item_events`: status, tool, handoff, review, and delivery events.
- `human_workbench_refs`: Feishu or other human-facing references for review and action.

## Monorepo Mapping

| Architecture area        | Monorepo location                         |
| ------------------------ | ----------------------------------------- |
| Agent profiles           | `agents/<agent>/`                         |
| Reusable capabilities    | `skills/<capability>/`                    |
| Cross-Agent processes    | `workflows/<workflow>/`                   |
| MCP servers and adapters | `mcp/<adapter>/`                          |
| Runtime templates        | `runtime/<area>/`                         |
| Deploy and rollback      | `deploy/<area>/`                          |
| Replay and evidence      | `fixtures/<area>/`                        |
| Historical material      | `deprecated/<topic>/`                     |
| Architecture and rules   | `docs/architecture/`, `docs/engineering/` |

## Current Engineering Direction

The monorepo migration and sidecar release/current cutover are complete. Current work
should harden package contracts, profile/plugin bundle distribution, external adapter
allowlists, and owner-approved archive retention without reviving deprecated WorkTool,
OpenClaw, or Hermes Kanban paths.

Production runtime changes should move through reviewed artifacts, deploy requests,
smoke checks, rollback notes, and the stable
`/home/ubuntu/qintopia-agent-os-releases/current` symlink instead of server-local source
edits or standalone checkouts.

Conversation-aware asynchronous work follows the authenticated message-first boundary
defined in [Xiaoman Conversation Ingress V3](xiaoman-conversation-ingress-v3.md): Hermes
supplies the channel runtime, while Postgres policy and AgentOS work items own
authorization, routing, recovery, and audit.
