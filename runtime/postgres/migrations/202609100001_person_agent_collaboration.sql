-- Additive F1 storage. No real administrators, authorizations or adapters are enabled.
CREATE TABLE IF NOT EXISTS qintopia_agent_os.collaboration_tenants (
    tenant_key text PRIMARY KEY,
    identity_namespace text NOT NULL UNIQUE,
    mode text NOT NULL CHECK (mode IN ('synthetic','shadow','live')),
    initialized boolean NOT NULL DEFAULT false,
    version bigint NOT NULL DEFAULT 1 CHECK (version > 0),
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS qintopia_agent_os.collaboration_scopes (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_key text NOT NULL REFERENCES qintopia_agent_os.collaboration_tenants(tenant_key),
    parent_scope_id uuid,
    label text NOT NULL CHECK (length(label) BETWEEN 1 AND 80),
    kind text NOT NULL CHECK (kind IN ('community','building','business')),
    status text NOT NULL DEFAULT 'active' CHECK (status IN ('active','revoked')),
    version bigint NOT NULL DEFAULT 1,
    UNIQUE (tenant_key,id),
    FOREIGN KEY (tenant_key,parent_scope_id) REFERENCES qintopia_agent_os.collaboration_scopes(tenant_key,id),
    CHECK (parent_scope_id IS DISTINCT FROM id)
);

CREATE TABLE IF NOT EXISTS qintopia_agent_os.collaboration_scope_bindings (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_key text NOT NULL,
    scope_id uuid NOT NULL,
    conversation_id uuid NOT NULL REFERENCES qintopia_messages.conversations(id),
    version bigint NOT NULL DEFAULT 1,
    created_at timestamptz NOT NULL DEFAULT now(),
    revoked_at timestamptz,
    FOREIGN KEY (tenant_key,scope_id) REFERENCES qintopia_agent_os.collaboration_scopes(tenant_key,id)
);
CREATE UNIQUE INDEX IF NOT EXISTS collaboration_scope_binding_current
    ON qintopia_agent_os.collaboration_scope_bindings(tenant_key,scope_id,conversation_id)
    WHERE revoked_at IS NULL;

CREATE TABLE IF NOT EXISTS qintopia_agent_os.collaboration_roles (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_key text NOT NULL REFERENCES qintopia_agent_os.collaboration_tenants(tenant_key),
    label text NOT NULL CHECK (length(label) BETWEEN 1 AND 80),
    UNIQUE (tenant_key,id),
    UNIQUE (tenant_key,label)
);

CREATE TABLE IF NOT EXISTS qintopia_agent_os.collaboration_appointments (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_key text NOT NULL,
    person_id uuid NOT NULL REFERENCES qintopia_identity.persons(id),
    role_id uuid NOT NULL,
    scope_id uuid NOT NULL,
    proxy_for_id uuid,
    status text NOT NULL DEFAULT 'active' CHECK (status IN ('active','ended','revoked')),
    valid_from timestamptz NOT NULL DEFAULT now(),
    valid_until timestamptz,
    version bigint NOT NULL DEFAULT 1,
    ended_at timestamptz,
    UNIQUE (tenant_key,id),
    FOREIGN KEY (tenant_key,role_id) REFERENCES qintopia_agent_os.collaboration_roles(tenant_key,id),
    FOREIGN KEY (tenant_key,scope_id) REFERENCES qintopia_agent_os.collaboration_scopes(tenant_key,id),
    FOREIGN KEY (tenant_key,proxy_for_id) REFERENCES qintopia_agent_os.collaboration_appointments(tenant_key,id),
    CHECK (valid_until IS NULL OR valid_until > valid_from),
    CHECK (proxy_for_id IS DISTINCT FROM id),
    CHECK (proxy_for_id IS NULL OR valid_until IS NOT NULL)
);
CREATE UNIQUE INDEX IF NOT EXISTS collaboration_appointment_current
    ON qintopia_agent_os.collaboration_appointments(tenant_key,person_id,role_id,scope_id)
    WHERE status = 'active';

CREATE TABLE IF NOT EXISTS qintopia_agent_os.agent_collaborations (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_key text NOT NULL,
    appointment_id uuid NOT NULL,
    agent_key text NOT NULL,
    domain_key text NOT NULL,
    responsibility_text text NOT NULL CHECK (length(responsibility_text) BETWEEN 1 AND 2000),
    status text NOT NULL DEFAULT 'active' CHECK (status IN ('active','ended','revoked')),
    version bigint NOT NULL DEFAULT 1,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (tenant_key,id),
    FOREIGN KEY (tenant_key,appointment_id) REFERENCES qintopia_agent_os.collaboration_appointments(tenant_key,id)
);
CREATE UNIQUE INDEX IF NOT EXISTS agent_collaboration_current
    ON qintopia_agent_os.agent_collaborations(tenant_key,appointment_id,agent_key,domain_key)
    WHERE status='active';

CREATE TABLE IF NOT EXISTS qintopia_agent_os.collaboration_grants (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_key text NOT NULL,
    collaboration_id uuid NOT NULL,
    action_key text NOT NULL CHECK (action_key IN
      ('submit','confirm_knowledge','train','change_rules','review','publish','designate','identity','manage','technical_support')),
    parent_grant_id uuid,
    include_descendants boolean NOT NULL DEFAULT false,
    -- Only manage grants may carry a delegation envelope. It is not execution authority.
    managed_agents text[] NOT NULL DEFAULT '{}',
    managed_domains text[] NOT NULL DEFAULT '{}',
    managed_actions text[] NOT NULL DEFAULT '{}',
    delegation_depth integer NOT NULL DEFAULT 0 CHECK (delegation_depth BETWEEN 0 AND 8),
    status text NOT NULL DEFAULT 'active' CHECK (status IN ('active','revoked')),
    version bigint NOT NULL DEFAULT 1,
    created_at timestamptz NOT NULL DEFAULT now(),
    revoked_at timestamptz,
    UNIQUE (tenant_key,id),
    FOREIGN KEY (tenant_key,collaboration_id) REFERENCES qintopia_agent_os.agent_collaborations(tenant_key,id),
    FOREIGN KEY (tenant_key,parent_grant_id) REFERENCES qintopia_agent_os.collaboration_grants(tenant_key,id),
    CHECK (parent_grant_id IS DISTINCT FROM id),
    CHECK (action_key='manage' OR
      (cardinality(managed_agents)=0 AND cardinality(managed_domains)=0 AND cardinality(managed_actions)=0 AND delegation_depth=0))
);
CREATE UNIQUE INDEX IF NOT EXISTS collaboration_grant_current
    ON qintopia_agent_os.collaboration_grants(tenant_key,collaboration_id,action_key)
    WHERE status='active';

CREATE TABLE IF NOT EXISTS qintopia_agent_os.collaboration_commands (
    id uuid PRIMARY KEY,
    tenant_key text NOT NULL REFERENCES qintopia_agent_os.collaboration_tenants(tenant_key),
    actor_identity_id uuid NOT NULL REFERENCES qintopia_identity.source_identity_links(id),
    actor_person_id uuid NOT NULL REFERENCES qintopia_identity.persons(id),
    request_hash text NOT NULL CHECK (request_hash ~ '^[0-9a-f]{64}$'),
    expected_version bigint NOT NULL,
    result jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (tenant_key,actor_identity_id,id)
);

INSERT INTO qintopia_agent_os.schema_change_log
    (schema_version,migration_name,summary,design_doc_path,metadata)
VALUES ('2026-09-10.001','202609100001_person_agent_collaboration.sql',
    'Add scoped appointments, collaborations, delegated grants and idempotent commands; no production activation.',
    'runtime/postgres/docs/data-design/2026-09-10-person-agent-collaboration-v1.md',
    '{"external_effects":false,"bootstrap_grants":false}')
ON CONFLICT (schema_version) DO NOTHING;
