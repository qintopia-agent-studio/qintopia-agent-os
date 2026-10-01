-- Design: runtime/postgres/docs/data-design/2026-09-29-scope-communication.md
-- Empty configuration has no general staff-group destination or contact authority.
ALTER TABLE qintopia_agent_os.collaboration_scopes
    ADD COLUMN IF NOT EXISTS communication_config jsonb NOT NULL DEFAULT '{}'::jsonb
    CHECK (jsonb_typeof(communication_config) = 'object');

INSERT INTO qintopia_agent_os.schema_change_log
    (schema_version,migration_name,summary,design_doc_path,metadata)
VALUES
    ('2026-09-29.001','202609290001_scope_communication.sql',
     'Add an empty-by-default scope communication choice without granting send authority.',
     'runtime/postgres/docs/data-design/2026-09-29-scope-communication.md',
     '{"external_effects":false}'::jsonb)
ON CONFLICT (schema_version) DO NOTHING;
