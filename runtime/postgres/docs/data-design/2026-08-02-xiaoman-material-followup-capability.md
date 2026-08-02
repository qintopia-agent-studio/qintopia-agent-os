# Xiaoman Material Follow-Up Capability

Schema version: `2026-08-02.001`  
Migration: `migrations/202608020001_xiaoman_material_followup_capability.sql`

## Purpose

Register a Postgres-owned AgentOS capability for Xiaoman activity material follow-up.
The material follow-up worker scans sanitized `activity_occurrence` records and creates
internal `activity_recap_request` work items for T+24/T+48/T+72 reminder handling.

The third attempt is represented as an operations-lead escalation draft through work
item priority and payload metadata. It is not a group-send authorization.

## Contract

`xiaoman.material_followup_request` is provided by Xiaoman, may be requested only by
Xiaoman, and may create only `activity_recap_request` work items. Requests must bind a
sanitized `source_record_ref`, `material_followup_attempt`, `escalation_required`, and
`external_send_executed=false`.

The worker idempotency key remains bound to the business scan date, sanitized source
record reference, and follow-up attempt so replay returns the existing work item instead
of creating another reminder.

## Boundary

- Postgres writes: one internal AgentOS work item and one creation audit event.
- Feishu reads: allowed only through the existing allowlisted read boundary.
- Feishu writes: none.
- Erhua/QiWe sends: none.
- Group-message request creation: none.
- Approved text announcement handoff: remains a separate reviewed step before any
  `erhua.send_group_message` work item can exist.

Rollback can use the previous immutable sidecar. The additive capability row may remain;
older workers do not call it.
