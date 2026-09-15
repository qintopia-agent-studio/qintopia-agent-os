# 2026-08-14.001 Erhua Conversational Self-Extension

## Purpose

Add the smallest Postgres contract needed for per-group Erhua policy, declarative
business definitions, event or schedule automations, and reusable channel-event
mappings. The migration is infrastructure-only and does not activate or execute a
business rule.

## Space Model

`qintopia_messages.conversations.id` is the v1 Space identifier. A channel room is
resolved by the existing unique key `(tenant_id, platform, chat_id)`. Authorization and
runtime routing always use the immutable UUID, not `display_name`.

Nullable `space_id` foreign keys are added to `qintopia_messages.raw_events` and
`qintopia_agent_os.work_items`. Existing rows remain compatible. New Space-aware paths
must populate the key before policy lookup or execution.

`raw_events.ingress_auth_verified` defaults to false. It is an adapter-derived trust
fact, separate from the provider payload. Duplicate capture can promote this flag from
false to true but cannot downgrade it. Only authenticated rows may enter the provider
event interpreter or back an activation-grade shadow observation.

## Version Tables

All version rows use UUID public identifiers, a positive integer version, a canonical
SHA-256 digest, status timestamps, actor provenance, and status constrained to `draft`,
`shadow`, `active`, `paused`, or `retired`.

### `space_policy_versions`

One stream per `(space_id, definition_key)`. `policy_config` contains bounded identity,
knowledge-scope, capability-grant, and quota declarations. It cannot grant a capability
that the platform registry does not expose.

An active policy proposal may include `capability_revocations` only as an additive
change instruction. Materialization starts from the current active policy, rejects
revocation of a capability that is not currently granted and rejects granting and
revoking the same key, then persists the complete final `capability_grants` set without
the revocation field. `quota_declaration` is schema-reserved for future enforcement and
is accepted only as `{"enforcement":"reserved_non_enforced","limits":{...}}`; v1
validates and records bounded positive limits but does not enforce them.

As a defense-in-depth boundary, the authoritative runtime projection still subtracts
every `capability_revocations` entry from `capability_grants` if an active row ever
contains both. A stale or independently written overlap therefore cannot restore a
revoked capability.

The active `default` stream also governs Erhua's ordinary QiWe group turn. The internal
`space_turn_policy_context` operation resolves the current active conversation by exact
`chat_id` and returns only `identity`, `knowledge_scope`, and the effective capability
intersection. Although it is read-only, it uses the same authenticated persisted-message
receipt as capability authorization so another same-UID socket client cannot enumerate
another Space's policy by forging a group id. A missing active default policy returns an
explicit `policy_found=false` with empty values; query or schema failures reject the
call and never degrade into an unscoped turn.

### `business_definition_versions`

One stream per `(space_id, definition_key)`. `execution_mode` is `deterministic` or
`agent_turn`; `definition` owns the generic input/output contract;
`allowed_capabilities` is an explicit ceiling; and `approval_policy` records the
required review boundary.

### `automation_definition_versions`

One stream per `(space_id, definition_key)`. Each version binds an exact
`business_definition_versions.id`. A composite foreign key over
`(business_definition_id, space_id)` enforces that the business belongs to the same
Space even for future write paths. `trigger_kind` is `event` or `schedule`;
`trigger_config` is declarative; `timezone` defaults to `Asia/Shanghai`;
`misfire_policy` is constrained to `run_once` in v1; and `next_run_at` plus
`last_dispatched_at` support one generic dispatcher. `skip` and `catch_up` are reserved
for a future reviewed schema and runtime version and are rejected by the current
proposal validator and database constraint.

The schedule idempotency contract is
`automation:<automation-definition-id>:<scheduled-for-utc>`.

The dispatcher validates cron, timezone, and misfire policy before creating a work item
or advancing a cursor. Invalid active configuration fails the apply transaction closed;
the dispatcher does not mutate the immutable version into `paused`. Recovery requires a
reviewed pause or replacement definition operation.

