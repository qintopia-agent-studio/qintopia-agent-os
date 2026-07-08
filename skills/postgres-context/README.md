# Postgres Context Skill

This package owns the Agent-facing contract for Postgres-backed context. It is the
starting point for changes to safe member context, Erhua answer context, and audited
trainer-memory writes.

The active runtime implementation still lives in `runtime/sidecar/src/context_tools.rs`
and is exposed through `mcp/context-server`. That is intentional for now: this
capability touches Postgres reads, controlled Postgres writes, audit rows, and profile
safety boundaries, so the sidecar remains the execution layer while this package owns
the contract, fixtures, validation, and change routing.

## Capability

- return only safe member or context snapshots;
- prepare Erhua reply context without exposing raw profile data;
- resolve speaker and mentioned-member context for direct chat and group mention flows;
- write only audited Erhua trainer-memory notes through allowlisted trainer IDs;
- keep write-capable operations explicit, auditable, and revocable;
- block unrestricted SQL, raw chat logs, live `.env` files, and Hermes live state.

## Tool Contract

| Tool                                  | Mode      | Owner surface                | Runtime implementation                 |
| ------------------------------------- | --------- | ---------------------------- | -------------------------------------- |
| `qintopia_member_context_lookup`      | read-only | safe member reply context    | `runtime/sidecar/src/context_tools.rs` |
| `qintopia_answer_context_prepare`     | read-only | Erhua answer context package | `runtime/sidecar/src/context_tools.rs` |
| `qintopia_erhua_training_note_submit` | write     | audited trainer memory       | `runtime/sidecar/src/context_tools.rs` |

`qintopia_wenyuange_lookup`, GIS lookup, and external disclosure filtering currently
share the same sidecar module but are not owned by this package. WenYuanGe/Dify evidence
belongs to `skills/knowledge-retrieval`; provider or database adapter rules belong under
`mcp/`.

## Read Boundary

Read tools may return:

- `safe_summary`;
- `communication_style`;
- `safe_reply_hints`;
- selected active training guidance;
- source ids and audit-friendly metadata.

For QiWe speaker identity in `qintopia_answer_context_prepare`, `sender_id` is the
current QiWe user id in both group mentions and direct chats. The read path first
resolves the exact `platform + chat_id + channel_user_id` identity, then may use only
the materialized QiWe platform identity
`platform='qiwe' + chat_id='' + channel_user_id`. It must not pick the most recent
cross-chat row. Cross-chat continuity is produced asynchronously by identity workers,
which materialize `chat_id=''` only when the linked person is unambiguous.

Mentioned-member resolution must be deterministic and safe:

- display names, Chinese aliases, and channel mention text may be used as lookup inputs;
- exact or alias matches may resolve a single member;
- ambiguous matches must be returned as ambiguous context so Erhua can clarify;
- missing matches must be returned as unresolved context so Erhua does not invent;
- vector or message-history search must not be used to guess member identity.

`qintopia_answer_context_prepare` should also return routing guidance for the current
message. Member/self-identity questions use member context, public facts use approved
knowledge, discussion-history questions may use message evidence, and live operations
questions require human/live-ops handoff.

Read tools must not return:

- raw message text outside an approved evidence contract;
- hidden profile details;
- raw member facts or private notes;
- pending, rejected, or revoked trainer notes;
- profile source internals that would teach Erhua to claim surveillance.

Every member-context read must write `qintopia_identity.member_context_audit`.

## Write Boundary

`qintopia_erhua_training_note_submit` is the only write-capable tool in this capability.
It must:

- require `caller_profile=erhua`;
- require `trainer_user_id` to be in `QINTOPIA_ERHUA_TRAINER_USER_IDS`;
- restrict `training_type` to `member_preference`, `member_fact`, `reply_example`, or
  `persona_rule`;
- sanitize summaries before returning or applying them;
- reject sensitive or boundary-overriding training;
- store audit metadata and applied artifact pointers;
- keep notes revocable through `status='revoked'` and `revoked_at`.

The current V1 table does not yet have a dedicated idempotency key. Until that is added,
callers should pass `source_platform_message_id`, and future schema work must add an
explicit idempotency constraint before broadening write usage.

## Database Tables

Primary tables:

- `qintopia_identity.persons`
- `qintopia_identity.channel_identities`
- `qintopia_identity.member_profile_snapshots`
- `qintopia_identity.member_context_audit`
- `qintopia_identity.erhua_training_notes`
- `qintopia_identity.erhua_persona_overlays`
- `qintopia_identity.member_facts`

Design docs:

- `runtime/postgres/docs/data-design/2026-06-24-agent-os-data-layer-v2.md`
- `runtime/postgres/docs/data-design/2026-06-26-profile-digest-archive-v1.md`
- `runtime/postgres/docs/data-design/2026-06-29-erhua-training-memory.md`

Migrations:

- `runtime/postgres/migrations/202606240002_agent_os_data_layer.sql`
- `runtime/postgres/migrations/202606260004_profile_digest_archive_v1.sql`
- `runtime/postgres/migrations/202606290006_erhua_training_memory.sql`

## Fixtures

Fixtures under `fixtures/` are sanitized contract examples:

- `fixtures/member-context-lookup.json`
- `fixtures/answer-context-prepare.json`
- `fixtures/training-note-submit-allowed.json`
- `fixtures/training-note-submit-blocked.json`

They are not production exports and must not contain private chat logs or live member
profiles.

## Production Boundary

- External sends: none.
- Database reads: yes, through sidecar/context MCP only.
- Database writes: yes, only audited trainer-memory writes.
- Hermes profile runtime: no direct mutation.
- Secrets: runtime-only Postgres connection strings and trainer allowlists stay outside
  git.

Do not expose unrestricted SQL or generic Postgres MCP access to frontend Agents.

## Validation

```bash
pnpm skills:postgres-context:check
pnpm mcp:adapters:check
pnpm test:sidecar
pnpm check:light
```
