SET search_path TO qintopia_messages, public;

CREATE TABLE IF NOT EXISTS qintopia_messages.raw_events (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    event_id text NOT NULL,
    source text NOT NULL DEFAULT 'qiwe',
    subject text NOT NULL,
    received_at timestamptz NOT NULL,
    payload jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    last_seen_at timestamptz NOT NULL DEFAULT now(),
    duplicate_count integer NOT NULL DEFAULT 0,
    UNIQUE (source, event_id)
);

CREATE TABLE IF NOT EXISTS qintopia_messages.messages (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    platform text NOT NULL,
    message_id text NOT NULL,
    event_id text NOT NULL,
    chat_id text NOT NULL,
    chat_type text NOT NULL,
    sender_id text NOT NULL,
    sender_name text,
    message_kind text NOT NULL,
    text text,
    is_mention_bot boolean NOT NULL DEFAULT false,
    should_trigger boolean NOT NULL DEFAULT false,
    trigger_reason text,
    sent_at timestamptz,
    received_at timestamptz NOT NULL,
    raw_event_id uuid REFERENCES qintopia_messages.raw_events(id) ON DELETE SET NULL,
    raw jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    last_seen_at timestamptz NOT NULL DEFAULT now(),
    duplicate_count integer NOT NULL DEFAULT 0,
    UNIQUE (platform, message_id)
);

CREATE TABLE IF NOT EXISTS qintopia_messages.message_mentions (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    message_id uuid NOT NULL REFERENCES qintopia_messages.messages(id) ON DELETE CASCADE,
    mention_key text NOT NULL,
    platform_user_id text,
    display_name text,
    raw jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (message_id, mention_key)
);

CREATE TABLE IF NOT EXISTS qintopia_messages.message_embeddings (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    message_id uuid NOT NULL REFERENCES qintopia_messages.messages(id) ON DELETE CASCADE,
    embedding_model text NOT NULL,
    embedding vector(1536) NOT NULL,
    content_hash text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (message_id, embedding_model, content_hash)
);

CREATE TABLE IF NOT EXISTS qintopia_messages.message_processing_jobs (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    message_id uuid NOT NULL REFERENCES qintopia_messages.messages(id) ON DELETE CASCADE,
    job_type text NOT NULL,
    status text NOT NULL DEFAULT 'pending',
    attempts integer NOT NULL DEFAULT 0,
    available_at timestamptz NOT NULL DEFAULT now(),
    locked_at timestamptz,
    completed_at timestamptz,
    error text,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (message_id, job_type)
);

CREATE TABLE IF NOT EXISTS qintopia_messages.dead_letter_events (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    subject text NOT NULL,
    stream_sequence bigint,
    consumer text NOT NULL,
    error_kind text NOT NULL,
    error text NOT NULL,
    payload_text text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS qintopia_messages.entities (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    entity_type text NOT NULL,
    canonical_name text NOT NULL,
    aliases text[] NOT NULL DEFAULT ARRAY[]::text[],
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (entity_type, canonical_name)
);

CREATE TABLE IF NOT EXISTS qintopia_messages.message_entities (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    message_id uuid NOT NULL REFERENCES qintopia_messages.messages(id) ON DELETE CASCADE,
    entity_id uuid NOT NULL REFERENCES qintopia_messages.entities(id) ON DELETE CASCADE,
    role text,
    confidence double precision,
    raw jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (message_id, entity_id, role)
);

CREATE TABLE IF NOT EXISTS qintopia_messages.entity_edges (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    source_entity_id uuid NOT NULL REFERENCES qintopia_messages.entities(id) ON DELETE CASCADE,
    target_entity_id uuid NOT NULL REFERENCES qintopia_messages.entities(id) ON DELETE CASCADE,
    edge_type text NOT NULL,
    confidence double precision,
    evidence_message_id uuid REFERENCES qintopia_messages.messages(id) ON DELETE SET NULL,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (source_entity_id, target_entity_id, edge_type, evidence_message_id)
);

CREATE INDEX IF NOT EXISTS raw_events_received_at_idx
    ON qintopia_messages.raw_events (received_at DESC);

CREATE INDEX IF NOT EXISTS messages_chat_sent_idx
    ON qintopia_messages.messages (chat_id, sent_at DESC);

CREATE INDEX IF NOT EXISTS messages_sender_sent_idx
    ON qintopia_messages.messages (sender_id, sent_at DESC);

CREATE INDEX IF NOT EXISTS messages_raw_gin_idx
    ON qintopia_messages.messages USING gin (raw);

CREATE INDEX IF NOT EXISTS raw_events_payload_gin_idx
    ON qintopia_messages.raw_events USING gin (payload);

CREATE INDEX IF NOT EXISTS message_processing_jobs_status_idx
    ON qintopia_messages.message_processing_jobs (status, available_at);
