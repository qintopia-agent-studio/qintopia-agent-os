-- Local executors are only availability declarations, never production activation.
ALTER TABLE qintopia_agent_os.collaboration_local_executors
    DROP CONSTRAINT IF EXISTS collaboration_local_executors_agent_key_check;
ALTER TABLE qintopia_agent_os.collaboration_local_executors
    ADD CONSTRAINT collaboration_local_executors_agent_key_check
    CHECK (agent_key IN ('erhua','anan','huabaosi'));

INSERT INTO qintopia_agent_os.schema_change_log
    (schema_version,migration_name,summary,design_doc_path,metadata)
VALUES ('2026-09-22.004','202609220004_welcome_local_executors.sql',
    'Allow the registered local welcome coordinator and renderer availability declarations',
    'docs/data-design/2026-09-22-foundation-welcome.md',
    '{"change_type":"additive","external_execution_enabled":false}'::jsonb)
ON CONFLICT (schema_version) DO NOTHING;
