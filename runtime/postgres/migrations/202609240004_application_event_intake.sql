-- Design: docs/data-design/2026-09-24-application-event-intake.md
-- Source observation/dispatch state only. No application or identity master copy.
CREATE TABLE IF NOT EXISTS qintopia_agent_os.application_intake_states (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_key text NOT NULL,
    binding_id uuid NOT NULL,
    binding_version bigint NOT NULL CHECK (binding_version > 0),
    source_instance text NOT NULL,
    property_id text NOT NULL,
    resource_alias text NOT NULL,
    record_ref text NOT NULL,
    application_id uuid UNIQUE REFERENCES qintopia_agent_os.welcome_applications(id),
    identity_hash text CHECK (identity_hash ~ '^[0-9a-f]{64}$'),
    read_token uuid NOT NULL,
    expected_revision bigint NOT NULL DEFAULT 0 CHECK (expected_revision >= 0),
    completed_token uuid,
    completed_hash text CHECK (completed_hash ~ '^[0-9a-f]{64}$'),
    source_version text,
    anan_work_id uuid REFERENCES qintopia_agent_os.work_items(id),
    silaoshi_work_id uuid REFERENCES qintopia_agent_os.work_items(id),
    updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    UNIQUE (tenant_key,resource_alias,record_ref),
    UNIQUE (source_instance,property_id,resource_alias,record_ref),
    FOREIGN KEY (tenant_key,binding_id)
        REFERENCES qintopia_agent_os.business_property_bindings(tenant_key,id)
);
INSERT INTO qintopia_agent_os.capabilities
    (capability_key,provider_agent,display_name,allowed_callers,allowed_work_item_types,
     risk_level,review_policy,enabled,metadata)
VALUES ('silaoshi.application_review','silaoshi','申请运营跟进',ARRAY[]::text[],
    ARRAY['application_review'],'high','before_external_use',false,
    '{"local_only":true,"event_is_not_authority":true,"financial_approval":false}')
ON CONFLICT (capability_key) DO NOTHING;
INSERT INTO qintopia_agent_os.schema_change_log
    (schema_version,migration_name,summary,design_doc_path,metadata)
VALUES ('2026-09-24.004','202609240004_application_event_intake.sql',
    'Local trusted application readback fences and durable Anan/Silaoshi dispatch references.',
    'docs/data-design/2026-09-24-application-event-intake.md','{"external_effects":false}')
ON CONFLICT (schema_version) DO NOTHING;
