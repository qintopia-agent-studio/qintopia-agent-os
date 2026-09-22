-- Design: docs/data-design/2026-09-22-person-foundation-consumers.md
-- Inert local consumers; does not activate a production executor or grant authority.
CREATE TABLE IF NOT EXISTS qintopia_agent_os.collaboration_knowledge_items (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_key text NOT NULL,
    scope_id uuid NOT NULL,
    knowledge_key text NOT NULL CHECK (knowledge_key ~ '^[a-z][a-z0-9_.-]{0,79}$'),
    case_ref uuid,
    space_id uuid NOT NULL REFERENCES qintopia_messages.conversations(id),
    definition_key text NOT NULL,
    kind text NOT NULL CHECK (kind IN ('rule','fact','culture','experience','principle')),
    shared boolean NOT NULL DEFAULT false,
    version integer NOT NULL DEFAULT 0,
    UNIQUE (tenant_key,id),
    FOREIGN KEY (tenant_key,scope_id) REFERENCES qintopia_agent_os.collaboration_scopes(tenant_key,id)
);
CREATE UNIQUE INDEX IF NOT EXISTS collaboration_knowledge_item_key
    ON qintopia_agent_os.collaboration_knowledge_items(tenant_key,scope_id,knowledge_key,COALESCE(case_ref,'00000000-0000-0000-0000-000000000000'::uuid));
CREATE TABLE IF NOT EXISTS qintopia_agent_os.collaboration_knowledge_revisions (
    id uuid PRIMARY KEY REFERENCES qintopia_agent_os.business_definition_versions(id),
    tenant_key text NOT NULL,
    item_id uuid NOT NULL,
    author_person_id uuid NOT NULL REFERENCES qintopia_identity.persons(id),
    authority_grant_id uuid NOT NULL REFERENCES qintopia_agent_os.collaboration_grants(id),
    effective_at timestamptz NOT NULL,
    effective_until timestamptz,
    withdrawn_at timestamptz,
    FOREIGN KEY (tenant_key,item_id) REFERENCES qintopia_agent_os.collaboration_knowledge_items(tenant_key,id),
    CHECK (effective_until IS NULL OR effective_until>effective_at)
);
CREATE TABLE IF NOT EXISTS qintopia_agent_os.collaboration_tool_receipts (
    tenant_key text NOT NULL REFERENCES qintopia_agent_os.collaboration_tenants(tenant_key),
    operation_id uuid NOT NULL,
    person_id uuid NOT NULL REFERENCES qintopia_identity.persons(id),
    request_hash text NOT NULL,
    result jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY(tenant_key,operation_id)
);
CREATE TABLE IF NOT EXISTS qintopia_agent_os.collaboration_work_requests (
    work_item_id uuid PRIMARY KEY REFERENCES qintopia_agent_os.work_items(id),
    tenant_key text NOT NULL,
    scope_id uuid NOT NULL,
    actor_identity_id uuid NOT NULL REFERENCES qintopia_identity.source_identity_links(id),
    identity_version bigint NOT NULL,
    person_id uuid NOT NULL REFERENCES qintopia_identity.persons(id),
    authority_grant_id uuid NOT NULL REFERENCES qintopia_agent_os.collaboration_grants(id),
    request_hash text NOT NULL,
    approval_person_id uuid REFERENCES qintopia_identity.persons(id),
    approved_input_hash text,
    result jsonb,
    FOREIGN KEY (tenant_key,scope_id) REFERENCES qintopia_agent_os.collaboration_scopes(tenant_key,id)
);
CREATE TABLE IF NOT EXISTS qintopia_agent_os.collaboration_local_executors (
    tenant_key text NOT NULL REFERENCES qintopia_agent_os.collaboration_tenants(tenant_key),
    agent_key text NOT NULL CHECK(agent_key='erhua'),
    available boolean NOT NULL DEFAULT false,
    PRIMARY KEY(tenant_key,agent_key)
);
CREATE TABLE IF NOT EXISTS qintopia_agent_os.collaboration_turn_sources (
    tenant_key text NOT NULL REFERENCES qintopia_agent_os.collaboration_tenants(tenant_key),
    message_ref uuid NOT NULL,
    person_id uuid NOT NULL REFERENCES qintopia_identity.persons(id),
    observed_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY(tenant_key,message_ref)
);
INSERT INTO qintopia_agent_os.capabilities(capability_key,provider_agent,display_name,allowed_callers,allowed_work_item_types,enabled,metadata)
VALUES ('erhua.foundation_rule','erhua','本栋规则持久维护',ARRAY['default','erhua','silaoshi'],ARRAY['foundation_rule_request'],false,'{"local_only":true,"authority":"collaboration_grants"}'),
       ('erhua.foundation_context','erhua','本栋受控背景整理',ARRAY['default','silaoshi'],ARRAY['foundation_context_request'],false,'{"local_only":true,"authority":"collaboration_grants"}')
ON CONFLICT(capability_key) DO NOTHING;
INSERT INTO qintopia_agent_os.schema_change_log(schema_version,migration_name,summary,design_doc_path,metadata)
VALUES ('2026-09-22.001','202609220001_person_foundation_consumers.sql','Shared authority consumers, governed Space knowledge references and durable local work requests.',
'docs/data-design/2026-09-22-person-foundation-consumers.md','{"external_effects":false}')
ON CONFLICT(schema_version) DO NOTHING;
