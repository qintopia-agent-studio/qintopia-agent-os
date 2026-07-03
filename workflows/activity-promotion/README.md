# Workflow: Activity Promotion

`workflows/activity-promotion` is the governed multi-Agent workflow for turning activity
signals into reviewed operating assets and controlled group-send readiness.

## Current Source

- Local source: `../qintopia-message-sidecar/src/operations.rs`
- Supporting modules: `src/collaboration.rs`, `src/evidence.rs`,
  `src/group_message_send.rs`, `src/workbench.rs`, `src/xiaoman_activity.rs`
- Operations doc:
  `../qintopia-message-sidecar/docs/operations/agentos-operations-control-plane.md`
- Adopted reference: `eda2652f21999e4f32699463413372accbd3b76e`

## Responsibility

The workflow coordinates Xiaoman activity requests, Wenyuange evidence retrieval,
Huabaosi visual asset work, human review, and Erhua group-message readiness. It is a
control-plane workflow, not an autonomous publish path.

## Required Human Gates

- Visual artifacts need review before use.
- Group message requests need final human confirmation before send readiness.
- Allowlists control group targets, reviewers, confirmers, owners, and attachment hosts
  when configured.

## Boundaries

- Writes Postgres work items, events, artifacts, and workflow summaries.
- Does not directly call Feishu, QiWe, Huabaosi, Wenyuange, or external send adapters in
  dry-run and current apply smoke paths.
- Must not use Hermes Kanban as the future orchestration backbone.

## Validation

Use the local no-credential smoke first:

```bash
cd ../qintopia-message-sidecar
scripts/operations-control-plane-smoke.sh
```

The Postgres apply smoke is guarded and must only run with explicit owner approval and
configured database credentials.
