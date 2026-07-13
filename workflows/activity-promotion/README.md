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
Huabaosi visual asset work, human review, optional image-generation request intake, and
Erhua group-message readiness. It is a control-plane workflow, not an autonomous publish
path.

## Required Human Gates

- Visual artifacts need review before use.
- An approved `poster_brief` may create an `image_generation_request`, but the image
  provider and isolated media storage remain disabled until separately owner-reviewed.
- A future `generated_image` must remain pending human review before any downstream use.
- Group message requests need final human confirmation before send readiness.
- Allowlists control group targets, reviewers, confirmers, owners, and attachment hosts
  when configured.

## Boundaries

- Writes Postgres work items, events, artifacts, and workflow summaries.
- Does not directly call Feishu, QiWe, Huabaosi image providers, Wenyuange, or external
  send adapters in dry-run and current apply smoke paths.
- Must not use Hermes Kanban as the future orchestration backbone.

## Production Boundary

This workflow is already active as a control-plane package, but group-send execution and
external publication remain outside the current approved boundary. Any change that
enables real external sends needs owner review, allowlist evidence, smoke output, and
rollback notes.

## Acceptance Scenarios

- Activity signal creates a governed work item without sending an external message.
- Evidence lookup records source basis and risk notes.
- Visual asset work records artifact evidence and review state.
- Group-send readiness requires final human confirmation before any external send path
  is considered.

## Validation

Use the local no-credential smoke first:

```bash
deploy/sidecar/scripts/operations-control-plane-smoke.sh
```

The Postgres apply smoke is guarded and must only run with explicit owner approval and
configured database credentials.
