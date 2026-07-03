# 2026-07-02.001 Operations Human Actor Guards

## Purpose

This migration makes the human side of the AgentOS operations control plane explicit at
the database layer. Human-facing fields must not store Feishu app ids, bot ids, service
ids, or worker identities.

This is a guardrail-only change. It does not enable Feishu, QiWe, Huabaosi, Wenyuange,
Erhua, or external send adapters.

## Scope

New database function:

- `qintopia_agent_os.is_human_actor_id(actor_id text)`

New constraints:

- `work_items.human_owner` must be empty or a human actor id.
- `artifacts.reviewed_by` must be null/empty or a human actor id.
- `work_item_events.actor_id` must be a human actor id when `actor_type = 'human'`.

The guard rejects ids starting with `cli_`, `app_`, `bot_`, or
`system`/`service`/`worker` prefixes. Empty `human_owner` remains valid so headless work
items can stay unassigned or待认领.

## Rationale

Feishu app/bot identities can create, sync, or mirror records, but they must not be
treated as human owners, reviewers, or final confirmers. Agent identity stays in AgentOS
fields such as `requester_agent`, `target_agent`, `capability_key`, `created_by_agent`,
and system event actor ids.

## Compatibility

The migration is additive. It is safe for current headless control-plane work because
existing system and worker events use `actor_type = 'system'` or `actor_type = 'agent'`,
not `actor_type = 'human'`.

Before applying to a live database, run a read-only check for existing rows that would
violate the new constraints:

```sql
SELECT id, human_owner
FROM qintopia_agent_os.work_items
WHERE NOT qintopia_agent_os.is_human_actor_id(human_owner);

SELECT id, reviewed_by
FROM qintopia_agent_os.artifacts
WHERE NOT qintopia_agent_os.is_human_actor_id(reviewed_by);

SELECT id, actor_type, actor_id
FROM qintopia_agent_os.work_item_events
WHERE actor_type = 'human'
  AND NOT qintopia_agent_os.is_human_actor_id(actor_id);
```

## Acceptance Checks

- `cargo test operations`
- `scripts/operations-control-plane-smoke.sh`
- Guarded Postgres apply smoke before deployment:
  `QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE=1 scripts/operations-control-plane-apply-smoke.sh`
