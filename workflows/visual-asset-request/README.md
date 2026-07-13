# Workflow: Visual Asset Request

`workflows/visual-asset-request` defines the request path from Xiaoman activity needs
to 阿靓（Huabaosi / 画报司）visual brief generation, review, and delivery.

## Responsibility

- Receive a structured visual asset request from an activity workflow.
- Wait for the sibling `evidence_summary` for activity-promotion requests, so a visual
  brief is not created from ungrounded activity context.
- Preserve evidence, owner, reviewer, and artifact metadata.
- Keep draft generation separate from publication.
- Require human review before a draft becomes usable production material.

## Production Boundary

- Draft and artifact metadata may be written to Agent OS control-plane tables.
- The active internal worker creates a pending `poster_brief`, not an image file.
- This workflow must not publish media, send group messages, or update production pages.
- Huabaosi implementation details remain behind reviewed package contracts.

## Acceptance Scenarios

- Valid request creates a draft-needed work item.
- An activity-promotion visual request remains queued until its matching evidence
  request has produced a completed `evidence_summary`.
- Missing brief or owner creates a needs-human state.
- Draft completion records artifact evidence and review status.
- Review approval marks the artifact as ready for downstream use, not auto-published.

## Validation

```bash
pnpm workflows:check
pnpm check:runtime
bash -n deploy/sidecar/scripts/operations-control-plane-apply-smoke.sh
```
