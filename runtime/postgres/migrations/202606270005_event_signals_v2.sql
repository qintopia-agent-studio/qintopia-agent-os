SET search_path TO qintopia_messages, public;

CREATE SCHEMA IF NOT EXISTS qintopia_agent_os;

CREATE TABLE IF NOT EXISTS qintopia_agent_os.event_signal_candidates (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    platform text NOT NULL DEFAULT 'qiwe',
    chat_id text NOT NULL,
    signal_date date NOT NULL,
    source_message_id uuid REFERENCES qintopia_messages.messages(id) ON DELETE SET NULL,
    sender_person_id uuid,
    sender_channel_identity_id uuid,
    sender_name text,
    message_received_at timestamptz NOT NULL,
    message_text text NOT NULL,
    candidate_labels text[] NOT NULL DEFAULT ARRAY[]::text[],
    candidate_score double precision NOT NULL DEFAULT 0.0,
    filter_reason text,
    extraction_version text NOT NULL,
    status text NOT NULL DEFAULT 'pending',
    judge_status text NOT NULL DEFAULT 'not_required',
    judge_model text,
    judge_reason text,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (platform, chat_id, signal_date, source_message_id, extraction_version)
);

CREATE INDEX IF NOT EXISTS event_signal_candidates_pending_idx
    ON qintopia_agent_os.event_signal_candidates (status, judge_status, signal_date DESC);

CREATE INDEX IF NOT EXISTS event_signal_candidates_chat_date_idx
    ON qintopia_agent_os.event_signal_candidates (platform, chat_id, signal_date DESC);

CREATE INDEX IF NOT EXISTS event_signal_candidates_message_idx
    ON qintopia_agent_os.event_signal_candidates (source_message_id)
    WHERE source_message_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS qintopia_agent_os.event_signals (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    platform text NOT NULL DEFAULT 'qiwe',
    chat_id text NOT NULL,
    signal_date date NOT NULL,
    signal_type text NOT NULL,
    title text NOT NULL,
    summary text NOT NULL,
    related_member_names text[] NOT NULL DEFAULT ARRAY[]::text[],
    owner_name text NOT NULL,
    owner_agent text NOT NULL,
    priority text NOT NULL DEFAULT '中',
    status text NOT NULL DEFAULT '待处理',
    confidence double precision NOT NULL DEFAULT 0.0,
    source_candidate_ids uuid[] NOT NULL DEFAULT ARRAY[]::uuid[],
    source_message_ids uuid[] NOT NULL DEFAULT ARRAY[]::uuid[],
    source_window_start timestamptz,
    source_window_end timestamptz,
    dedupe_key text NOT NULL,
    judge_model text NOT NULL DEFAULT 'rule_v2',
    judge_reason text NOT NULL DEFAULT '',
    extraction_version text NOT NULL,
    information_class text NOT NULL DEFAULT 'internal_ops',
    risk_level text NOT NULL DEFAULT '无',
    external_publish_status text NOT NULL DEFAULT '未评估',
    feishu_record_id text,
    last_published_at timestamptz,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (platform, chat_id, signal_date, dedupe_key, extraction_version)
);

CREATE INDEX IF NOT EXISTS event_signals_chat_date_idx
    ON qintopia_agent_os.event_signals (platform, chat_id, signal_date DESC);

CREATE INDEX IF NOT EXISTS event_signals_owner_status_idx
    ON qintopia_agent_os.event_signals (owner_agent, status, signal_date DESC);

CREATE INDEX IF NOT EXISTS event_signals_type_idx
    ON qintopia_agent_os.event_signals (signal_type, signal_date DESC);

CREATE INDEX IF NOT EXISTS event_signals_source_message_ids_idx
    ON qintopia_agent_os.event_signals USING gin (source_message_ids);

INSERT INTO qintopia_agent_os.schema_change_log
    (schema_version, migration_name, summary, design_doc_path, metadata)
VALUES
    (
        '2026-06-27.005',
        '202606270005_event_signals_v2.sql',
        'Adds V2 structured event signal candidates and accepted event signals as the Agent OS source of truth for Xiaoman daily community event radar.',
        'docs/data-design/2026-06-27-event-signals-v2.md',
        '{"change_type":"additive","domain":"event_signals","replaces_runtime_path":"markdown_to_signal_rows"}'::jsonb
    )
ON CONFLICT (schema_version) DO UPDATE SET
    migration_name = EXCLUDED.migration_name,
    status = 'applied',
    summary = EXCLUDED.summary,
    design_doc_path = EXCLUDED.design_doc_path,
    metadata = EXCLUDED.metadata,
    applied_at = now();
