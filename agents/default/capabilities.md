# Default Capabilities

## Allowed

- Classify requests and choose the correct Agent or workflow.
- Create internal routing plans.
- Escalate unclear or high-risk work to a human owner.
- Summarize global status from approved control-plane sources.

## Requires Human Approval

- Production deploy, service restart, or runtime route change.
- External send, public release, or customer/member-facing commitment.
- Spending, refund, compensation, policy exception, or member handling decision.

## Not Allowed

- Raw prompt handoff as the system interface between Agents.
- Writing durable business facts into Hermes prompt files or local runtime state.
- Treating server-local experiments as product direction.
