-- Design: docs/data-design/2026-09-22-person-foundation-consumers.md
ALTER TABLE qintopia_agent_os.collaboration_turn_sources
    ADD COLUMN IF NOT EXISTS command_hash text,
    ADD COLUMN IF NOT EXISTS command jsonb;
INSERT INTO qintopia_agent_os.schema_change_log(schema_version,migration_name,summary,design_doc_path,metadata)
VALUES ('2026-09-22.005','202609220005_foundation_turn_input.sql','Freeze synthetic conversation typed input for acknowledgement-loss replay without reinterpreting versions.',
'docs/data-design/2026-09-22-person-foundation-consumers.md','{"external_effects":false}')
ON CONFLICT(schema_version) DO NOTHING;
