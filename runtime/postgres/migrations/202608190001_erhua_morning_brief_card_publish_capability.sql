-- Design: docs/data-design/2026-08-19-erhua-morning-brief-card-publish-capability.md
-- Registers the AgentOS capability backing the Erhua morning-brief card publish
-- command (operations-erhua-morning-brief-card-publish-create). The command was
-- introduced with the card-send work (#648) but its capability row was never
-- seeded, so creating the source work item failed with a
-- work_items_capability_key_fkey violation and the worker degraded the card to
-- the text brief. This migration is additive and idempotent.
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
        'erhua.morning_brief_card_publish',
        'erhua',
        '二花早报卡片自动发布',
        'Bind a rendered Erhua morning-brief card JPEG to one automatic QiWe image-send work item without per-day human final confirmation; the card source work item and approved generated-image artifact are created in one transaction.',
        ARRAY['xiaoman']::text[],
        ARRAY['morning_brief_card_request']::text[],
        'high',
        'automatic_publish',
        '{
            "required": ["brief_date", "artifact_uri", "content_hash", "file_md5", "byte_size", "target_group_id"],
            "properties": {
                "brief_date": {"type": "string"},
                "artifact_uri": {"type": "string"},
                "content_hash": {"type": "string"},
                "file_md5": {"type": "string"},
                "byte_size": {"type": "integer"},
                "width": {"type": "integer"},
                "height": {"type": "integer"},
                "filename": {"type": "string"},
                "target_group_id": {"type": "string"},
                "message_text": {"type": "string", "maxLength": 500},
                "title": {"type": "string"}
            }
        }'::jsonb,
        '{
            "artifact_types": ["generated_image"],
            "send_work_item_type": "group_message_request",
            "requires_human_final_confirmation": false
        }'::jsonb,
        '{
            "workflow": "workflows/erhua-morning-brief",
            "external_send": true,
            "local_image_path_forbidden": true,
            "message_text_max_chars": 500
        }'::jsonb
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
    metadata = EXCLUDED.metadata,
    enabled = true,
    updated_at = now();

INSERT INTO qintopia_agent_os.schema_change_log
    (schema_version, migration_name, summary, design_doc_path, metadata)
VALUES
    (
        '2026-08-19.001',
        '202608190001_erhua_morning_brief_card_publish_capability.sql',
        'Registers the erhua.morning_brief_card_publish capability so card-publish-create can create its source work item; the row was missing since the card-send feature shipped, causing card delivery to degrade to the text brief.',
        'docs/data-design/2026-08-19-erhua-morning-brief-card-publish-capability.md',
        '{"change_type":"additive","domain":"erhua_morning_brief_card","fact_source":"postgres","external_sends":true,"new_timers":false}'::jsonb
    )
ON CONFLICT (schema_version) DO UPDATE SET
    migration_name = EXCLUDED.migration_name,
    status = 'applied',
    summary = EXCLUDED.summary,
    design_doc_path = EXCLUDED.design_doc_path,
    metadata = EXCLUDED.metadata,
    applied_at = now();
