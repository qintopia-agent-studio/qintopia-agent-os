-- Design: docs/data-design/2026-09-23-steward-review-delegation.md
CREATE TABLE IF NOT EXISTS qintopia_agent_os.collaboration_review_delegations (
    id uuid PRIMARY KEY,
    tenant_key text NOT NULL REFERENCES qintopia_agent_os.collaboration_tenants(tenant_key),
    scope_id uuid NOT NULL REFERENCES qintopia_agent_os.collaboration_scopes(id),
    owner_person_id uuid NOT NULL REFERENCES qintopia_identity.persons(id),
    delegate_person_id uuid NOT NULL REFERENCES qintopia_identity.persons(id),
    designate_grant_id uuid NOT NULL REFERENCES qintopia_agent_os.collaboration_grants(id),
    review_grant_id uuid NOT NULL REFERENCES qintopia_agent_os.collaboration_grants(id),
    valid_from timestamptz NOT NULL,
    valid_until timestamptz NOT NULL,
    revoked_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    CHECK (valid_until > valid_from AND owner_person_id <> delegate_person_id)
);
CREATE UNIQUE INDEX IF NOT EXISTS collaboration_review_delegation_current
    ON qintopia_agent_os.collaboration_review_delegations(tenant_key,scope_id,owner_person_id)
    WHERE revoked_at IS NULL;
INSERT INTO qintopia_agent_os.schema_change_log
    (schema_version,migration_name,summary,design_doc_path,metadata)
VALUES ('2026-09-23.009','202609230009_steward_review_delegation.sql',
    'Scoped temporary content review delegation with current resident and source authority revalidation.',
    'docs/data-design/2026-09-23-steward-review-delegation.md','{"external_effects":false}')
ON CONFLICT(schema_version) DO NOTHING;
