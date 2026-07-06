# Workflow: Visual Asset Request

`workflows/visual-asset-request` defines the request path from Xiaoman activity needs to
Huabaosi visual draft generation, review, and delivery.

## Responsibility

- Receive a structured visual asset request from an activity workflow.
- Preserve evidence, owner, reviewer, and artifact metadata.
- Keep draft generation separate from publication.
- Require human review before a draft becomes usable production material.

## Production Boundary

- Draft and artifact metadata may be written to Agent OS control-plane tables.
- This workflow must not publish media, send group messages, or update production pages.
- Huabaosi implementation details remain behind reviewed package contracts.

## Acceptance Scenarios

- Valid request creates a draft-needed work item.
- Missing brief or owner creates a needs-human state.
- Draft completion records artifact evidence and review status.
- Review approval marks the artifact as ready for downstream use, not auto-published.

## Validation

```bash
pnpm workflows:check
pnpm check:runtime
```
