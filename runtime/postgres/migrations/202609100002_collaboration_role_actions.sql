-- Role choices narrow new assignments; they do not create execution grants.
ALTER TABLE qintopia_agent_os.collaboration_roles
    ADD COLUMN available_actions text[] NOT NULL DEFAULT '{}'
    CHECK (available_actions <@ ARRAY['confirm_knowledge','train','change_rules',
      'review','publish','designate','identity','manage','technical_support']::text[]);

-- Upgrade only the explicitly named local demo. No live/shadow title-based policy.
UPDATE qintopia_agent_os.collaboration_roles r
SET available_actions = CASE WHEN r.label = '技术负责人'
    THEN ARRAY['technical_support']
    ELSE ARRAY['confirm_knowledge','train','change_rules','review','publish','designate','identity','manage'] END
FROM qintopia_agent_os.collaboration_tenants t
WHERE t.tenant_key = r.tenant_key AND t.mode = 'synthetic'
  AND t.tenant_key = 'synthetic-collaboration-ui-v1'
  AND r.label IN ('公司负责人','社区负责人','舍长','小管家','活动运营','技术负责人');

INSERT INTO qintopia_agent_os.schema_change_log
    (schema_version,migration_name,summary,design_doc_path,metadata)
VALUES ('2026-09-10.002','202609100002_collaboration_role_actions.sql',
    'Persist available role duties and reject incompatible new assignments; no grants created.',
    'runtime/postgres/docs/data-design/2026-09-10-person-agent-collaboration-v1.md',
    '{"external_effects":false,"bootstrap_grants":false}')
ON CONFLICT (schema_version) DO NOTHING;
