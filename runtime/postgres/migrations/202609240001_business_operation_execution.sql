-- Design: docs/data-design/2026-09-24-business-operation-execution.md
-- Inert local-only primitives; no default business grants or production activation.
ALTER TABLE qintopia_agent_os.collaboration_grants
    DROP CONSTRAINT IF EXISTS collaboration_grants_action_key_check;
ALTER TABLE qintopia_agent_os.collaboration_grants
    ADD CONSTRAINT collaboration_grants_action_key_check CHECK (action_key IN
    ('submit','confirm_knowledge','train','change_rules','review','publish','designate',
     'identity','manage','technical_support','read_business','execute_business'));

ALTER TABLE qintopia_agent_os.collaboration_roles
    DROP CONSTRAINT IF EXISTS collaboration_roles_available_actions_check;
ALTER TABLE qintopia_agent_os.collaboration_roles
    ADD CONSTRAINT collaboration_roles_available_actions_check CHECK (available_actions <@
    ARRAY['confirm_knowledge','train','change_rules','review','publish','designate',
          'identity','manage','technical_support','read_business','execute_business']::text[]);
ALTER TABLE qintopia_agent_os.collaboration_duties
    DROP CONSTRAINT IF EXISTS collaboration_duties_available_actions_check;
ALTER TABLE qintopia_agent_os.collaboration_duties
    ADD CONSTRAINT collaboration_duties_available_actions_check CHECK (available_actions <@
    ARRAY['confirm_knowledge','train','change_rules','review','publish','designate',
          'identity','manage','technical_support','read_business','execute_business']::text[]);

CREATE TABLE IF NOT EXISTS qintopia_agent_os.business_property_bindings (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_key text NOT NULL,
    scope_id uuid NOT NULL,
    source_instance text NOT NULL,
    property_id text NOT NULL,
    active boolean NOT NULL DEFAULT true,
    version bigint NOT NULL DEFAULT 1,
    UNIQUE(tenant_key,id),
    UNIQUE(tenant_key,scope_id,source_instance,property_id),
    FOREIGN KEY(tenant_key,scope_id) REFERENCES qintopia_agent_os.collaboration_scopes(tenant_key,id)
);
CREATE TABLE IF NOT EXISTS qintopia_agent_os.business_operation_grants (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_key text NOT NULL,
    authority_grant_id uuid NOT NULL,
    binding_id uuid NOT NULL,
    operation_key text NOT NULL,
    parent_id uuid,
    issued_by uuid NOT NULL REFERENCES qintopia_identity.persons(id),
    valid_until timestamptz,
    revoked_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    UNIQUE(tenant_key,id),
    FOREIGN KEY(tenant_key,authority_grant_id) REFERENCES qintopia_agent_os.collaboration_grants(tenant_key,id),
    FOREIGN KEY(tenant_key,binding_id) REFERENCES qintopia_agent_os.business_property_bindings(tenant_key,id),
    FOREIGN KEY(tenant_key,parent_id) REFERENCES qintopia_agent_os.business_operation_grants(tenant_key,id),
    CHECK(parent_id IS DISTINCT FROM id)
);
CREATE UNIQUE INDEX IF NOT EXISTS business_operation_grant_current
    ON qintopia_agent_os.business_operation_grants(tenant_key,authority_grant_id,binding_id,operation_key)
    WHERE revoked_at IS NULL;
