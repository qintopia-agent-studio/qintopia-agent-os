-- Design: docs/data-design/2026-08-15-space-execution-runner-contract.md
UPDATE qintopia_agent_os.capabilities
SET metadata = metadata || '{"space_execution_recipe":"qiwe_text_template_v1"}'::jsonb,
    updated_at = now()
WHERE capability_key = 'erhua.qiwe_text_template';

UPDATE qintopia_agent_os.capabilities
SET description =
        'Execute one version-bound Space agent turn through the dedicated authenticated runner broker and validate the result against the business-owned output contract.',
    input_schema =
        '{"type":"object","additionalProperties":false,"required":["goal","trigger","output_contract","capabilities"],"properties":{"goal":{"type":"string"},"trigger":{"type":"object"},"output_contract":{"type":"object"},"capabilities":{"type":"array"}}}'::jsonb,
    output_schema =
        '{"type":"object","additionalProperties":false,"required":["output"],"properties":{"output":{"type":"object"}}}'::jsonb,
    metadata = metadata
        || '{"runner_contract":"dedicated_broker_v1","runner_identity":"erhua-space-agent-runner-v1","runner_authentication":"socket_group_and_bearer_sha256","result_contract":"business_definition.output_contract","enablement":"owner_review_required"}'::jsonb,
    updated_at = now()
WHERE capability_key = 'erhua.space_agent_turn';

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
        enabled,
        metadata
    )
VALUES
    (
        'erhua.space_subject_identity_lookup',
        'erhua',
        'Erhua current trigger subject identity lookup',
        'Resolve only the current event trigger subject ids against a recent current-member roster sync for the exact current Space; external-send flows still require live room-detail verification.',
        ARRAY['erhua']::text[],
        ARRAY['space_agent_turn']::text[],
        'low',
        'definition_policy',
        '{"type":"object","additionalProperties":false,"required":["scope"],"properties":{"scope":{"type":"string","const":"trigger_subjects"}}}'::jsonb,
        '{"type":"object","additionalProperties":false,"required":["members"],"properties":{"members":{"type":"array","maxItems":64,"items":{"type":"object","additionalProperties":false,"required":["user_id","display_name","resolved"],"properties":{"user_id":{"type":"string","minLength":1,"maxLength":256},"display_name":{"type":"string","maxLength":200},"resolved":{"type":"boolean"}}}}}}'::jsonb,
        false,
        '{"space_invocable":true,"space_scope_binding":"work_item_space_id","invocation_boundary":"erhua.space_agent_turn","runner_access":"bounded_catalog_v1","space_agent_turn_recipe":"trigger_subject_identity_lookup_v1","roster_source":"recent_current_qiwe_room_member_sync","roster_max_age_hours":24,"live_verification_required_before_external_send":true,"direct_chat_invocation":false,"external_send":false,"enablement":"owner_review_required"}'::jsonb
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

INSERT INTO qintopia_agent_os.schema_change_log
    (schema_version, migration_name, summary, design_doc_path, metadata)
VALUES
    (
        '2026-08-15.001',
        '202608150001_space_execution_runner_contract.sql',
        'Adds declarative deterministic recipe metadata and the default-disabled authenticated Space agent-turn runner contract.',
        'docs/data-design/2026-08-15-space-execution-runner-contract.md',
        '{"change_type":"additive","domain":"space_execution","external_send":false,"default_enabled":false,"new_tables":0}'::jsonb
    )
ON CONFLICT (schema_version) DO UPDATE SET
    migration_name = EXCLUDED.migration_name,
    status = 'applied',
    summary = EXCLUDED.summary,
    design_doc_path = EXCLUDED.design_doc_path,
    metadata = EXCLUDED.metadata,
    applied_at = now();