The conversation planner may use a bounded `definition_operation` alias for an existing
automation. `activate` binds the latest shadow automation and its exact business and
event-mapping versions by UUID, digest, status, and stream head; it never accepts a
model-supplied schedule or dependency snapshot. An event activation additionally
requires authenticated real-event evidence scoped to that exact shadow automation and
Space. Required shadow dependencies are copied into new active versions, while exact
already-active dependencies are reused. A conflicting active business or provider
mapping fails closed instead of being silently replaced. `agent_turn` targets may be
activated through the ordinary exact-shadow operation. Their execution is available only
through the default-disabled dedicated runner contract added by `2026-08-15.001`; the
global capability and both owner-gated runtime boundaries must still be enabled
separately. `pause` snapshots the active version into a new `paused` version. `rollback`
copies an explicit historical version, or the immediately previous version when omitted,
into a new `active` version. The exact historical business and event-mapping rows must
still be active; otherwise the operation fails and requires an explicit compound change.
These aliases add no mutable-history column or new table.

### `channel_event_mapping_versions`

One provider-level stream per `(provider, definition_key)`. `selector` and `extractor`
contain the bounded event DSL; `official_sources` records the allowlisted source pages;
and `validation_evidence` records fixture and shadow evidence. The mapping output cannot
carry a target room or destination. Event runtime derives the source room and resolves
it to a Space before selecting an automation.

### Execution capabilities

`erhua.execute_space_business` is the queue gate. A deterministic definition names a
registered primitive in `definition.capability_key`; v1 registers only
`erhua.qiwe_text_template`. The follow-up `2026-08-15.001` migration moves deterministic
selection to a closed recipe registry. `agent_turn` hands off through
`erhua.space_agent_turn` with the exact business-owned output contract. Both
sub-capabilities accept only the system executor as caller, and all three capabilities
are seeded disabled.

Runtime execution intersects the business `allowed_capabilities`, the active default
Space policy `capability_grants`, and globally enabled capability rows. The capability
row must explicitly declare `space_invocable=true` and
`space_scope_binding=work_item_space_id`; this is checked both when a business is
confirmed and when it is claimed. The schedule dispatcher applies the same selected-row
gate before queueing, including `enabled`, provider, caller, mode-specific work-item
type, and invocation-boundary checks. QiWe delivery derives its target only through
`work_items.space_id -> conversations.chat_id`. Event subject ids remain opaque strings
and are intersected with one exact current-room roster before a single combined template
render. The exact room response also refreshes the validated
`conversations.display_name`; authorization continues to use ids only. The attempt event
is committed before room lookup or send; an uncertain send is terminal and cannot be
claimed again.

### Ordinary-turn capabilities

Ordinary group-turn capabilities use a separate fixed catalog and do not reuse the
automation executor boundary. The migration registers seven existing QiWe native-tool
capabilities plus public/community knowledge and complaint/sales workflow categories.
Every row is disabled by default and must declare all of:

- `provider_agent = 'erhua'`;
- caller `erhua` and work-item type `qiwe_group_turn`;
- `metadata.space_turn_invocable = true`;
- `metadata.space_scope_binding = 'trusted_session_space_id'`; and
- `metadata.invocation_boundary = 'erhua.space_turn'`.

`effective_capabilities` is the intersection of this fixed catalog, the active default
policy's `capability_grants`, and matching globally enabled registry rows. Both internal
operations require the authenticated persisted source-message receipt through the
existing trusted Space session resolver. Before every native QiWe tool invocation,
`space_turn_capability_authorize` repeats the intersection for one capability and
returns only the capability key and an authorization boolean. The context and
authorization responses never expose the conversation UUID, provider group id, or
operator id. Space configuration prepare/confirm/status tools keep their existing
administrator and confirmation boundary and are not governed by ordinary-turn capability
grants.

`erhua.knowledge.community` is additionally excluded unless its enabled registry row
declares `metadata.knowledge_scope_enforced=true`. The migration seeds that metadata as
false, so a policy grant alone cannot enable community knowledge before the backend
enforces the current Space knowledge scope. `erhua.knowledge.public` is always invoked
with the fixed `Public` scope; model-supplied internal or member-scoped values cannot
widen it.

## Version Invariants

- `(scope, definition_key, version)` is unique.
- Version is positive and increases under a transaction-level advisory lock.
- A partial unique index permits at most one `active` row per stream.
- Canonically equal proposals reuse the existing digest instead of creating churn.
- Activating a version retires the previous active version in the same transaction.
- Automation proposals pin same-proposal dependencies by digest and existing
  dependencies by id, digest, and stream-head version; confirmation rejects drift.
