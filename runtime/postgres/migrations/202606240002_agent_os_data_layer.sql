SET search_path TO qintopia_messages, public;

CREATE SCHEMA IF NOT EXISTS qintopia_messages;
CREATE EXTENSION IF NOT EXISTS vector WITH SCHEMA qintopia_messages;
CREATE EXTENSION IF NOT EXISTS pgcrypto WITH SCHEMA qintopia_messages;
CREATE EXTENSION IF NOT EXISTS pg_trgm WITH SCHEMA qintopia_messages;

CREATE SCHEMA IF NOT EXISTS qintopia_knowledge;
CREATE SCHEMA IF NOT EXISTS qintopia_identity;
CREATE SCHEMA IF NOT EXISTS qintopia_graph;
CREATE SCHEMA IF NOT EXISTS qintopia_agent_os;

CREATE TABLE IF NOT EXISTS qintopia_agent_os.schema_change_log (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    schema_version text NOT NULL UNIQUE,
    migration_name text NOT NULL,
    status text NOT NULL DEFAULT 'applied',
    summary text NOT NULL,
    design_doc_path text,
    applied_at timestamptz NOT NULL DEFAULT now(),
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb
);

INSERT INTO qintopia_agent_os.schema_change_log
    (schema_version, migration_name, summary, design_doc_path, metadata)
VALUES
    (
        '2026-06-18.001',
        '202606180001_init.sql',
        'Initial message-capture schema for QiWe raw events, normalized messages, mentions, embedding slots, processing jobs, dead letters, and message-local graph placeholders.',
        'docs/data-design/2026-06-18-message-capture-v1.md',
        '{"change_type":"initial","recorded_by":"202606240002_agent_os_data_layer.sql"}'::jsonb
    )
ON CONFLICT (schema_version) DO UPDATE SET
    migration_name = EXCLUDED.migration_name,
    status = 'applied',
    summary = EXCLUDED.summary,
    design_doc_path = EXCLUDED.design_doc_path,
    metadata = EXCLUDED.metadata;

CREATE TABLE IF NOT EXISTS qintopia_agent_os.embedding_models (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    model_key text NOT NULL UNIQUE,
    provider text NOT NULL,
    vector_dimensions integer NOT NULL CHECK (vector_dimensions > 0),
    distance_metric text NOT NULL DEFAULT 'cosine',
    status text NOT NULL DEFAULT 'active',
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

INSERT INTO qintopia_agent_os.embedding_models
    (model_key, provider, vector_dimensions, distance_metric, metadata)
VALUES
    (
        'text-embedding-3-small',
        'openai-compatible',
        1536,
        'cosine',
        '{"default_for":"message_embeddings_v1"}'::jsonb
    )
ON CONFLICT (model_key) DO UPDATE SET
    provider = EXCLUDED.provider,
    vector_dimensions = EXCLUDED.vector_dimensions,
    distance_metric = EXCLUDED.distance_metric,
    metadata = qintopia_agent_os.embedding_models.metadata || EXCLUDED.metadata,
    updated_at = now();

CREATE TABLE IF NOT EXISTS qintopia_messages.conversations (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id text NOT NULL DEFAULT 'qintopia',
    platform text NOT NULL,
    chat_id text NOT NULL,
    chat_type text NOT NULL,
    display_name text,
    status text NOT NULL DEFAULT 'active',
    first_seen_at timestamptz NOT NULL DEFAULT now(),
    last_seen_at timestamptz NOT NULL DEFAULT now(),
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, platform, chat_id)
);

