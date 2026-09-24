-- Additive local foundation integration. No grants or production activation.
CREATE TABLE IF NOT EXISTS qintopia_agent_os.welcome_foundation_targets (
    target_id uuid PRIMARY KEY REFERENCES qintopia_agent_os.welcome_targets(id),
    tenant_key text NOT NULL REFERENCES qintopia_agent_os.collaboration_tenants(tenant_key),
    scope_id uuid NOT NULL REFERENCES qintopia_agent_os.collaboration_scopes(id),
    created_at timestamptz NOT NULL DEFAULT now()
);

ALTER TABLE qintopia_agent_os.welcome_approvals ALTER COLUMN grant_id DROP NOT NULL;
ALTER TABLE qintopia_agent_os.welcome_approvals ALTER COLUMN grant_version DROP NOT NULL;
ALTER TABLE qintopia_agent_os.welcome_approvals
    ADD COLUMN IF NOT EXISTS foundation_basis jsonb;
ALTER TABLE qintopia_agent_os.welcome_actions ALTER COLUMN approval_id DROP NOT NULL;
ALTER TABLE qintopia_agent_os.welcome_actions ALTER COLUMN publish_grant_id DROP NOT NULL;
ALTER TABLE qintopia_agent_os.welcome_actions ALTER COLUMN publish_grant_version DROP NOT NULL;
ALTER TABLE qintopia_agent_os.welcome_actions
    ADD COLUMN IF NOT EXISTS foundation_basis jsonb,
    ADD COLUMN IF NOT EXISTS artifact_id uuid REFERENCES qintopia_agent_os.artifacts(id);

ALTER TABLE qintopia_agent_os.welcome_artifact_bindings
    ADD COLUMN IF NOT EXISTS target_id uuid REFERENCES qintopia_agent_os.welcome_targets(id);

CREATE TABLE IF NOT EXISTS qintopia_agent_os.welcome_local_artifact_data (
    artifact_id uuid PRIMARY KEY REFERENCES qintopia_agent_os.artifacts(id),
    content_hash text NOT NULL CHECK (content_hash ~ '^[0-9a-f]{64}$'),
    content bytea NOT NULL CHECK (octet_length(content) BETWEEN 1 AND 10485760),
    media_type text NOT NULL CHECK (media_type IN ('image/png','text/plain'))
);

CREATE TABLE IF NOT EXISTS qintopia_agent_os.welcome_synthetic_effects (
    action_id uuid PRIMARY KEY REFERENCES qintopia_agent_os.welcome_actions(id),
    attempt_id uuid NOT NULL,
    agent_key text NOT NULL CHECK (agent_key IN ('erhua','silaoshi')),
    artifact_id uuid NOT NULL REFERENCES qintopia_agent_os.artifacts(id),
    content_hash text NOT NULL,
    receipt_hash text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

INSERT INTO qintopia_agent_os.schema_change_log
    (schema_version,migration_name,summary,design_doc_path,metadata)
VALUES ('2026-09-22.003','202609220003_foundation_welcome.sql',
    'Connect welcome actions to shared authorization and versioned rules; synthetic adapters only',
    'docs/data-design/2026-09-22-foundation-welcome.md',
    '{"change_type":"additive","external_execution_enabled":false}'::jsonb)
ON CONFLICT (schema_version) DO NOTHING;

DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname='welcome_action_authority_source') THEN
        ALTER TABLE qintopia_agent_os.welcome_actions ADD CONSTRAINT welcome_action_authority_source CHECK (
            (foundation_basis IS NULL AND approval_id IS NOT NULL AND publish_grant_id IS NOT NULL AND publish_grant_version IS NOT NULL)
            OR (foundation_basis IS NOT NULL AND artifact_id IS NOT NULL AND publish_grant_id IS NULL AND publish_grant_version IS NULL)
        );
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname='welcome_approval_authority_source') THEN
        ALTER TABLE qintopia_agent_os.welcome_approvals ADD CONSTRAINT welcome_approval_authority_source CHECK (
            (foundation_basis IS NULL AND grant_id IS NOT NULL AND grant_version IS NOT NULL)
            OR (foundation_basis IS NOT NULL AND grant_id IS NULL AND grant_version IS NULL)
        );
    END IF;
END $$;
