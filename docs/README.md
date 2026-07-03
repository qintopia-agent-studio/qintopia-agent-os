# Documentation Hub

[中文](README.zh-CN.md)

This directory is the documentation entrypoint for Qintopia Agent OS. It connects the
current monorepo structure, the server-side documents that were inventoried read-only,
and the local historical documents that still contain useful design evidence.

## Start Here

1. Current architecture:
   [architecture/agent-os-overview.md](architecture/agent-os-overview.md)
2. Product scope: [product/agent-os-prd.md](product/agent-os-prd.md)
3. Agent OS design: [agent-os/README.md](agent-os/README.md)
4. Runtime baseline: [operations/runtime-baseline.md](operations/runtime-baseline.md)
5. Agent capability matrix:
   [operations/agent-capability-matrix.md](operations/agent-capability-matrix.md)
6. Collaboration and PR flow:
   [engineering/collaboration-model.md](engineering/collaboration-model.md)
7. Package contract: [engineering/package-contract.md](engineering/package-contract.md)
8. Migration policy: [engineering/migration-policy.md](engineering/migration-policy.md)
9. Anti-drift policy:
   [engineering/anti-drift-policy.md](engineering/anti-drift-policy.md)
10. Server change policy:
    [engineering/server-change-policy.md](engineering/server-change-policy.md)
11. Source document inventory:
    [operations/source-document-inventory.md](operations/source-document-inventory.md)
12. M1 source inventory:
    [operations/inventory/README.md](operations/inventory/README.md)
13. M9 server cutover runbook:
    [operations/m9-server-cutover-runbook.md](operations/m9-server-cutover-runbook.md)
14. Reports index: [reports/README.md](reports/README.md)
15. Active migration plan:
    [plans/active/monorepo-migration.md](plans/active/monorepo-migration.md)

## Directory Map

```text
docs/
├── README.md                         # Documentation entrypoint
├── README.zh-CN.md                   # Chinese entrypoint
├── architecture/                     # System boundaries and data flow
├── engineering/                      # Collaboration, package, migration rules
├── operations/                       # Server evidence and operating references
├── plans/                            # Active and completed execution plans
├── product/                          # Product and business documents
├── reports/                          # Internal sync reports and generated reports
└── agent-os/                         # Domain model, contracts, acceptance tests
```

## Source Policy

Documents from the production server are evidence, not automatic product decisions. They
must be classified before adoption:

- `adopt`: stable enough to become a canonical monorepo document or package input.
- `template`: useful as a template after removing live state and secrets.
- `runtime-only`: describes live deployment state and should remain operational
  evidence.
- `review-pool`: useful exploration that needs owner review before becoming direction.
- `deprecated`: historical material kept for audit or migration reference only.
- `remove`: material with no future product or audit value.

Server-side Rust and Huabaosi shadow documents are currently treated as `review-pool`
unless an owner explicitly approves the direction. WorkTool and Hermes Kanban material
are not future product paths.

## Documentation Rules

- Keep stable rules in root docs and `docs/engineering/`.
- Keep changing migration status in
  [plans/active/monorepo-migration.md](plans/active/monorepo-migration.md).
- Keep source inventories in [operations/](operations/).
- Do not copy secrets, live runtime state, raw member profiles, or private chat logs
  into documentation.
- Link to source documents by path and disposition instead of copying large historical
  documents wholesale.