ALTER TABLE qintopia_messages.messages
    ADD COLUMN IF NOT EXISTS tenant_id text NOT NULL DEFAULT 'qintopia',
    ADD COLUMN IF NOT EXISTS conversation_id uuid REFERENCES qintopia_messages.conversations(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS conversation_key text,
    ADD COLUMN IF NOT EXISTS visibility text NOT NULL DEFAULT 'internal',
    ADD COLUMN IF NOT EXISTS information_class text NOT NULL DEFAULT 'Internal',
    ADD COLUMN IF NOT EXISTS content_hash text,
    ADD COLUMN IF NOT EXISTS language text,
    ADD COLUMN IF NOT EXISTS message_scope jsonb NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS processing_hints jsonb NOT NULL DEFAULT '{}'::jsonb;

ALTER TABLE qintopia_messages.message_embeddings
    ADD COLUMN IF NOT EXISTS embedding_dimension integer,
    ADD COLUMN IF NOT EXISTS metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS source_job_id uuid REFERENCES qintopia_messages.message_processing_jobs(id) ON DELETE SET NULL;

ALTER TABLE qintopia_messages.message_embeddings
    ALTER COLUMN embedding TYPE vector USING embedding::vector;

UPDATE qintopia_messages.message_embeddings
SET embedding_dimension = vector_dims(embedding)
WHERE embedding_dimension IS NULL;

ALTER TABLE qintopia_messages.message_embeddings
    ALTER COLUMN embedding_dimension SET NOT NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'message_embeddings_dimension_positive'
          AND conrelid = 'qintopia_messages.message_embeddings'::regclass
    ) THEN
        ALTER TABLE qintopia_messages.message_embeddings
            ADD CONSTRAINT message_embeddings_dimension_positive
            CHECK (embedding_dimension > 0);
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS conversations_platform_chat_idx
    ON qintopia_messages.conversations (platform, chat_id);

CREATE INDEX IF NOT EXISTS conversations_last_seen_idx
    ON qintopia_messages.conversations (tenant_id, last_seen_at DESC);

CREATE INDEX IF NOT EXISTS messages_tenant_received_idx
    ON qintopia_messages.messages (tenant_id, received_at DESC);

CREATE INDEX IF NOT EXISTS messages_conversation_sent_idx
    ON qintopia_messages.messages (conversation_id, sent_at DESC);

CREATE INDEX IF NOT EXISTS messages_information_class_idx
    ON qintopia_messages.messages (information_class, received_at DESC);

CREATE INDEX IF NOT EXISTS messages_visibility_idx
    ON qintopia_messages.messages (visibility, received_at DESC);

CREATE INDEX IF NOT EXISTS messages_content_hash_idx
    ON qintopia_messages.messages (content_hash)
    WHERE content_hash IS NOT NULL;

CREATE INDEX IF NOT EXISTS messages_text_trgm_idx
    ON qintopia_messages.messages USING gin (text gin_trgm_ops)
    WHERE text IS NOT NULL;

CREATE INDEX IF NOT EXISTS message_embeddings_model_idx
    ON qintopia_messages.message_embeddings (embedding_model, created_at DESC);

CREATE INDEX IF NOT EXISTS message_embeddings_dimension_idx
    ON qintopia_messages.message_embeddings (embedding_model, embedding_dimension);

CREATE TABLE IF NOT EXISTS qintopia_knowledge.knowledge_sources (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    source_type text NOT NULL,
    source_key text NOT NULL,
    title text NOT NULL,
    url text,
    owner text,
    information_class text NOT NULL DEFAULT 'Internal',
    visibility text NOT NULL DEFAULT 'internal',
    status text NOT NULL DEFAULT 'active',
    sync_status text NOT NULL DEFAULT 'pending',
    access_policy jsonb NOT NULL DEFAULT '{}'::jsonb,
    sync_cursor jsonb NOT NULL DEFAULT '{}'::jsonb,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    last_synced_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (source_type, source_key)
);

CREATE TABLE IF NOT EXISTS qintopia_knowledge.knowledge_documents (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    source_id uuid NOT NULL REFERENCES qintopia_knowledge.knowledge_sources(id) ON DELETE CASCADE,
    external_document_id text NOT NULL,
    parent_external_document_id text,
    title text NOT NULL,
    title_path text[] NOT NULL DEFAULT ARRAY[]::text[],
    document_type text NOT NULL,
    version_key text,
    canonical_url text,
    content_hash text,
    information_class text NOT NULL DEFAULT 'Internal',
    visibility text NOT NULL DEFAULT 'internal',
    status text NOT NULL DEFAULT 'active',
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    source_updated_at timestamptz,
    indexed_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (source_id, external_document_id)
);

CREATE TABLE IF NOT EXISTS qintopia_knowledge.knowledge_chunks (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    document_id uuid NOT NULL REFERENCES qintopia_knowledge.knowledge_documents(id) ON DELETE CASCADE,
    chunk_index integer NOT NULL,
    chunk_key text,
    chunk_kind text NOT NULL DEFAULT 'body',
    heading_path text[] NOT NULL DEFAULT ARRAY[]::text[],
    content text NOT NULL,
    content_hash text NOT NULL,
    token_count integer,
    information_class text NOT NULL DEFAULT 'Internal',
    visibility text NOT NULL DEFAULT 'internal',
    allowed_audiences text[] NOT NULL DEFAULT ARRAY[]::text[],
    source_locator jsonb NOT NULL DEFAULT '{}'::jsonb,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (document_id, chunk_index),
    UNIQUE (document_id, content_hash)
);

