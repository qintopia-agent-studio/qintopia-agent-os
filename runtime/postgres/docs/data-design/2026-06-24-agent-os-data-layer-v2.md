# Agent OS Data Layer v2

Schema version: `2026-06-24.002`  
Migration: `migrations/202606240002_agent_os_data_layer.sql`  
Status: applied by migration when deployed  
Date: 2026-06-24

## Purpose

The first sidecar schema, `2026-06-18.001`, is a message-capture foundation. This
version completes the first Agent OS data layer around that foundation so
that 二花 and 文渊阁 can use durable, auditable context instead of directly reading raw
documents, Dify chunks, or member records.

This version adds:

- Conversation metadata for captured messages.
- Knowledge source, document, chunk, embedding, sync, and access audit tables.
- Person, channel identity, membership, evidence-backed member facts, interaction
  summaries, and safe reply profile snapshots.
- Cross-domain graph projection tables for later graph workers.
- Context request/result and tool invocation audit tables.
- Embedding model registry and dimension metadata.
- A durable schema change log for future data design traceability.

All changes are additive to existing message rows.

## Design Sources

This schema follows the Agent OS design documents:

- `qintopia-agent-os/docs/agent-os/domain-model.md`
- `qintopia-agent-os/docs/agent-os/erhua-member-identification-reply-flow.md`
- `qintopia-agent-os/docs/agent-os/wenyuange-knowledge-research-capability.md`
- `qintopia-agent-os/config/hermes/plugins/qintopia-tools/README.md`

## Schema Map

### `qintopia_messages`

Ownership: capture and immutable conversation facts.

Existing tables remain. This version adds:

- `conversations`: stable chat/conversation records.
- `messages.tenant_id`: tenant partition, default `qintopia`.
- `messages.conversation_id` and `conversation_key`: future conversation joins.
- `messages.sender_channel_identity_id` and `sender_person_id`: resolved identity links.
- `messages.visibility`: coarse operational visibility, default `internal`.
- `messages.information_class`: disclosure class, default `Internal`.
- `messages.content_hash`, `language`, `message_scope`, and `processing_hints`:
  retrieval and worker metadata.
- trigram index on `messages.text` for keyword/fuzzy retrieval.
- model/dimension/time indexes on `message_embeddings`.

The capture sidecar still writes only message facts and pending jobs. Identity linking,
embedding, summarization, and graph projection remain separate workers.

### `qintopia_knowledge`

Ownership: indexed knowledge from Feishu, Dify exports, markdown snapshots, SOPs, FAQs,
and other approved sources.

Tables:

- `knowledge_sources`: source metadata, sync cursor, policy, and classification.
- `knowledge_documents`: source document metadata, version key, URL, status, and hash.
- `knowledge_chunks`: chunked retrieval text with source locator and audience policy.
- `knowledge_embeddings`: pgvector embeddings for chunks.
- `knowledge_sync_jobs`: async source sync and indexing jobs.
- `knowledge_access_audit`: retrieval decisions, classes used, and redactions.

Agents should not read Feishu full text directly in normal operation. 文渊阁 should
query indexed chunks and only fetch small live fragments when a task is authorized and
freshness requires it.

### `qintopia_identity`

Ownership: person identity, channel identity, evidence-backed facts, interaction
summaries, and safe reply context.

Tables:

- `persons`: canonical human/person records.
- `person_aliases`: nicknames, aliases, and name evidence.
- `channel_identities`: platform ids, chat-scoped display names, and match confidence.
- `person_memberships`: community roles and status.
- `member_facts`: evidence-backed facts from messages, documents, activities, or manual
  review.
- `person_interaction_summaries`: time-bounded summaries derived from messages.
- `member_profile_snapshots`: safe, generated reply context and style hints.
- `member_context_audit`: audit log for member context reads.

二花 must not receive raw member dossiers or private facts. It should receive only safe,
minimal context from 文渊阁 or a controlled member-context tool.

### `qintopia_graph`

Ownership: graph projection layer built from messages, knowledge, identity, and business
records.

Tables:

