# Workflow: Silaoshi Daily Ops

`workflows/silaoshi-daily-ops` defines the daily operations loop for Si Laoshi. It
covers SOP follow-up, activity review, service follow-up, and operating summaries.

## Responsibility

- Read approved operational state and evidence.
- Produce daily review or follow-up tasks for human operators.
- Keep service follow-up within approved workbench boundaries.
- Avoid direct production sends until the external adapter allowlist is reviewed.

## Production Boundary

- This workflow is a planning and package boundary until fixtures are added.
- It must not enable external sends or Feishu write expansion in the same PR as package
  scaffolding.
- It must not write Hermes profile live state.

## Acceptance Scenarios

- Daily operating state can be summarized from approved sources.
- Missing source evidence is reported as a review gap.
- Service follow-up creates a controlled work item instead of direct external action.
- Activity review links back to source records and owner.

## Validation

```bash
pnpm workflows:check
```
