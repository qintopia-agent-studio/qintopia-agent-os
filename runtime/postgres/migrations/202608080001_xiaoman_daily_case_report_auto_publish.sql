-- Design: docs/data-design/2026-08-08-xiaoman-daily-case-report-auto-publish.md
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
        'xiaoman.daily_case_report_auto_publish',
        'xiaoman',
        '小满日报自动发布',
        'Bind a durable Xiaoman daily case-report JPEG artifact to one automatic QiWe image-send work item without per-day human confirmation.',
        ARRAY['xiaoman']::text[],
        ARRAY['daily_case_report_request']::text[],
        'high',
        'automatic_publish',
        '{"required":["window_start","window_end","artifact_uri","content_hash","file_md5","byte_size","target_group_id"],"properties":{"window_start":{"type":"string"},"window_end":{"type":"string"},"artifact_uri":{"type":"string"},"content_hash":{"type":"string"},"file_md5":{"type":"string"},"byte_size":{"type":"integer"},"target_group_id":{"type":"string"}}}'::jsonb,
        '{"artifact_types":["generated_image"],"send_work_item_type":"group_message_request","requires_human_final_confirmation":false}'::jsonb,
        '{"workflow":"workflows/xiaoman-daily-case-report","external_send":true,"local_image_path_forbidden":true}'::jsonb
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