- Activation proposals pin the exact shadow automation plus its business and optional
  provider mapping by id, digest, status, and stream-head version. Confirmation locks
  and rechecks each stream before creating active copies.
- Pause and rollback aliases also pin the automation stream head; confirmation holds the
  stream advisory lock while rechecking it and rejects an intervening version.
- An active business or provider mapping cannot be retired while an active or shadow
  automation remains bound to it without a same-proposal migration or pause. Provider
  mapping replacement also rejects references from every other Space.
- Pause and rollback are new immutable versions; old rows are never overwritten into a
  different definition.

## Proposal And Approval Reuse

No proposal, approval, or execution table is added. A conversational change uses:

- `work_items.work_item_type = 'space_change_request'`;
- capability `erhua.manage_space_configuration`;
- an `awaiting_review` work item scoped by `space_id`;
- one pending `space_change_proposal` artifact containing the bounded change set; and
- append-only, allowlisted `work_item_events` for prepare, denial, activation, and
  failure evidence.

Only confirmation-code salt/hash, expiry, bounded attempt count, actor UUID, Space UUID,
and proposal digest may be retained as confirmation metadata. The clear code is returned
once and is never stored. Confirmation additionally requires the current authenticated
message text, loaded server-side by its trusted receipt, to equal `确认 <8位确认码>`
exactly after channel mention stripping.

## Administrator Bootstrap

The actor must resolve from `(platform, channel_user_id, chat_id)` through
`channel_identities.person_id`. Existing Space administrators are active memberships at
`community_key = 'space:<space uuid>'` with role `space_admin` or `business_admin`. When
a Space has no such administrator, only an active global membership at
`community_key = 'qintopia'` with role `owner` or `admin` may confirm the first policy.
Display names never authorize.

## Privacy And Safety

- The three Space configuration tools do not accept Space, room, destination, or actor
  ids. Other existing tools remain registered, but ordinary QiWe group calls are
  governed by the current Space policy when enforcement is enabled.
- Ordinary-turn context and authorization operations require an adapter-derived session
  backed by the exact authenticated message receipt and never return Space, room,
  destination, or actor ids.
- Definition JSON rejects credentials, arbitrary network requests, executable code, SQL,
  and target-room fields at the service boundary.
- Event mapping output is restricted to canonical event facts.
- Unauthenticated raw events are persisted for compatibility but cannot run mappings;
  shadow evidence must join an authenticated raw row from the same Space and provider.
- Space-scoped status and idempotency queries include `space_id`.
- The migration seeds no active policy, business, automation, mapping, or external send.
- The migration seeds no enabled ordinary-turn capability.
- Existing raw payload retention rules remain unchanged; this migration does not add new
  raw data.

## Compatibility

The migration is idempotent and additive. Existing raw events and work items retain a
null `space_id`. Existing global `work_items.idempotency_key` uniqueness remains valid;
new conversational keys include the Space UUID. No Redis, workflow engine, scheduler
database, or administration UI is introduced.

## Acceptance

- Migration reruns without error and records `2026-08-14.001` in
  `qintopia_agent_os.schema_change_log`.
- Foreign keys reject unknown Spaces.
- Partial indexes reject two active versions in the same stream but permit active
  versions in different Spaces.
- Provider mappings can be shared while destination fields are rejected by the service
  validator.
- A request cannot be confirmed by another actor or Space, after expiry, or after the
  attempt ceiling.
- The first administrator bootstrap requires a global owner/admin membership; later
  changes require Space-scoped administrator membership.
- A missing default policy denies every ordinary-turn capability and returns an empty
  identity and knowledge scope.
- Two groups with different active policies cannot observe each other's identity,
  knowledge scope, or capability grants, even when their operator identifiers are the
  same.
- A Space grant alone cannot authorize an ordinary tool; the global capability row must
  remain enabled with the exact caller, work-item type, scope-binding, and invocation
  metadata.
- Applying a definition creates no external message or provider call.

## Rollback

Application rollback stops writes and tool exposure, then marks active definition rows
paused through a reviewed operation. Nullable Space links and immutable version rows
remain in place so audit and forward recovery are preserved.