CREATE TABLE IF NOT EXISTS qintopia_agent_os.business_turn_evidence (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_key text NOT NULL REFERENCES qintopia_agent_os.collaboration_tenants(tenant_key),
    gateway_key text NOT NULL,
    platform text NOT NULL CHECK(platform IN ('wecom','qiwe')),
    chat_hash text NOT NULL,
    chat_type text NOT NULL CHECK(chat_type IN ('direct','group')),
    message_hash text NOT NULL,
    person_id uuid NOT NULL REFERENCES qintopia_identity.persons(id),
    identity_id uuid NOT NULL REFERENCES qintopia_identity.source_identity_links(id),
    identity_version bigint NOT NULL,
    content_hash text NOT NULL,
    confirmation_code text,
    explicit_intent jsonb,
    observed_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    UNIQUE(tenant_key,gateway_key,message_hash),
    UNIQUE(tenant_key,id)
);
INSERT INTO qintopia_agent_os.capabilities(capability_key,provider_agent,display_name,allowed_callers,allowed_work_item_types,enabled,metadata)
VALUES('anan.pms','anan','客房受控办理',ARRAY['anan'],ARRAY['business_operation'],false,'{"local_only":true}')
ON CONFLICT(capability_key) DO NOTHING;
CREATE TABLE IF NOT EXISTS qintopia_agent_os.business_actions (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_key text NOT NULL,
    work_item_id uuid NOT NULL REFERENCES qintopia_agent_os.work_items(id),
    binding_id uuid NOT NULL,
    binding_version bigint NOT NULL,
    operation_key text NOT NULL,
    actor_person_id uuid NOT NULL REFERENCES qintopia_identity.persons(id),
    actor_identity_id uuid NOT NULL REFERENCES qintopia_identity.source_identity_links(id),
    identity_version bigint NOT NULL,
    source_evidence_id uuid NOT NULL,
    authority_operation_id uuid NOT NULL,
    version bigint NOT NULL DEFAULT 1,
    request_hash text NOT NULL,
    input jsonb NOT NULL,
    reason jsonb NOT NULL,
    phase text NOT NULL CHECK(phase IN ('draft','previewing','awaiting_confirmation','executing','unknown','completed','not_executed','paused','cancelled','manual_handoff','manual_completed','preview_rejected')),
    preview_key text NOT NULL UNIQUE,
    execution_key text NOT NULL UNIQUE,
    resolution_key text NOT NULL UNIQUE,
    correlation_id text NOT NULL UNIQUE,
    preview jsonb,
    confirmation_code text NOT NULL UNIQUE,
    confirmation_evidence_id uuid,
    confirmed_by uuid REFERENCES qintopia_identity.persons(id),
    claim_id uuid,
    result jsonb,
    readback jsonb,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    UNIQUE(tenant_key,id),
    UNIQUE(tenant_key,source_evidence_id,operation_key,request_hash),
    FOREIGN KEY(tenant_key,binding_id) REFERENCES qintopia_agent_os.business_property_bindings(tenant_key,id),
    FOREIGN KEY(tenant_key,source_evidence_id) REFERENCES qintopia_agent_os.business_turn_evidence(tenant_key,id),
    FOREIGN KEY(tenant_key,confirmation_evidence_id) REFERENCES qintopia_agent_os.business_turn_evidence(tenant_key,id),
    FOREIGN KEY(tenant_key,authority_operation_id) REFERENCES qintopia_agent_os.business_operation_grants(tenant_key,id)
);
CREATE TABLE IF NOT EXISTS qintopia_agent_os.business_feed_checkpoints (
    tenant_key text NOT NULL,
    binding_id uuid NOT NULL,
    feed text NOT NULL,
    enabled_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    baseline_cursor text NOT NULL,
    cursor_value text NOT NULL DEFAULT '0',
    version bigint NOT NULL DEFAULT 0,
    PRIMARY KEY(tenant_key,binding_id,feed),
    FOREIGN KEY(tenant_key,binding_id) REFERENCES qintopia_agent_os.business_property_bindings(tenant_key,id)
);
CREATE TABLE IF NOT EXISTS qintopia_agent_os.business_event_inbox (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_key text NOT NULL,
    binding_id uuid NOT NULL,
    feed text NOT NULL,
    event_id text NOT NULL,
    subject_ref text NOT NULL,
    source_revision text NOT NULL,
    event_hash text NOT NULL,
    payload jsonb NOT NULL,
    baseline boolean NOT NULL,
    work_item_id uuid REFERENCES qintopia_agent_os.work_items(id),
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    UNIQUE(tenant_key,binding_id,feed,event_id),
    FOREIGN KEY(tenant_key,binding_id) REFERENCES qintopia_agent_os.business_property_bindings(tenant_key,id)
);
INSERT INTO qintopia_agent_os.schema_change_log(schema_version,migration_name,summary,design_doc_path,metadata)
VALUES ('2026-09-24.001','202609240001_business_operation_execution.sql',
'Exact business operation grants, trusted turn evidence, per-action WorkItem execution and durable event checkpoints.',
'runtime/postgres/docs/data-design/2026-09-24-business-operation-execution.md','{"external_effects":false,"local_only":true}')
ON CONFLICT(schema_version) DO NOTHING;
