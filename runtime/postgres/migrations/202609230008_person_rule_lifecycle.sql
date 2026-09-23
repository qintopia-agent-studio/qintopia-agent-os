-- Design: docs/data-design/2026-09-23-person-rule-lifecycle.md
ALTER TABLE qintopia_agent_os.collaboration_knowledge_items
    ADD COLUMN IF NOT EXISTS lifecycle_managed boolean NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS stopped_at timestamptz;

CREATE TABLE IF NOT EXISTS qintopia_agent_os.collaboration_rule_events (
    id uuid PRIMARY KEY,
    tenant_key text NOT NULL,
    scope_id uuid NOT NULL REFERENCES qintopia_agent_os.collaboration_scopes(id),
    knowledge_key text NOT NULL,
    operation_id uuid NOT NULL,
    actor_person_id uuid NOT NULL REFERENCES qintopia_identity.persons(id),
    action text NOT NULL CHECK (action IN ('save','stop','cancel_scheduled')),
    before_state jsonb NOT NULL,
    after_state jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    UNIQUE (tenant_key, operation_id)
);
CREATE INDEX IF NOT EXISTS collaboration_rule_events_scope_idx
    ON qintopia_agent_os.collaboration_rule_events(tenant_key, scope_id, created_at DESC);
INSERT INTO qintopia_agent_os.schema_change_log
    (schema_version,migration_name,summary,design_doc_path,metadata)
VALUES ('2026-09-23.008','202609230008_person_rule_lifecycle.sql',
    'Complete scoped text-rule lifecycle, expiry without fallback, explicit stop and retained audit.',
    'docs/data-design/2026-09-23-person-rule-lifecycle.md','{"external_effects":false}')
ON CONFLICT(schema_version) DO NOTHING;
