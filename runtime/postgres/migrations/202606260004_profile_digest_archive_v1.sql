SET search_path TO qintopia_messages, public;

CREATE SCHEMA IF NOT EXISTS qintopia_agent_os;

CREATE TABLE IF NOT EXISTS qintopia_agent_os.daily_digests (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_agent text NOT NULL,
    platform text NOT NULL DEFAULT 'qiwe',
    chat_id text NOT NULL,
    digest_date date NOT NULL,
    schedule_time text NOT NULL DEFAULT '03:00',
    timezone text NOT NULL DEFAULT 'Asia/Shanghai',
    title text NOT NULL,
    markdown text NOT NULL,
    feishu_parent_node text,
    feishu_document_token text,
    feishu_document_url text,
    publish_status text NOT NULL DEFAULT 'pending_feishu_publish',
    publish_error text,
    publish_attempts bigint NOT NULL DEFAULT 0,
    published_at timestamptz,
    message_count bigint NOT NULL DEFAULT 0,
    useful_signal_count bigint NOT NULL DEFAULT 0,
    generated_by text NOT NULL,
    generated_at timestamptz NOT NULL DEFAULT now(),
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (platform, chat_id, digest_date, owner_agent)
);

CREATE INDEX IF NOT EXISTS daily_digests_owner_status_idx
    ON qintopia_agent_os.daily_digests (owner_agent, publish_status, digest_date DESC);

CREATE INDEX IF NOT EXISTS daily_digests_chat_date_idx
    ON qintopia_agent_os.daily_digests (platform, chat_id, digest_date DESC);

ALTER TABLE qintopia_agent_os.daily_digests
    ADD COLUMN IF NOT EXISTS publish_error text,
    ADD COLUMN IF NOT EXISTS publish_attempts bigint NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS published_at timestamptz;

CREATE TABLE IF NOT EXISTS qintopia_agent_os.daily_digest_publish_audit (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    digest_id uuid NOT NULL REFERENCES qintopia_agent_os.daily_digests(id) ON DELETE CASCADE,
    actor_agent text NOT NULL,
    tool_name text NOT NULL DEFAULT 'qintopia_daily_digest_publish',
    action text NOT NULL,
    status text NOT NULL,
    feishu_parent_node text,
    feishu_document_token text,
    feishu_document_url text,
    error text,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS daily_digest_publish_audit_digest_idx
    ON qintopia_agent_os.daily_digest_publish_audit (digest_id, created_at DESC);

CREATE TABLE IF NOT EXISTS qintopia_agent_os.raw_message_archives (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    platform text NOT NULL DEFAULT 'qiwe',
    chat_ids text[] NOT NULL DEFAULT ARRAY[]::text[],
    cutoff_at timestamptz NOT NULL,
    first_received_at timestamptz,
    last_received_at timestamptz,
    message_count bigint NOT NULL DEFAULT 0,
    archive_format text NOT NULL DEFAULT 'jsonl.zst',
    archive_path text NOT NULL,
    manifest_path text NOT NULL,
    content_sha256 text NOT NULL,
    policy jsonb NOT NULL DEFAULT '{}'::jsonb,
    status text NOT NULL DEFAULT 'completed',
    created_by text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb
);

CREATE INDEX IF NOT EXISTS raw_message_archives_created_idx
    ON qintopia_agent_os.raw_message_archives (created_at DESC);

CREATE INDEX IF NOT EXISTS raw_message_archives_chat_ids_idx
    ON qintopia_agent_os.raw_message_archives USING gin (chat_ids);

INSERT INTO qintopia_agent_os.schema_change_log
    (schema_version, migration_name, summary, design_doc_path, metadata)
VALUES
    (
        '2026-06-26.004',
        '202606260004_profile_digest_archive_v1.sql',
        'Adds V1 daily digest outbox and raw message archive manifest indexes for member profile, graph, operations digest, and retention workflows.',
        'docs/data-design/2026-06-26-profile-digest-archive-v1.md',
        '{"change_type":"additive","domain":"profile_digest_archive"}'::jsonb
    )
ON CONFLICT (schema_version) DO UPDATE SET
    migration_name = EXCLUDED.migration_name,
    status = 'applied',
    summary = EXCLUDED.summary,
    design_doc_path = EXCLUDED.design_doc_path,
    metadata = EXCLUDED.metadata;
