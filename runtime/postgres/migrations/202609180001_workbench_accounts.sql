-- Design: docs/data-design/2026-09-18-workbench-accounts.md
CREATE TABLE IF NOT EXISTS qintopia_agent_os.collaboration_accounts (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_key text NOT NULL REFERENCES qintopia_agent_os.collaboration_tenants(tenant_key),
    person_id uuid NOT NULL REFERENCES qintopia_identity.persons(id),
    identity_link_id uuid NOT NULL REFERENCES qintopia_identity.source_identity_links(id),
    username text NOT NULL CHECK (username ~ '^[a-z0-9][a-z0-9_.-]{2,63}$'),
    password_hash text NOT NULL,
    status text NOT NULL DEFAULT 'active' CHECK (status IN ('active','disabled')),
    version bigint NOT NULL DEFAULT 1,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE(tenant_key,username), UNIQUE(tenant_key,person_id), UNIQUE(tenant_key,id)
);
CREATE TABLE IF NOT EXISTS qintopia_agent_os.collaboration_sessions (
    token_hash text PRIMARY KEY,
    tenant_key text NOT NULL,
    account_id uuid NOT NULL,
    account_version bigint NOT NULL,
    identity_version bigint NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    revoked_at timestamptz,
    FOREIGN KEY(tenant_key,account_id) REFERENCES qintopia_agent_os.collaboration_accounts(tenant_key,id)
);
CREATE INDEX IF NOT EXISTS collaboration_sessions_account_idx
 ON qintopia_agent_os.collaboration_sessions(tenant_key,account_id);
CREATE TABLE IF NOT EXISTS qintopia_agent_os.collaboration_login_limits (
    tenant_key text NOT NULL REFERENCES qintopia_agent_os.collaboration_tenants(tenant_key),
    bucket text NOT NULL,
    window_start timestamptz NOT NULL,
    attempts integer NOT NULL,
    PRIMARY KEY(tenant_key,bucket)
);
INSERT INTO qintopia_agent_os.schema_change_log(schema_version,migration_name,summary,design_doc_path,metadata)
VALUES('2026-09-18.001','202609180001_workbench_accounts.sql',
 'Add personal accounts, revocable opaque sessions and persistent login limits; no business grants or live seeds.',
 'docs/data-design/2026-09-18-workbench-accounts.md','{"external_effects":false}')
ON CONFLICT(schema_version) DO NOTHING;
