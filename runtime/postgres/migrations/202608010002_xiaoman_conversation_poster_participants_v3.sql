BEGIN;

CREATE UNIQUE INDEX IF NOT EXISTS poster_workflow_participants_one_requester_idx
    ON qintopia_agent_os.poster_workflow_participants (workflow_root_id)
    WHERE participant_role = 'requester';

ALTER TABLE qintopia_agent_os.poster_revision_requests
    ADD COLUMN IF NOT EXISTS first_revision_guarded boolean NOT NULL DEFAULT false;

CREATE UNIQUE INDEX IF NOT EXISTS poster_revision_requests_guarded_source_artifact_idx
    ON qintopia_agent_os.poster_revision_requests (source_artifact_id)
    WHERE first_revision_guarded;

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
        'xiaoman.notify_conversation',
        'xiaoman',
        '小满原会话任务通知',
        'Return a poster result to its trusted originating direct conversation or internal collaboration thread without publication authorization.',
        ARRAY['xiaoman']::text[],
        ARRAY['conversation_notification_request']::text[],
        'high',
        'origin_conversation_only',
        '{"required":["notification_type","origin_conversation_ref"],"properties":{"notification_type":{"enum":["image_ready","generation_failed","generation_ambiguous"]},"generated_image_artifact_id":{"type":["string","null"]},"failure_code":{"type":["string","null"]},"origin_conversation_ref":{"type":"string"}},"oneOf":[{"properties":{"notification_type":{"const":"image_ready"},"generated_image_artifact_id":{"type":"string"}},"required":["generated_image_artifact_id"]},{"properties":{"notification_type":{"enum":["generation_failed","generation_ambiguous"]},"failure_code":{"type":"string"}},"required":["failure_code"]}]}'::jsonb,
        '{"events":["conversation_notification_delivered","conversation_notification_failed","conversation_notification_ambiguous"]}'::jsonb,
        '{"external_send":true,"origin_conversation_only":true,"direct_chat":true,"internal_group_thread":true,"group_send_authorized":false,"public_send":false}'::jsonb
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
        '2026-08-01.002',
        '202608010002_xiaoman_conversation_poster_participants_v3.sql',
        'Activates immutable poster participant authority, first-valid image revision semantics, and trusted direct-or-thread conversation notifications.',
        'docs/data-design/2026-08-01-xiaoman-conversation-poster-participants-v3.md',
        '{"change_type":"additive","domain":"xiaoman_conversation_poster","fact_source":"postgres","internal_group_delivery_enabled":false,"automatic_group_send":false}'::jsonb
    )
ON CONFLICT (schema_version) DO UPDATE SET
    migration_name = EXCLUDED.migration_name,
    status = 'applied',
    summary = EXCLUDED.summary,
    design_doc_path = EXCLUDED.design_doc_path,
    metadata = EXCLUDED.metadata,
    applied_at = now();

COMMIT;
