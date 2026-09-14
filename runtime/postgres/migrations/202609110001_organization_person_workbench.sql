-- A additive organizational positions and scoped ledgers. No live seed or grants.
CREATE TABLE qintopia_agent_os.collaboration_positions (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_key text NOT NULL,
    role_id uuid NOT NULL,
    scope_id uuid NOT NULL,
    parent_id uuid,
    label text NOT NULL,
    description text NOT NULL,
    status text NOT NULL DEFAULT 'active' CHECK(status IN ('draft','active','retired')),
    version bigint NOT NULL DEFAULT 1,
    used boolean NOT NULL DEFAULT false,
    UNIQUE(tenant_key,id),
    UNIQUE(tenant_key,role_id,scope_id),
    FOREIGN KEY(tenant_key,role_id) REFERENCES qintopia_agent_os.collaboration_roles(tenant_key,id),
    FOREIGN KEY(tenant_key,scope_id) REFERENCES qintopia_agent_os.collaboration_scopes(tenant_key,id),
    FOREIGN KEY(tenant_key,parent_id) REFERENCES qintopia_agent_os.collaboration_positions(tenant_key,id),
    CHECK(parent_id IS DISTINCT FROM id)
);
CREATE TABLE qintopia_agent_os.collaboration_ledger (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_key text NOT NULL REFERENCES qintopia_agent_os.collaboration_tenants(tenant_key),
    kind text NOT NULL CHECK(kind IN ('person','agent','group')),
    object_ref text NOT NULL,
    label text NOT NULL,
    nickname text NOT NULL DEFAULT '',
    description text NOT NULL,
    scope_id uuid,
    owner_id uuid REFERENCES qintopia_identity.persons(id),
    status text NOT NULL CHECK(status IN ('draft','active','retired')),
    verified boolean NOT NULL DEFAULT false,
    used boolean NOT NULL DEFAULT false,
    version bigint NOT NULL DEFAULT 1,
    UNIQUE(tenant_key,kind,object_ref),
    FOREIGN KEY(tenant_key,scope_id) REFERENCES qintopia_agent_os.collaboration_scopes(tenant_key,id)
);
CREATE TABLE qintopia_agent_os.collaboration_audiences (
    collaboration_id uuid PRIMARY KEY,
    tenant_key text NOT NULL,
    configuration jsonb NOT NULL,
    version bigint NOT NULL DEFAULT 1,
    FOREIGN KEY(tenant_key,collaboration_id) REFERENCES qintopia_agent_os.agent_collaborations(tenant_key,id)
);
INSERT INTO qintopia_agent_os.schema_change_log(schema_version,migration_name,summary,design_doc_path,metadata)
VALUES('2026-09-11.001','202609110001_organization_person_workbench.sql',
 'Add organization positions, local ledgers and contact audience policies without granting authority.',
 'docs/plans/active/organization-workbench-a-increment.md','{"external_effects":false}')
ON CONFLICT(schema_version) DO NOTHING;