- `graph_entities`: cross-domain canonical entities.
- `graph_entity_observations`: observations from source tables.
- `graph_edges`: evidence-backed relations between entities.
- `graph_projections`: worker watermarks for graph builds.

This schema is a projection target, not the system of record. Workers should be able to
rebuild it from source facts.

### `qintopia_agent_os`

Ownership: runtime audit, context delivery, embedding model metadata, and schema
history.

Tables:

- `schema_change_log`: durable record for data structure versions.
- `embedding_models`: embedding model/provider/dimension registry.
- `agent_context_requests`: request envelope for knowledge/member/message context
  lookup.
- `agent_context_results`: filtered answer basis and safe reply guidance.
- `tool_invocation_audit`: profile/tool/purpose audit for sensitive calls.

This layer separates raw data access from the minimal context front-line Agents are
allowed to use.

## Information Classes

The schema stores `information_class` as text so the taxonomy can evolve without
blocking migrations. Current expected values:

- `Public`
- `Internal`
- `Member-scoped`
- `Restricted`
- `Sensitive/Secret`

Default for captured QiWe messages is `Internal`. Workers may later upgrade or restrict
rows/chunks based on policy.

## Embedding Model Policy

`qintopia_agent_os.embedding_models` records provider, model key, vector dimensions,
distance metric, and status.

New embedding workers must:

- write `embedding_model` exactly as registered or intentionally versioned;
- write `embedding_dimension = vector_dims(embedding)`;
- filter vector search by both model and dimension;
- create pgvector ANN indexes only after enough rows exist for the specific
  model/dimension pair.

The previous `message_embeddings.embedding vector(1536)` assumption is generalized to
`vector` by this migration. This avoids locking the data layer to one embedding
provider.

## Retrieval Path

Preferred WenYuanGe retrieval sequence:

1. Apply caller, purpose, audience, source, time, and information-class filters.
2. Use structured filters and trigram/keyword retrieval for narrow candidates.
3. Use pgvector semantic recall when compatible embeddings exist.
4. Merge, rank, and deduplicate results.
5. Apply disclosure filtering and redaction.
6. Return minimal answer basis and safe reply guidance.
7. Write access audit.

Front-line Agents should not receive raw message-store, Feishu full-text, or unfiltered
member-profile tools. They should receive filtered WenYuanGe outputs.

## Worker Boundaries

The following workers are expected and must stay independent from the capture sidecar:

- `qintopia-message-embedding-worker`: consumes `embedding_pending`, writes
  `qintopia_messages.message_embeddings`.
- `qintopia-message-identity-worker`: resolves `sender_id` into `channel_identities` /
  `persons`.
- `qintopia-message-summary-worker`: produces interaction summaries from messages.
- `qintopia-knowledge-sync-worker`: syncs Feishu/Dify/markdown sources into
  `qintopia_knowledge`.
- `qintopia-knowledge-embedding-worker`: writes
  `qintopia_knowledge.knowledge_embeddings`.
- `qintopia-member-profile-worker`: produces `member_facts` and
  `member_profile_snapshots`.
- `qintopia-graph-projection-worker`: builds `qintopia_graph` projections.

None of these workers should block QiWe webhook ACKs or 二花 replies.

## Compatibility

This migration is safe to run on the current `qintopia` database:

- Adds new schemas and tables.
- Adds defaulted or nullable columns to existing message tables.
- Adds indexes.
- Backfills `message_embeddings.embedding_dimension` if rows exist.
- Generalizes `message_embeddings.embedding` from `vector(1536)` to `vector`.
- Does not delete, rename, or rewrite message rows.
- Does not change the sidecar write path.

The current sidecar binary can continue to run after this migration because its insert
statements use explicit message columns.

## Change Record

The migration records:

```sql
schema_version = '2026-06-18.001'
migration_name = '202606180001_init.sql'
design_doc_path = 'docs/data-design/2026-06-18-message-capture-v1.md'
```

and:

```sql
schema_version = '2026-06-24.002'
migration_name = '202606240002_agent_os_data_layer.sql'
design_doc_path = 'docs/data-design/2026-06-24-agent-os-data-layer-v2.md'
```

in `qintopia_agent_os.schema_change_log`.

Future migrations must add a new schema version and a matching design note.
