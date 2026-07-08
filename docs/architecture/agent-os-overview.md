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
- Postgres and Agent OS data structures are the system fact source.
- Feishu is a human workbench and mirror. It is not the long-term source of truth.
- Sidecars and workers handle slower or isolated work. They must not block the initial
  QiWe / WeCom acknowledgement path.
- High-risk actions need human review or explicit confirmation before external delivery.
- Raw prompt handoff is not a reliable system interface. Use governed capabilities, work
  items, artifacts, events, and review records.
- WorkTool and Hermes Kanban are deprecated for future product development.
- Server-side Rust and Huabaosi shadow work is review-pool material until owner
  approval.

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
