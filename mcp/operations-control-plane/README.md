# MCP: Operations Control Plane

`mcp/operations-control-plane` is the schema-bound MCP contract for AgentOS operations
workflows and work items.

Agents should use this boundary when they need to start an operations workflow, create a
controlled work item, or inspect operations status. They should not shell out to raw
sidecar commands, mutate Postgres directly, or pass free-form prompts to another Agent
as a substitute for work-item state.

## Tools

- `qintopia_operations_workflow_start`
- `qintopia_operations_work_item_create`
- `qintopia_operations_status`

## Current Implementation

The first implementation is a dry-run command wrapper:

```bash
mcp/operations-control-plane/bin/qintopia-operations-control-plane-mcp \
  --tool qintopia_operations_workflow_start \
  --args-json '{"workflow_type":"activity_promotion","requester_agent":"xiaoman","source_type":"xiaoman_activity","source_refs":{"source_record_ref":"activity_plan:demo"}}'
```

It validates known operation shapes and returns the work items that would be created. It
does not write Postgres, create Feishu tasks, call QiWe, run workers, or send messages.

## Production Boundary

- `qintopia_operations_workflow_start` may preview workflow roots and child work items.
- `qintopia_operations_work_item_create` may preview one controlled work item.
- `qintopia_operations_status` may preview a bounded status lookup request.
- Apply mode is intentionally rejected until the runtime sidecar owns the audited DB
  write path for this MCP surface.
- External sends, Feishu writes, QiWe sends, worker execution, and raw SQL are out of
  scope.
- Production will need database secrets, but secrets are not read by this dry-run
  wrapper.

## Validation

```bash
pnpm mcp:operations-control-plane:check
pnpm mcp:adapters:check
pnpm registry:check
```
