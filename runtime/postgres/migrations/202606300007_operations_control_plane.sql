CREATE SCHEMA IF NOT EXISTS qintopia_agent_os;

CREATE TABLE IF NOT EXISTS qintopia_agent_os.capabilities (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    capability_key text NOT NULL UNIQUE,
    provider_agent text NOT NULL,
    display_name text NOT NULL,
    description text NOT NULL DEFAULT '',
    allowed_callers text[] NOT NULL DEFAULT ARRAY[]::text[],
    allowed_work_item_types text[] NOT NULL DEFAULT ARRAY[]::text[],
    risk_level text NOT NULL DEFAULT 'medium',
    review_policy text NOT NULL DEFAULT 'before_external_use',
    input_schema jsonb NOT NULL DEFAULT '{}'::jsonb,
    output_schema jsonb NOT NULL DEFAULT '{}'::jsonb,
    enabled boolean NOT NULL DEFAULT true,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT capabilities_risk_level_check CHECK (
        risk_level IN ('low', 'medium', 'high')
    )
);

CREATE INDEX IF NOT EXISTS capabilities_provider_idx
    ON qintopia_agent_os.capabilities (provider_agent, enabled);

CREATE TABLE IF NOT EXISTS qintopia_agent_os.work_items (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    parent_work_item_id uuid REFERENCES qintopia_agent_os.work_items(id) ON DELETE SET NULL,
    work_item_type text NOT NULL,
    status text NOT NULL DEFAULT 'queued',
    requester_agent text NOT NULL,
    target_agent text NOT NULL,
    capability_key text NOT NULL REFERENCES qintopia_agent_os.capabilities(capability_key),
    human_owner text NOT NULL DEFAULT '',
    priority text NOT NULL DEFAULT 'normal',
    available_at timestamptz NOT NULL DEFAULT now(),
    claimed_by text,
    locked_at timestamptz,
    claim_expires_at timestamptz,
    attempts integer NOT NULL DEFAULT 0,
    last_error text,
    brief_summary text NOT NULL,
    purpose text NOT NULL DEFAULT '',
    source_event_signal_id uuid REFERENCES qintopia_agent_os.event_signals(id) ON DELETE SET NULL,
    source_type text NOT NULL DEFAULT '',
    source_refs jsonb NOT NULL DEFAULT '{}'::jsonb,
    dedupe_key text NOT NULL,
    idempotency_key text NOT NULL UNIQUE,
    risk_level text NOT NULL DEFAULT 'medium',
    information_class text NOT NULL DEFAULT 'internal_ops',
    payload jsonb NOT NULL DEFAULT '{}'::jsonb,
    payload_redaction_policy text NOT NULL DEFAULT 'summary_only',
    review_policy text NOT NULL DEFAULT 'before_external_use',
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT work_items_status_check CHECK (
        status IN (
            'queued',
            'processing',
            'awaiting_review',
            'awaiting_publish',
            'completed',
            'cancelled',
            'failed'
        )
    ),
    CONSTRAINT work_items_priority_check CHECK (
        priority IN ('low', 'normal', 'high', 'urgent')
    ),
    CONSTRAINT work_items_risk_level_check CHECK (
        risk_level IN ('low', 'medium', 'high')
    )
);

CREATE INDEX IF NOT EXISTS work_items_claimable_idx
    ON qintopia_agent_os.work_items (status, available_at, priority, created_at)
    WHERE status = 'queued';

CREATE INDEX IF NOT EXISTS work_items_requester_idx
    ON qintopia_agent_os.work_items (requester_agent, status, created_at DESC);

CREATE INDEX IF NOT EXISTS work_items_target_idx
    ON qintopia_agent_os.work_items (target_agent, status, created_at DESC);

CREATE INDEX IF NOT EXISTS work_items_capability_idx
    ON qintopia_agent_os.work_items (capability_key, status, created_at DESC);

