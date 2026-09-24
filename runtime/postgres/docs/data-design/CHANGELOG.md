# Data Design Changelog

## 2026-09-24.004 — Application readback and dispatch

- Fence trusted local source reads and reuse welcome application projections.
- Keep identity fingerprints separate from content; preserve confirmation audit.
- Dispatch distinct Anan and Silaoshi work without financial authority or external sends.
- Design: [application readback](2026-09-24-application-event-intake.md).

## 2026-09-24.002 — Payment event ingress

- Persist immutable source/property/binding snapshots for inbox and checkpoints.
- Deduplicate events and sequences within the trusted source domain.
- Reject unsafe reconstruction of preexisting unscoped event rows.
- Design: [payment event ingress](2026-09-24-business-payment-event-ingress.md).

## 2026-09-24.001 — Business operation execution

- New exact operation grants, trusted host turn evidence and independent PMS action records on WorkItems.
- Extend role/duty selectable actions and grant constraints without assigning any existing person new authority.
- Durable event inbox and checkpoints; default capability remains disabled and synthetic only.
- Design: [business execution](2026-09-24-business-operation-execution.md).

## 2026-09-23.009 — Steward review delegation

- Add scoped, time-bounded resident review assignments sourced from existing designate and review authority.
- Recheck current occupancy, source authority and exact delegation at approval and send preparation.
- Preserve assignment history and idempotent receipts; no production enablement.
- Design: [steward review delegation](2026-09-23-steward-review-delegation.md).

## `2026-09-23.008`

Migration: `migrations/202609230008_person_rule_lifecycle.sql`.
Design note: `docs/data-design/2026-09-23-person-rule-lifecycle.md`.

Adds explicit lifecycle state and operation audit for scoped text agreements.
Managed agreements expire without resurrecting older versions; stop cancels all
remaining revisions while retaining history and invalidating stale queued writes.

## `2026-09-23.007`

Migration: `migrations/202609230007_person_stay_building_history_registration.sql`.
Design note: `docs/data-design/2026-09-23-ontology-audience.md`.

Registers migrations 006 and 007 in the schema change log and installs the building
history table, conservative backfill, capture function and trigger idempotently.
Migration 006 was already applied during local validation; this additive repair
preserves its original checksum and existing observations without rewriting the
applied migration record or enabling external effects.

## `2026-09-23.006`

Migration: `migrations/202609230006_person_stay_building_history.sql`.
Design note: `docs/data-design/2026-09-23-ontology-audience.md`.

Preserves actual occupancy observations for each building across moves within one
stay. Legacy history backfills only its recorded last building, with unknown
observation times left empty. Dynamic contact audiences reuse current verified PMS
projections and confirmed tenant-scoped Person links; historical observations alone
do not prove current membership or authorize sending.

## `2026-09-22.005`

Migration: `migrations/202609220005_foundation_turn_input.sql`.
Design note: `docs/data-design/2026-09-22-person-foundation-consumers.md`.

Adds original typed-command and semantic-hash columns to synthetic turn sources, so
acknowledgement-loss retries preserve their initial expected version. No real message
text or external effects are introduced.

## `2026-09-22.004`

Migration: `migrations/202609220004_welcome_local_executors.sql`.
Design note: `docs/data-design/2026-09-22-foundation-welcome.md`.

Extends the existing synthetic executor availability table to the registered Anan and
Huabaosi identities alongside Erhua. It neither creates an execution queue nor enables
a production profile, capability or channel.

## `2026-09-22.003`

Migration: `migrations/202609220003_foundation_welcome.sql`.
Design note: `docs/data-design/2026-09-22-foundation-welcome.md`.

Binds welcome targets to shared authorization scopes and immutable rule versions.
Direct delivery records effective authority without fabricating human approval;
reviewed delivery retains exact content and current reviewer authority. Existing
actions, artifacts and upload intents remain authoritative. Local artifact bytes and
synthetic provider effects support preview and unknown-outcome readback without real
uploads or sends.

## `2026-09-22.002`

Migration: `migrations/202609220002_person_memory.sql`.
Design note: `docs/data-design/2026-09-22-person-memory.md`.

Adds trusted Gateway identity bindings, sourced reply preferences and stop/replay
protection, and observed stay history derived from verified PMS projections. Person
identity and facts reuse the existing shared tables. Unknown identity and optional
memory do not create staff work or disclosure permission.

## `2026-09-22.001`

Migration: `migrations/202609220001_person_foundation_consumers.sql`.
Design note: `docs/data-design/2026-09-22-person-foundation-consumers.md`.

Adds scope/time mappings around existing Space knowledge versions, durable tool
receipts and governed WorkItem requests. Execution shares the current appointment and
grant authority under the tenant lock; future rules preserve the active version and
case overrides expire back to the current default. No production grants or scheduler
activation are included.

## `2026-09-18.001`

Migration: `migrations/202609180001_workbench_accounts.sql`.
Design note: `docs/data-design/2026-09-18-workbench-accounts.md`.

