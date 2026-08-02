-- Design: runtime/postgres/docs/data-design/2026-08-02-xiaoman-material-followup-capability.md
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
        'xiaoman.material_followup_request',
        'xiaoman',
        '小满活动素材回填催办',
        'Create internal Xiaoman material follow-up reminders and escalation drafts from sanitized activity occurrence records without authorizing external sends.',
        ARRAY['xiaoman']::text[],
        ARRAY['activity_recap_request']::text[],
        'medium',
        'before_external_use',
        '{
            "required": ["brief_summary", "source_refs"],
            "properties": {
                "brief_summary": {"type": "string"},
                "source_refs": {"type": "object"},
                "activity_phase": {"type": "string", "enum": ["post_event"]},
                "material_followup_attempt": {"type": "integer", "minimum": 1, "maximum": 3},
                "escalation_required": {"type": "boolean"},
                "external_send_executed": {"type": "boolean"}
            }
        }'::jsonb,
        '{
            "work_item_types": ["activity_recap_request"],
            "external_send": false
        }'::jsonb,
        '{
            "source_agent": "xiaoman",
            "activity_phase": "post_event",
            "external_send": false,
            "requires_approved_text_before_group_message": true
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
        '2026-08-02.001',
        '202608020001_xiaoman_material_followup_capability.sql',
        'Adds an internal Xiaoman material follow-up capability for idempotent post-event reminder and escalation work items without external send authorization.',
        'docs/data-design/2026-08-02-xiaoman-material-followup-capability.md',
        '{"change_type":"additive","domain":"xiaoman_material_followup","fact_source":"postgres","external_sends":false,"new_timers":false}'::jsonb
    )
ON CONFLICT (schema_version) DO UPDATE SET
    migration_name = EXCLUDED.migration_name,
    status = 'applied',
    summary = EXCLUDED.summary,
    design_doc_path = EXCLUDED.design_doc_path,
    metadata = EXCLUDED.metadata,
    applied_at = now();
