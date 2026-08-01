BEGIN;

CREATE TABLE IF NOT EXISTS qintopia_agent_os.poster_return_targets (
    origin_ref text PRIMARY KEY,
    platform text NOT NULL,
    conversation_type text NOT NULL,
    conversation_id text NOT NULL,
    requester_user_id text NOT NULL,
    source_message_id text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT poster_return_targets_platform_check CHECK (platform = 'feishu'),
    CONSTRAINT poster_return_targets_conversation_type_check CHECK (conversation_type = 'direct')
);

REVOKE ALL ON qintopia_agent_os.poster_return_targets FROM PUBLIC;

CREATE TABLE IF NOT EXISTS qintopia_agent_os.poster_notifications (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    work_item_id uuid NOT NULL UNIQUE
        REFERENCES qintopia_agent_os.work_items(id) ON DELETE CASCADE,
    source_work_item_id uuid NOT NULL
        REFERENCES qintopia_agent_os.work_items(id) ON DELETE CASCADE,
    workflow_root_id uuid NOT NULL
        REFERENCES qintopia_agent_os.work_items(id) ON DELETE CASCADE,
    generated_image_artifact_id uuid UNIQUE
        REFERENCES qintopia_agent_os.artifacts(id) ON DELETE CASCADE,
    notification_kind text NOT NULL DEFAULT 'image_ready',
    failure_code text,
    origin_ref text NOT NULL
        REFERENCES qintopia_agent_os.poster_return_targets(origin_ref) ON DELETE RESTRICT,
    status text NOT NULL DEFAULT 'pending',
    claimed_by text,
    claimed_at timestamptz,
    claim_expires_at timestamptz,
    attempt_count integer NOT NULL DEFAULT 0,
    last_error_code text,
    external_message_ref_hash text,
    delivered_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT poster_notifications_status_check CHECK (
        status IN ('pending', 'claimed', 'delivered', 'failed', 'ambiguous')
    ),
    CONSTRAINT poster_notifications_kind_check CHECK (
        notification_kind IN ('image_ready', 'generation_failed', 'generation_ambiguous')
    ),
    CONSTRAINT poster_notifications_payload_check CHECK (
        (notification_kind = 'image_ready' AND generated_image_artifact_id IS NOT NULL AND failure_code IS NULL)
        OR
        (notification_kind IN ('generation_failed', 'generation_ambiguous')
         AND generated_image_artifact_id IS NULL AND failure_code IS NOT NULL)
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS poster_notifications_source_kind_idx
    ON qintopia_agent_os.poster_notifications (source_work_item_id, notification_kind);

CREATE INDEX IF NOT EXISTS poster_notifications_claimable_idx
    ON qintopia_agent_os.poster_notifications (status, created_at)
    WHERE status = 'pending';

CREATE TABLE IF NOT EXISTS qintopia_agent_os.poster_notification_attempts (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    notification_id uuid NOT NULL
        REFERENCES qintopia_agent_os.poster_notifications(id) ON DELETE CASCADE,
    attempt_number integer NOT NULL,
    claim_token text NOT NULL,
    status text NOT NULL DEFAULT 'uploading',
    image_key_hash text,
    external_message_ref_hash text,
    failure_code text,
    audit_metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    send_started_at timestamptz,
    completed_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (notification_id, attempt_number),
    CONSTRAINT poster_notification_attempts_status_check CHECK (
        status IN ('uploading', 'sending', 'delivered', 'failed', 'ambiguous')
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS poster_notification_attempts_active_idx
    ON qintopia_agent_os.poster_notification_attempts (notification_id)
    WHERE status IN ('uploading', 'sending', 'delivered');

CREATE TABLE IF NOT EXISTS qintopia_agent_os.poster_review_actions (
    callback_event_id text PRIMARY KEY,
    notification_id uuid NOT NULL
        REFERENCES qintopia_agent_os.poster_notifications(id) ON DELETE CASCADE,
    artifact_id uuid NOT NULL
        REFERENCES qintopia_agent_os.artifacts(id) ON DELETE CASCADE,
    actor_ref text NOT NULL,
    decision text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT poster_review_actions_decision_check CHECK (
        decision IN ('approved', 'changes_requested', 'rejected')
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS poster_review_actions_notification_idx
    ON qintopia_agent_os.poster_review_actions (notification_id);

CREATE TABLE IF NOT EXISTS qintopia_agent_os.poster_revision_requests (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    workflow_root_id uuid NOT NULL
        REFERENCES qintopia_agent_os.work_items(id) ON DELETE CASCADE,
    source_artifact_id uuid NOT NULL
        REFERENCES qintopia_agent_os.artifacts(id) ON DELETE RESTRICT,
    source_message_ref text NOT NULL UNIQUE,
    actor_ref text NOT NULL,
    instruction_text text NOT NULL,
    image_generation_work_item_id uuid UNIQUE
        REFERENCES qintopia_agent_os.work_items(id) ON DELETE SET NULL,
    status text NOT NULL DEFAULT 'accepted',
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT poster_revision_requests_status_check CHECK (
        status IN ('accepted', 'queued', 'completed', 'failed', 'abandoned')
    )
);

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
        'xiaoman.notify_direct_conversation',
        'xiaoman',
        '小满原会话成图回传',
        'Return one pending generated image to its trusted originating Feishu direct conversation for review.',
        ARRAY['xiaoman']::text[],
        ARRAY['conversation_notification_request']::text[],
        'high',
        'origin_conversation_only',
        '{"required":["notification_type","origin_conversation_ref"],"properties":{"notification_type":{"enum":["image_ready","generation_failed","generation_ambiguous"]},"generated_image_artifact_id":{"type":["string","null"]},"failure_code":{"type":["string","null"]},"origin_conversation_ref":{"type":"string"}},"oneOf":[{"properties":{"notification_type":{"const":"image_ready"},"generated_image_artifact_id":{"type":"string"}},"required":["generated_image_artifact_id"]},{"properties":{"notification_type":{"enum":["generation_failed","generation_ambiguous"]},"failure_code":{"type":"string"}},"required":["failure_code"]}]}'::jsonb,
        '{"events":["conversation_notification_delivered","conversation_notification_failed","conversation_notification_ambiguous"]}'::jsonb,
        '{"external_send":true,"origin_direct_only":true,"artifact_review_unchanged":true,"group_send_authorized":false}'::jsonb
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
        '2026-07-31.001',
        '202607310001_xiaoman_poster_async_return.sql',
        'Adds trusted Feishu direct-conversation correlation, durable poster notification attempts, and idempotent review callbacks.',
        'docs/data-design/2026-07-31-xiaoman-poster-async-return.md',
        '{"change_type":"additive","domain":"xiaoman_poster","fact_source":"postgres","automatic_group_send":false}'::jsonb
    )
ON CONFLICT (schema_version) DO UPDATE SET
    migration_name = EXCLUDED.migration_name,
    status = 'applied',
    summary = EXCLUDED.summary,
    design_doc_path = EXCLUDED.design_doc_path,
    metadata = EXCLUDED.metadata,
    applied_at = now();

COMMIT;