CREATE INDEX IF NOT EXISTS work_items_source_event_signal_idx
    ON qintopia_agent_os.work_items (source_event_signal_id)
    WHERE source_event_signal_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS qintopia_agent_os.artifacts (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    work_item_id uuid NOT NULL REFERENCES qintopia_agent_os.work_items(id) ON DELETE CASCADE,
    artifact_type text NOT NULL,
    review_status text NOT NULL DEFAULT 'pending',
    created_by_agent text NOT NULL,
    title text NOT NULL DEFAULT '',
    summary text NOT NULL DEFAULT '',
    content_text text,
    artifact_uri text,
    content_hash text,
    source_ids jsonb NOT NULL DEFAULT '[]'::jsonb,
    risk_labels text[] NOT NULL DEFAULT ARRAY[]::text[],
    information_class text NOT NULL DEFAULT 'internal_ops',
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    review_requested_at timestamptz,
    reviewed_at timestamptz,
    reviewed_by text,
    review_decision_reason text,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT artifacts_review_status_check CHECK (
        review_status IN (
            'not_required',
            'pending',
            'approved',
            'rejected',
            'changes_requested'
        )
    )
);

CREATE INDEX IF NOT EXISTS artifacts_work_item_idx
    ON qintopia_agent_os.artifacts (work_item_id, created_at DESC);

CREATE INDEX IF NOT EXISTS artifacts_review_idx
    ON qintopia_agent_os.artifacts (review_status, created_at DESC);

CREATE UNIQUE INDEX IF NOT EXISTS artifacts_work_item_content_hash_idx
    ON qintopia_agent_os.artifacts (work_item_id, content_hash)
    WHERE content_hash IS NOT NULL AND content_hash <> '';

