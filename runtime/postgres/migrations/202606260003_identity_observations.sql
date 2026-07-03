SET search_path TO qintopia_messages, public;

CREATE SCHEMA IF NOT EXISTS qintopia_identity;
CREATE SCHEMA IF NOT EXISTS qintopia_agent_os;

CREATE TABLE IF NOT EXISTS qintopia_identity.channel_identity_observations (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    channel_identity_id uuid NOT NULL REFERENCES qintopia_identity.channel_identities(id) ON DELETE CASCADE,
    platform text NOT NULL,
    chat_id text NOT NULL,
    channel_user_id text NOT NULL,
    observed_display_name text NOT NULL,
    normalized_display_name text,
    observation_source text NOT NULL,
    source_message_id uuid REFERENCES qintopia_messages.messages(id) ON DELETE SET NULL,
    source_event_id text,
    observed_at timestamptz NOT NULL DEFAULT now(),
    confidence double precision NOT NULL DEFAULT 1.0,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE OR REPLACE FUNCTION qintopia_identity.identity_source_rank(source text)
RETURNS integer
LANGUAGE sql
IMMUTABLE
AS $$
    SELECT CASE source
        WHEN 'room_member' THEN 50
        WHEN 'contact' THEN 40
        WHEN 'webhook' THEN 30
        WHEN 'current_backfill' THEN 20
        WHEN 'fallback_sender_id' THEN 10
        ELSE 0
    END
$$;

CREATE INDEX IF NOT EXISTS channel_identity_observations_identity_idx
    ON qintopia_identity.channel_identity_observations (channel_identity_id, observed_at DESC);

CREATE INDEX IF NOT EXISTS channel_identity_observations_user_chat_idx
    ON qintopia_identity.channel_identity_observations (platform, chat_id, channel_user_id, observed_at DESC);

INSERT INTO qintopia_agent_os.schema_change_log
    (schema_version, migration_name, summary, design_doc_path, metadata)
VALUES
    (
        '2026-06-26.003',
        '202606260003_identity_observations.sql',
        'Adds channel identity observations for audited QiWe sender display-name resolution and historical backfill.',
        'docs/data-design/2026-06-26-identity-observations-v3.md',
        '{"change_type":"additive","domain":"identity"}'::jsonb
    )
ON CONFLICT (schema_version) DO UPDATE SET
    migration_name = EXCLUDED.migration_name,
    status = 'applied',
    summary = EXCLUDED.summary,
    design_doc_path = EXCLUDED.design_doc_path,
    metadata = EXCLUDED.metadata;
