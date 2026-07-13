# Xiaoman Event Signal Mutations

Schema version: `2026-07-14.001`  
Migration: `migrations/202607140001_xiaoman_event_signal_mutations.sql`  
Status: proposed  
Date: 2026-07-14

## Purpose

Make Xiaoman's `status-update` and `gap-update` operations mutate the AgentOS fact
source instead of a Feishu activity row. `qintopia_agent_os.event_signals` already owns
actionable event triage, assignment, closure, and retrospective state. Feishu remains a
human workbench and mirror.

## Contract

Both operations require:

- `actor_agent=xiaoman`;
- an internal UUID `event_signal_id` owned by Xiaoman;
- an explicit UUID `mutation_id` for replay safety; and
- exactly one allowlisted field mutation.

`status-update` accepts only `待处理`, `处理中`, `已完成`, or `已关闭`. `已完成` and
`已关闭` are terminal for this command. Automated stale-event retirement remains a
separate system-owned transition.

`gap-update` accepts a non-empty, sanitized summary of at most 500 characters. It writes
the dedicated `event_signals.gap_summary` column and does not overwrite the extracted
event `summary`.

## Audit And Idempotency

`qintopia_agent_os.event_signal_mutations` is append-only. Each row records the event,
operation, derived idempotency key, actor, previous value, and new value. Raw chat,
Feishu record ids, table ids, tokens, prompts, and payload dumps are not stored.

The sidecar locks the target event signal, checks an existing idempotency key, validates
that a replay has the same operation and value, updates the event, and inserts the audit
row in one transaction. A conflicting reuse of `mutation_id` fails without changing the
event. Replaying the same request returns the existing mutation without another update
or audit row.

## Privacy And Production Boundary

- Postgres reads: one Xiaoman-owned `event_signals` row and its mutation key.
- Postgres writes: one allowlisted event field and one mutation audit row.
- Feishu reads/writes: none.
- QiWe sends: none.
- External adapters: none.
- Hermes profile state: unchanged.

The migration is additive. Rollback uses the previous immutable sidecar; the new column
and audit rows may remain in place because older runtimes ignore them.

The migration also backfills the missing `schema_change_log` record for the already
immutable `202607130002_huabaosi_image_generation.sql` migration. It inserts only when
that version is absent, so an existing audit record is not rewritten. It does not edit
the historical migration or change its checksum.

## Validation

- Unit tests cover payload requirements, one-field-only mutations, status allowlist and
  transition rules, gap sanitization, stable idempotency keys, and safe report output.
- The disposable PostgreSQL apply smoke verifies one status update and one gap update,
  exact audit rows, replay without duplication, conflicting mutation ids, and no Feishu
  or external side effects.
- Schema preflight requires the new table and schema-change record after deployment.