CREATE TABLE IF NOT EXISTS qintopia_agent_os.work_item_events (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    work_item_id uuid REFERENCES qintopia_agent_os.work_items(id) ON DELETE CASCADE,
    artifact_id uuid REFERENCES qintopia_agent_os.artifacts(id) ON DELETE SET NULL,
    event_type text NOT NULL,
    actor_type text NOT NULL DEFAULT 'system',
    actor_id text NOT NULL DEFAULT '',
    message text NOT NULL DEFAULT '',
    data jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS work_item_events_work_item_idx
    ON qintopia_agent_os.work_item_events (work_item_id, created_at DESC);

CREATE INDEX IF NOT EXISTS work_item_events_type_idx
    ON qintopia_agent_os.work_item_events (event_type, created_at DESC);

CREATE TABLE IF NOT EXISTS qintopia_agent_os.human_workbench_refs (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    work_item_id uuid REFERENCES qintopia_agent_os.work_items(id) ON DELETE CASCADE,
    artifact_id uuid REFERENCES qintopia_agent_os.artifacts(id) ON DELETE CASCADE,
    provider text NOT NULL,
    external_id text NOT NULL,
    external_url text NOT NULL DEFAULT '',
    display_title text NOT NULL DEFAULT '',
    status text NOT NULL DEFAULT 'active',
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    last_synced_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT human_workbench_refs_status_check CHECK (
        status IN ('active', 'archived', 'sync_failed')
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS human_workbench_refs_provider_external_idx
    ON qintopia_agent_os.human_workbench_refs (provider, external_id);

CREATE INDEX IF NOT EXISTS human_workbench_refs_work_item_idx
    ON qintopia_agent_os.human_workbench_refs (work_item_id, provider);

INSERT INTO qintopia_agent_os.capabilities
    (
        capability_key,
        provider_agent,
        display_name,
        description,
        allowed_callers,
        allowed_work_item_types,
        risk_level,
        review_policy,
        input_schema,
        output_schema,
        metadata
    )
VALUES
    (
        'huabaosi.create_visual_asset',
        'huabaosi',
        '画报司生成视觉素材',
        'Generate internal visual asset drafts such as poster briefs, visual prompts, and caption drafts from redacted operations context.',
        ARRAY['xiaoman', 'silaoshi', 'default']::text[],
        ARRAY['visual_asset_request', 'activity_promotion_request']::text[],
        'medium',
        'before_external_use',
        '{"required":["brief_summary"],"properties":{"brief_summary":{"type":"string"},"source_refs":{"type":"object"}}}'::jsonb,
        '{"artifact_types":["poster_brief","visual_prompt","caption_draft"]}'::jsonb,
        '{"default_information_class":"internal_ops"}'::jsonb
    ),
    (
        'erhua.send_group_message',
        'erhua',
        '二花受控群发消息',
        'Send approved operations copy and attachments to allowlisted community groups through the Erhua channel boundary.',
        ARRAY['xiaoman', 'silaoshi', 'default']::text[],
        ARRAY['group_message_request']::text[],
        'high',
        'human_final_confirmation',
        '{"required":["approved_artifact_id","target_channel","target_group_alias","message_text"],"properties":{"approved_artifact_id":{"type":"string"},"target_channel":{"type":"string"},"target_group_alias":{"type":"string"},"message_text":{"type":"string"},"attachments":{"type":"array"},"send_window":{"type":"string"}}}'::jsonb,
        '{"events":["send_requested","send_confirmed","send_failed"]}'::jsonb,
        '{"requires_approved_artifact":true,"external_send":true}'::jsonb
    ),
    (
        'wenyuange.retrieve_evidence',
        'wenyuange',
        '文渊阁检索运营证据',
        'Retrieve source-grounded internal or public evidence for operations planning without mutating business records.',
        ARRAY['xiaoman', 'huabaosi', 'silaoshi', 'default']::text[],
        ARRAY['evidence_request', 'activity_promotion_request']::text[],
        'medium',
        'not_required',
        '{"required":["question"],"properties":{"question":{"type":"string"},"source_refs":{"type":"object"}}}'::jsonb,
        '{"artifact_types":["evidence_summary","source_brief"]}'::jsonb,
        '{"read_only":true}'::jsonb
    ),
    (
        'xiaoman.create_activity_request',
        'xiaoman',
        '小满创建活动运营请求',
        'Create structured activity operations requests from event signals or explicit human instructions.',
        ARRAY['default', 'silaoshi']::text[],
        ARRAY['activity_promotion_request']::text[],
        'medium',
        'before_external_use',
        '{"required":["brief_summary"],"properties":{"brief_summary":{"type":"string"},"source_refs":{"type":"object"}}}'::jsonb,
        '{"work_item_types":["activity_promotion_request"]}'::jsonb,
        '{"source_agent":"xiaoman"}'::jsonb
    )
ON CONFLICT (capability_key) DO UPDATE SET
    provider_agent = EXCLUDED.provider_agent,
    display_name = EXCLUDED.display_name,
    description = EXCLUDED.description,
    allowed_callers = EXCLUDED.allowed_callers,
    allowed_work_item_types = EXCLUDED.allowed_work_item_types,
    risk_level = EXCLUDED.risk_level,
    review_policy = EXCLUDED.review_policy,
    input_schema = EXCLUDED.input_schema,
    output_schema = EXCLUDED.output_schema,
    enabled = true,
    metadata = EXCLUDED.metadata,
    updated_at = now();

INSERT INTO qintopia_agent_os.schema_change_log
    (schema_version, migration_name, summary, design_doc_path, metadata)
VALUES
    (
        '2026-06-30.007',
        '202606300007_operations_control_plane.sql',
        'Adds the AgentOS operations control plane for capability-governed multi-agent work items, artifacts, audit events, and human workbench references.',
        'docs/data-design/2026-06-30-operations-control-plane.md',
        '{"change_type":"additive","domain":"operations_control_plane","replaces_runtime_path":"hermes_kanban_for_new_operations"}'::jsonb
    )
ON CONFLICT (schema_version) DO UPDATE SET
    migration_name = EXCLUDED.migration_name,
    status = 'applied',
    summary = EXCLUDED.summary,
    design_doc_path = EXCLUDED.design_doc_path,
    metadata = EXCLUDED.metadata,
    applied_at = now();