CREATE TABLE IF NOT EXISTS qintopia_knowledge.knowledge_embeddings (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    chunk_id uuid NOT NULL REFERENCES qintopia_knowledge.knowledge_chunks(id) ON DELETE CASCADE,
    embedding_model text NOT NULL,
    embedding_dimension integer NOT NULL CHECK (embedding_dimension > 0),
    embedding vector NOT NULL,
    content_hash text NOT NULL,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (chunk_id, embedding_model, content_hash)
);

CREATE TABLE IF NOT EXISTS qintopia_knowledge.knowledge_sync_jobs (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    source_id uuid REFERENCES qintopia_knowledge.knowledge_sources(id) ON DELETE CASCADE,
    document_id uuid REFERENCES qintopia_knowledge.knowledge_documents(id) ON DELETE CASCADE,
    job_type text NOT NULL,
    status text NOT NULL DEFAULT 'pending',
    attempts integer NOT NULL DEFAULT 0,
    available_at timestamptz NOT NULL DEFAULT now(),
    locked_at timestamptz,
    completed_at timestamptz,
    error text,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS qintopia_knowledge.knowledge_access_audit (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    caller_profile text NOT NULL,
    purpose text NOT NULL,
    query text,
    information_class_requested text,
    information_class_used text,
    source_ids uuid[] NOT NULL DEFAULT ARRAY[]::uuid[],
    document_ids uuid[] NOT NULL DEFAULT ARRAY[]::uuid[],
    chunk_ids uuid[] NOT NULL DEFAULT ARRAY[]::uuid[],
    decision text NOT NULL,
    redactions jsonb NOT NULL DEFAULT '{}'::jsonb,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS knowledge_sources_class_status_idx
    ON qintopia_knowledge.knowledge_sources (information_class, status);

CREATE INDEX IF NOT EXISTS knowledge_sources_sync_status_idx
    ON qintopia_knowledge.knowledge_sources (sync_status, last_synced_at DESC);

CREATE INDEX IF NOT EXISTS knowledge_documents_source_idx
    ON qintopia_knowledge.knowledge_documents (source_id, source_updated_at DESC);

CREATE INDEX IF NOT EXISTS knowledge_documents_status_idx
    ON qintopia_knowledge.knowledge_documents (status, indexed_at DESC);

CREATE INDEX IF NOT EXISTS knowledge_chunks_document_idx
    ON qintopia_knowledge.knowledge_chunks (document_id, chunk_index);

CREATE INDEX IF NOT EXISTS knowledge_chunks_class_idx
    ON qintopia_knowledge.knowledge_chunks (information_class, updated_at DESC);

CREATE INDEX IF NOT EXISTS knowledge_chunks_content_trgm_idx
    ON qintopia_knowledge.knowledge_chunks USING gin (content gin_trgm_ops);

CREATE INDEX IF NOT EXISTS knowledge_embeddings_model_idx
    ON qintopia_knowledge.knowledge_embeddings (embedding_model, created_at DESC);

CREATE INDEX IF NOT EXISTS knowledge_embeddings_dimension_idx
    ON qintopia_knowledge.knowledge_embeddings (embedding_model, embedding_dimension);

CREATE INDEX IF NOT EXISTS knowledge_sync_jobs_status_idx
    ON qintopia_knowledge.knowledge_sync_jobs (status, available_at);

CREATE INDEX IF NOT EXISTS knowledge_access_audit_created_idx
    ON qintopia_knowledge.knowledge_access_audit (created_at DESC);

CREATE TABLE IF NOT EXISTS qintopia_identity.persons (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    person_type text NOT NULL DEFAULT 'human',
    display_name text NOT NULL,
    primary_name text,
    preferred_name text,
    status text NOT NULL DEFAULT 'active',
    information_class text NOT NULL DEFAULT 'Internal',
    visibility text NOT NULL DEFAULT 'internal',
    profile_policy jsonb NOT NULL DEFAULT '{}'::jsonb,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS qintopia_identity.person_aliases (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    person_id uuid NOT NULL REFERENCES qintopia_identity.persons(id) ON DELETE CASCADE,
    alias text NOT NULL,
    alias_type text NOT NULL DEFAULT 'nickname',
    source text,
    confidence double precision,
    first_seen_at timestamptz NOT NULL DEFAULT now(),
    last_seen_at timestamptz NOT NULL DEFAULT now(),
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (person_id, alias, alias_type)
);

CREATE TABLE IF NOT EXISTS qintopia_identity.channel_identities (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    person_id uuid REFERENCES qintopia_identity.persons(id) ON DELETE SET NULL,
    platform text NOT NULL,
    channel_user_id text NOT NULL,
    chat_id text NOT NULL DEFAULT '',
    display_name text,
    normalized_display_name text,
    identity_source text NOT NULL DEFAULT 'observed',
    is_bot boolean NOT NULL DEFAULT false,
    confidence double precision,
    first_seen_at timestamptz NOT NULL DEFAULT now(),
    last_seen_at timestamptz NOT NULL DEFAULT now(),
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (platform, channel_user_id, chat_id)
);

CREATE TABLE IF NOT EXISTS qintopia_identity.person_memberships (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    person_id uuid NOT NULL REFERENCES qintopia_identity.persons(id) ON DELETE CASCADE,
    community_key text NOT NULL DEFAULT 'qintopia',
    role text NOT NULL DEFAULT 'member',
    status text NOT NULL DEFAULT 'active',
    display_label text,
    information_class text NOT NULL DEFAULT 'Internal',
    started_at timestamptz,
    ended_at timestamptz,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (person_id, community_key, role)
);

CREATE TABLE IF NOT EXISTS qintopia_identity.member_facts (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    person_id uuid REFERENCES qintopia_identity.persons(id) ON DELETE CASCADE,
    channel_identity_id uuid REFERENCES qintopia_identity.channel_identities(id) ON DELETE SET NULL,
    fact_type text NOT NULL,
    fact_key text,
    fact_text text NOT NULL,
    evidence_type text NOT NULL,
    evidence_ref_id uuid,
    evidence_ref_table text,
    source_message_id uuid REFERENCES qintopia_messages.messages(id) ON DELETE SET NULL,
    source_document_id uuid REFERENCES qintopia_knowledge.knowledge_documents(id) ON DELETE SET NULL,
    information_class text NOT NULL DEFAULT 'Internal',
    visibility text NOT NULL DEFAULT 'internal',
    confidence double precision,
    observed_at timestamptz NOT NULL DEFAULT now(),
    valid_from timestamptz,
    expires_at timestamptz,
    revoked_at timestamptz,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS qintopia_identity.person_interaction_summaries (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    person_id uuid REFERENCES qintopia_identity.persons(id) ON DELETE CASCADE,
    channel_identity_id uuid REFERENCES qintopia_identity.channel_identities(id) ON DELETE SET NULL,
    platform text,
    chat_id text,
    period_start timestamptz,
    period_end timestamptz,
    summary text NOT NULL,
    topics text[] NOT NULL DEFAULT ARRAY[]::text[],
    source_message_ids uuid[] NOT NULL DEFAULT ARRAY[]::uuid[],
    information_class text NOT NULL DEFAULT 'Internal',
    confidence double precision,
    generated_by text,
    generated_at timestamptz NOT NULL DEFAULT now(),
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS qintopia_identity.member_profile_snapshots (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    person_id uuid NOT NULL REFERENCES qintopia_identity.persons(id) ON DELETE CASCADE,
    profile_kind text NOT NULL DEFAULT 'reply_context',
    profile_version text NOT NULL,
    status text NOT NULL DEFAULT 'active',
    summary text NOT NULL,
    communication_style jsonb NOT NULL DEFAULT '{}'::jsonb,
    safe_reply_hints jsonb NOT NULL DEFAULT '{}'::jsonb,
    do_not_disclose jsonb NOT NULL DEFAULT '{}'::jsonb,
    source_fact_ids uuid[] NOT NULL DEFAULT ARRAY[]::uuid[],
    source_summary_ids uuid[] NOT NULL DEFAULT ARRAY[]::uuid[],
    information_class text NOT NULL DEFAULT 'Internal',
    confidence double precision,
    generated_by text,
    input_hash text,
    generated_at timestamptz NOT NULL DEFAULT now(),
    reviewed_at timestamptz,
    valid_until timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (person_id, profile_kind, profile_version, generated_at)
);

CREATE TABLE IF NOT EXISTS qintopia_identity.member_context_audit (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    caller_profile text NOT NULL,
    platform text NOT NULL,
    channel_user_id text NOT NULL,
    person_id uuid REFERENCES qintopia_identity.persons(id) ON DELETE SET NULL,
    chat_id text,
    purpose text NOT NULL,
    fields_returned jsonb NOT NULL DEFAULT '{}'::jsonb,
    redactions jsonb NOT NULL DEFAULT '{}'::jsonb,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now()
);

ALTER TABLE qintopia_messages.messages
    ADD COLUMN IF NOT EXISTS sender_channel_identity_id uuid REFERENCES qintopia_identity.channel_identities(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS sender_person_id uuid REFERENCES qintopia_identity.persons(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS persons_display_name_trgm_idx
    ON qintopia_identity.persons USING gin (display_name gin_trgm_ops);

CREATE INDEX IF NOT EXISTS persons_primary_name_idx
    ON qintopia_identity.persons (primary_name)
    WHERE primary_name IS NOT NULL;

CREATE INDEX IF NOT EXISTS person_aliases_alias_trgm_idx
    ON qintopia_identity.person_aliases USING gin (alias gin_trgm_ops);

CREATE INDEX IF NOT EXISTS channel_identities_person_idx
    ON qintopia_identity.channel_identities (person_id);

CREATE INDEX IF NOT EXISTS channel_identities_last_seen_idx
    ON qintopia_identity.channel_identities (platform, last_seen_at DESC);

CREATE INDEX IF NOT EXISTS person_memberships_person_idx
    ON qintopia_identity.person_memberships (person_id, status);

CREATE INDEX IF NOT EXISTS member_facts_person_observed_idx
    ON qintopia_identity.member_facts (person_id, observed_at DESC);

CREATE INDEX IF NOT EXISTS member_facts_type_idx
    ON qintopia_identity.member_facts (fact_type, observed_at DESC);

CREATE INDEX IF NOT EXISTS member_facts_source_message_idx
    ON qintopia_identity.member_facts (source_message_id)
    WHERE source_message_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS member_facts_text_trgm_idx
    ON qintopia_identity.member_facts USING gin (fact_text gin_trgm_ops);

CREATE INDEX IF NOT EXISTS person_interaction_summaries_person_idx
    ON qintopia_identity.person_interaction_summaries (person_id, generated_at DESC);

CREATE INDEX IF NOT EXISTS member_profile_snapshots_person_idx
    ON qintopia_identity.member_profile_snapshots (person_id, generated_at DESC);

CREATE INDEX IF NOT EXISTS member_context_audit_created_idx
    ON qintopia_identity.member_context_audit (created_at DESC);

CREATE INDEX IF NOT EXISTS messages_sender_identity_idx
    ON qintopia_messages.messages (sender_channel_identity_id, sent_at DESC)
    WHERE sender_channel_identity_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS messages_sender_person_idx
    ON qintopia_messages.messages (sender_person_id, sent_at DESC)
    WHERE sender_person_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS qintopia_graph.graph_entities (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    entity_type text NOT NULL,
    canonical_key text NOT NULL,
    display_name text NOT NULL,
    aliases text[] NOT NULL DEFAULT ARRAY[]::text[],
    information_class text NOT NULL DEFAULT 'Internal',
    status text NOT NULL DEFAULT 'active',
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (entity_type, canonical_key)
);

CREATE TABLE IF NOT EXISTS qintopia_graph.graph_entity_observations (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    entity_id uuid NOT NULL REFERENCES qintopia_graph.graph_entities(id) ON DELETE CASCADE,
    source_type text NOT NULL,
    source_table text,
    source_id uuid,
    observed_name text,
    confidence double precision,
    observed_at timestamptz NOT NULL DEFAULT now(),
    raw jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS qintopia_graph.graph_edges (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    source_entity_id uuid NOT NULL REFERENCES qintopia_graph.graph_entities(id) ON DELETE CASCADE,
    target_entity_id uuid NOT NULL REFERENCES qintopia_graph.graph_entities(id) ON DELETE CASCADE,
    edge_type text NOT NULL,
    predicate text,
    weight double precision,
    confidence double precision,
    evidence_type text,
    evidence_table text,
    evidence_id uuid,
    valid_from timestamptz,
    valid_until timestamptz,
    information_class text NOT NULL DEFAULT 'Internal',
    status text NOT NULL DEFAULT 'active',
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS qintopia_graph.graph_projections (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    projection_name text NOT NULL UNIQUE,
    status text NOT NULL DEFAULT 'pending',
    source_watermark jsonb NOT NULL DEFAULT '{}'::jsonb,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'graph_edges_unique_evidence'
          AND conrelid = 'qintopia_graph.graph_edges'::regclass
    ) THEN
        ALTER TABLE qintopia_graph.graph_edges
            ADD CONSTRAINT graph_edges_unique_evidence
            UNIQUE (source_entity_id, target_entity_id, edge_type, evidence_table, evidence_id);
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS graph_entities_display_name_trgm_idx
    ON qintopia_graph.graph_entities USING gin (display_name gin_trgm_ops);

CREATE INDEX IF NOT EXISTS graph_entity_observations_entity_idx
    ON qintopia_graph.graph_entity_observations (entity_id, observed_at DESC);

CREATE INDEX IF NOT EXISTS graph_edges_source_idx
    ON qintopia_graph.graph_edges (source_entity_id, edge_type);

CREATE INDEX IF NOT EXISTS graph_edges_target_idx
    ON qintopia_graph.graph_edges (target_entity_id, edge_type);

CREATE TABLE IF NOT EXISTS qintopia_agent_os.agent_context_requests (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    caller_profile text NOT NULL,
    target_profile text,
    request_type text NOT NULL,
    purpose text NOT NULL,
    input jsonb NOT NULL DEFAULT '{}'::jsonb,
    status text NOT NULL DEFAULT 'pending',
    created_at timestamptz NOT NULL DEFAULT now(),
    completed_at timestamptz,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb
);

CREATE TABLE IF NOT EXISTS qintopia_agent_os.agent_context_results (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    request_id uuid NOT NULL REFERENCES qintopia_agent_os.agent_context_requests(id) ON DELETE CASCADE,
    answer_basis jsonb NOT NULL DEFAULT '{}'::jsonb,
    sources jsonb NOT NULL DEFAULT '[]'::jsonb,
    information_class_used text,
    confidence text,
    redactions jsonb NOT NULL DEFAULT '{}'::jsonb,
    safe_reply_guidance jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS qintopia_agent_os.tool_invocation_audit (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    profile_id text NOT NULL,
    tool_name text NOT NULL,
    purpose text,
    input_summary jsonb NOT NULL DEFAULT '{}'::jsonb,
    output_summary jsonb NOT NULL DEFAULT '{}'::jsonb,
    risk_level text,
    created_at timestamptz NOT NULL DEFAULT now(),
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb
);

CREATE INDEX IF NOT EXISTS embedding_models_status_idx
    ON qintopia_agent_os.embedding_models (status, provider);

CREATE INDEX IF NOT EXISTS agent_context_requests_status_idx
    ON qintopia_agent_os.agent_context_requests (status, created_at DESC);

CREATE INDEX IF NOT EXISTS agent_context_requests_type_idx
    ON qintopia_agent_os.agent_context_requests (request_type, created_at DESC);

CREATE INDEX IF NOT EXISTS agent_context_results_request_idx
    ON qintopia_agent_os.agent_context_results (request_id, created_at DESC);

CREATE INDEX IF NOT EXISTS tool_invocation_audit_profile_idx
    ON qintopia_agent_os.tool_invocation_audit (profile_id, created_at DESC);

INSERT INTO qintopia_agent_os.schema_change_log
    (schema_version, migration_name, summary, design_doc_path, metadata)
VALUES
    (
        '2026-06-24.002',
        '202606240002_agent_os_data_layer.sql',
        'Add Agent OS data layer schemas for knowledge chunks, member identity/profile context, cross-domain graph projection, context delivery audit, embedding model registry, and message visibility/person-link fields.',
        'docs/data-design/2026-06-24-agent-os-data-layer-v2.md',
        '{"compatible_with":"202606180001_init.sql","change_type":"additive"}'::jsonb
    )
ON CONFLICT (schema_version) DO UPDATE SET
    migration_name = EXCLUDED.migration_name,
    status = 'applied',
    summary = EXCLUDED.summary,
    design_doc_path = EXCLUDED.design_doc_path,
    metadata = EXCLUDED.metadata,
    applied_at = now();
