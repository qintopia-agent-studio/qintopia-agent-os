# Data Design Changelog

This file is the repository-side history for database design changes. The database-side
history is `qintopia_agent_os.schema_change_log`.

## `2026-07-14.003`

Migration: `migrations/202607140003_qiwe_upload_attempt_lifecycle.sql` Design note:
`docs/data-design/2026-07-14-qiwe-upload-attempt-lifecycle.md`

Adds the pre-network `uploading` state and terminalizes stale or legacy unrecorded
claims when AgentOS cannot prove that QiWe did not accept an upload. Unknown upload
outcomes cannot be retried automatically.

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