Adds Person-bound accounts, revocable server sessions and login rate limits.
No business permission is created by account provisioning; synthetic isolation remains.

## `2026-09-11.001`

Migration: `migrations/202609110001_organization_person_workbench.sql`.
Design note: `docs/data-design/2026-09-11-organization-person-workbench.md`.

Adds scoped organizational positions, local metadata ledgers referencing existing identities and conversations,
and connection-specific contact settings. Parent links and registration grant no authority. Retirement ends affected
connections; restoration never reopens previous grants. No production seed or activation.

## `2026-09-10.003`

Migration: `migrations/202609100003_collaboration_duties_permissions.sql` Design note:
`docs/data-design/2026-09-10-person-agent-collaboration-v1.md`

Adds reusable duties and role associations, complete connection history, and explicit
autonomous/confirmation/denied permission settings. Reviewer eligibility is evaluated
against current authority. Catalog edits never issue execution grants; legacy connections
require an explicit duty selection. Only synthetic fixture definitions are seeded.

## `2026-09-10.002`

Migration: `migrations/202609100002_collaboration_role_actions.sql` Design note:
`docs/data-design/2026-09-10-person-agent-collaboration-v1.md`

Adds explicit available duties to positions. New assignments must fit both the position
and the operator's authority. Only the named synthetic UI fixture receives initial
choices; no execution grants are created or changed.

## `2026-09-10.001`

Migration: `migrations/202609100001_person_agent_collaboration.sql` Design note:
`docs/data-design/2026-09-10-person-agent-collaboration-v1.md`

Adds tenant-scoped appointments, person/Agent collaborations, management envelopes,
versioned grants, group bindings and idempotent configuration commands. Reuses Person,
source identities, conversations and audit. No administrator seed or production
activation is included in the migration.

## `2026-09-09.003`

Migration: `migrations/202609090003_resident_welcome_conflict_quarantine.sql` Design note:
`docs/data-design/2026-09-09-resident-welcome-v1.md`

Keeps same-vector source conflicts quarantined across repeated readbacks.

## `2026-09-09.002`

Migration: `migrations/202609090002_resident_welcome_recovery.sql` Design note:
`docs/data-design/2026-09-09-resident-welcome-v1.md`

Adds durable scan generations, page checkpoints and resumable baseline projections.
Scan completion does not enable publishing or promote historical cases.

## `2026-09-09.001`

Migration: `migrations/202609090001_resident_welcome_v1.sql` Design note:
`docs/data-design/2026-09-09-resident-welcome-v1.md`

Adds scoped source identity links, application/stay cases, durable Inbox/checkpoints,
grants, approvals and delivery/upload intents. Reuses Person, WorkItem and Artifact;
no production admission, permission grants or external execution is enabled.

## `2026-08-15.001`

Migration: `migrations/202608150001_space_execution_runner_contract.sql` Design note:
`docs/data-design/2026-08-15-space-execution-runner-contract.md`

Registers declarative deterministic execution recipes and the default-disabled,
authenticated broker contract for business-owned `agent_turn` output. It adds no table,
does not enable a capability or automation, and accepts no caller-provided Space,
destination, URL, or executable input.

## `2026-08-14.001`

Migration: `migrations/202608140001_erhua_conversational_self_extension.sql` Design
note: `docs/data-design/2026-08-14-erhua-conversational-self-extension.md`

Adds Space links and four versioned definition tables for Erhua conversational
self-extension. It reuses AgentOS work items, artifacts, and audit events for approval,
adds a default-disabled fixed catalog for isolated ordinary QiWe group turns, starts
with no active automation, and does not send externally.

## `2026-08-08.001`

Migration: `migrations/202608080001_xiaoman_daily_case_report_auto_publish.sql` Design note:
`docs/data-design/2026-08-08-xiaoman-daily-case-report-auto-publish.md`

Registers the Xiaoman daily case-report auto-publish capability. The apply path binds a
durable reviewed JPEG identity to one automatic QiWe image-send request while rejecting
local image paths, committed target group ids, and direct QiWe calls from the renderer.

## `2026-08-02.001`

Migration: `migrations/202608020001_xiaoman_material_followup_capability.sql` Design note:
`docs/data-design/2026-08-02-xiaoman-material-followup-capability.md`

Registers an internal Xiaoman material follow-up capability. The worker can create
idempotent post-event reminder and escalation work items without creating
`erhua.send_group_message`, sending externally, or treating the reminder as approved
public copy.

## 2026-08-01 - Xiaoman conversation poster participants V3

- Activates persisted conversation policy for unified direct/internal-group poster
  intake.
- Snapshots requester and reviewer authority and makes the source image the unique
  first-revision boundary.
- Adds a generic trusted-conversation notification capability without adding a public
  send path.
- Design: `2026-08-01-xiaoman-conversation-poster-participants-v3.md`

## 2026-07-31

- Added trusted Xiaoman Feishu poster intake correlation, durable direct-conversation
  notification state, and idempotent review callback records. Explicit direct-chat
  generation authorizes the source-grounded brief but never authorizes group send.

