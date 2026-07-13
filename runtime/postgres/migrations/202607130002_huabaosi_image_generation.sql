-- Design: runtime/postgres/docs/data-design/2026-07-13-huabaosi-image-generation.md
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
        'huabaosi.generate_image_asset',
        'huabaosi',
        '画报司生成审核前图片素材',
        'Create a controlled image-generation request from an approved poster brief. Generated images remain pending human review and are not published or sent.',
        ARRAY['xiaoman', 'default']::text[],
        ARRAY['image_generation_request']::text[],
        'high',
        'before_external_use',
        '{"required":["approved_brief_artifact_id","approved_brief_content_hash","image_specification","prompt_hash"],"properties":{"approved_brief_artifact_id":{"type":"string"},"approved_brief_content_hash":{"type":"string"},"evidence_content_hash":{"type":"string"},"image_specification":{"type":"string"},"prompt_hash":{"type":"string"}}}'::jsonb,
        '{"artifact_types":["generated_image"],"review_status":"pending"}'::jsonb,
        '{"external_provider_default_enabled":false,"media_storage":"owner_review_required","external_publish":false}'::jsonb
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
    updated_at = now();
