# Workflow: Erhua Consultation

`workflows/erhua-consultation` defines the controlled group-consultation path for Erhua.
It covers mention-triggered replies, evidence gaps, complaint intake, and human handoff.

## Responsibility

- Reply only when Erhua is explicitly mentioned or cued by the group context.
- Use safe evidence from approved skills and MCP adapters.
- Escalate missing source, complaint, approval, or high-risk operations to a human
  operator.
- Keep external send behavior behind the QiWe/Hermes gateway guardrails.

## Production Boundary

- This workflow is documentation and acceptance-contract only until its replay fixtures
  are wired into CI.
- It must not broaden Erhua group-send triggers or bypass mention/send guards.
- It must not write live Hermes memory or profile state directly.

## Acceptance Scenarios

- Group mention with enough source evidence returns an answer basis.
- Group mention without enough source evidence produces an escalation note.
- Complaint-like input opens a human handoff path instead of autonomous resolution.
- Private or non-mention group text does not trigger a group reply.

## Validation

```bash
pnpm workflows:check
pnpm test:qiwe
```