This file is the repository-side history for database design changes. The database-side
history is `qintopia_agent_os.schema_change_log`.

## `2026-07-14.003`

Migration: `migrations/202607140003_qiwe_upload_attempt_lifecycle.sql` Design note:
`docs/data-design/2026-07-14-qiwe-upload-attempt-lifecycle.md`

Adds the pre-network `uploading` state and terminalizes stale or legacy unrecorded
claims when AgentOS cannot prove that QiWe did not accept an upload. Unknown upload
outcomes cannot be retried automatically.

## `2026-07-15.002`

Migration: `migrations/202607150002_xiaoman_promotion_details.sql` Design note:
`docs/data-design/2026-07-15-xiaoman-promotion-details.md`

Adds the atomic Xiaoman promotion-details mutation using existing event-signal owner
and metadata fields. It does not add a business table, write Feishu, call a provider,
generate an image, or send a message.

## `2026-07-14.002`

Migration: `migrations/202607140002_qiwe_image_send_state.sql` Design note:
`docs/data-design/2026-07-14-qiwe-image-send-state.md`

Adds durable hashed QiWe image-upload correlation, callback idempotency, claim-token
validation, and sanitized terminal send audit. The migration is additive and does not
persist callback file credentials, enable the adapter, or send externally.

## `2026-07-14.001`

Migration: `migrations/202607140001_xiaoman_event_signal_mutations.sql` Design note:
`docs/data-design/2026-07-14-xiaoman-event-signal-mutations.md`

Adds a dedicated Xiaoman event-signal gap field and append-only mutation audit so
status/gap updates write AgentOS facts with explicit idempotency instead of treating
Feishu activity rows as the source of truth.

## `2026-07-13.002`

Migration: `migrations/202607130002_huabaosi_image_generation.sql` Design note:
`docs/data-design/2026-07-13-huabaosi-image-generation.md`

Registers the guarded `huabaosi.generate_image_asset` capability. The provider remains
disabled by default, generated images remain pending review, and the migration does not
enable external generation, media upload, publishing, or sending.

## `2026-06-30.007`

Migration: `migrations/202606300007_operations_control_plane.sql` Design note:
`docs/data-design/2026-06-30-operations-control-plane.md`

Adds the AgentOS operations control plane: capability-governed work items, artifacts,
append-only work item events, and human workbench references. This is the foundation for
multi-Agent operations workflows without using Hermes Kanban as the future orchestration
or task source.

## `2026-06-29.006`

Migration: `migrations/202606290006_erhua_training_memory.sql` Design note:
`docs/data-design/2026-06-29-erhua-training-memory.md`

Adds controlled Erhua trainer memory tables for audited trainer submissions and reviewed
persona overlays. Dynamic training memory is stored in Postgres and read back through
filtered context MCP responses rather than Hermes generic memory.

## `2026-06-27.005`

Migration: `migrations/202606270005_event_signals_v2.sql`  
Design note: `docs/data-design/2026-06-27-event-signals-v2.md`

Adds structured event signal candidate and accepted event tables. V2 makes
`qintopia_agent_os.event_signals` the source of truth for 小满每日社区事件雷达,
replacing the V1 runtime behavior that parsed daily-digest Markdown bullets back into
Feishu event rows.

## `2026-06-26.004`

Migration: `migrations/202606260004_profile_digest_archive_v1.sql`  
Design note: `docs/data-design/2026-06-26-profile-digest-archive-v1.md`

Adds the V1 daily digest outbox and raw message archive manifest index used by
member-profile, SQL graph, operations digest, and retention workflows. The daily digest
row is owned by 小满 and remains pending for a Feishu publisher; archived raw messages
are marked in `processing_hints` and hidden from default message search.

## `2026-06-26.003`

Migration: `migrations/202606260003_identity_observations.sql`  
Design note: `docs/data-design/2026-06-26-identity-observations-v3.md`

Adds `qintopia_identity.channel_identity_observations` so QiWe sender display names
resolved from group member/contact APIs can be audited and used to backfill captured
message rows without treating nickname text as a stable identity key.

## `2026-06-24.002`

Migration: `migrations/202606240002_agent_os_data_layer.sql`  
Design note: `docs/data-design/2026-06-24-agent-os-data-layer-v2.md`

Adds the first complete Agent OS data layer:

- message conversation metadata and identity links;
- knowledge source/document/chunk/embedding/sync/audit tables;
- person, alias, channel identity, membership, fact, interaction summary, and safe reply
  profile tables;
- graph projection tables;
- context request/result and tool invocation audit tables;
- embedding model registry and embedding dimension metadata;
- durable schema change log.

This is additive to captured message rows and keeps the capture sidecar independent from
embedding, identity, graph, and knowledge workers.

## `2026-06-18.001`

Migration: `migrations/202606180001_init.sql`  
Design note: `docs/data-design/2026-06-18-message-capture-v1.md`

Initial QiWe/Hermes message-capture schema:

- raw webhook events;
- normalized messages;
- mentions;
- message embedding slots;
- pending processing jobs;
- dead-letter payloads;
- early message-local graph placeholders.
