-- Design: runtime/postgres/docs/data-design/2026-09-24-welcome-source-and-group-projection.md
CREATE TABLE IF NOT EXISTS qintopia_agent_os.welcome_source_projections (
    tenant_key text NOT NULL,
    scope_id uuid NOT NULL,
    application_id uuid PRIMARY KEY REFERENCES qintopia_agent_os.welcome_applications(id),
    binding_id uuid NOT NULL,
    binding_version bigint NOT NULL,
    application_revision bigint NOT NULL,
    identity_hash text NOT NULL,
    field_hash text NOT NULL,
    hints jsonb NOT NULL CHECK (jsonb_typeof(hints)='object' AND octet_length(hints::text)<=2048),
    expires_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    FOREIGN KEY(tenant_key,binding_id) REFERENCES qintopia_agent_os.business_property_bindings(tenant_key,id),
    FOREIGN KEY(tenant_key,scope_id) REFERENCES qintopia_agent_os.collaboration_scopes(tenant_key,id)
);
CREATE TABLE IF NOT EXISTS qintopia_agent_os.welcome_group_presentations (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    reference text NOT NULL UNIQUE,
    tenant_key text NOT NULL,
    scope_id uuid NOT NULL,
    work_item_id uuid NOT NULL REFERENCES qintopia_agent_os.welcome_review_items(work_item_id),
    review_version bigint NOT NULL,
    configuration_version bigint NOT NULL,
    conversation_id uuid NOT NULL REFERENCES qintopia_messages.conversations(id),
    subject_kind text NOT NULL CHECK(subject_kind IN ('person','work_account')),
    subject_id uuid NOT NULL,
    subject_version bigint NOT NULL,
    subject_proof jsonb NOT NULL,
    snapshot jsonb NOT NULL,
    status text NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','claimed','delivered','failed','unknown','stale')),
    claim_id uuid,
    claim_until timestamptz,
    delivery_receipt text,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    UNIQUE(work_item_id,review_version,configuration_version),
    FOREIGN KEY(tenant_key,scope_id) REFERENCES qintopia_agent_os.welcome_review_settings(tenant_key,scope_id)
);
CREATE INDEX IF NOT EXISTS welcome_source_projection_scope_idx
    ON qintopia_agent_os.welcome_source_projections(tenant_key,scope_id,updated_at DESC,application_id);
CREATE INDEX IF NOT EXISTS welcome_group_presentation_scope_idx
    ON qintopia_agent_os.welcome_group_presentations(tenant_key,scope_id,status,created_at);
INSERT INTO qintopia_agent_os.schema_change_log(schema_version,migration_name,summary,design_doc_path,metadata)
VALUES('2026-09-24.005','202609240005_welcome_source_and_group_projection.sql',
       'Scoped source matching hints and durable local welcome group presentations.',
       'runtime/postgres/docs/data-design/2026-09-24-welcome-source-and-group-projection.md','{"external_effects":false}')
ON CONFLICT(schema_version) DO NOTHING;
