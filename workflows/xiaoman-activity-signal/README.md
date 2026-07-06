# Workflow: Xiaoman Activity Signal

`workflows/xiaoman-activity-signal` defines how Xiaoman turns activity signals into
reviewable Agent OS state.

## Responsibility

- Detect activity signals and map them to activity records.
- Track status transitions without treating Feishu as the system source of truth.
- Trigger downstream work requests only when required fields are present.
- Keep Xiaoman's path read-only or database-scoped until owner-reviewed runtime config
  enables more behavior.

## Production Boundary

- This workflow can write Agent OS control-plane rows after the sidecar contract is
  used.
- It must not directly send external messages.
- It must not create visual assets or group-send drafts by itself.

## Acceptance Scenarios

- New activity signal creates or updates one activity record idempotently.
- Duplicate signal does not create duplicate activity state.
- Missing required fields produce a review-needed state.
- Valid signal can request a visual asset workflow without publishing anything.

## Validation

```bash
pnpm workflows:check
pnpm check:runtime
```
