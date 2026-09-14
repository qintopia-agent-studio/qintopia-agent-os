-- Explicit duties and bounded permission modes; no live grants or external execution.
CREATE TABLE qintopia_agent_os.collaboration_duties (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_key text NOT NULL REFERENCES qintopia_agent_os.collaboration_tenants(tenant_key),
    label text NOT NULL CHECK (length(label) BETWEEN 1 AND 80),
    description text NOT NULL CHECK (length(description) BETWEEN 1 AND 2000),
    domain_key text NOT NULL CHECK (domain_key IN
      ('community_service','activity_operations','hospitality','technical_support','organization')),
    available_actions text[] NOT NULL DEFAULT '{}' CHECK (available_actions <@
      ARRAY['confirm_knowledge','train','change_rules','review','publish','designate','identity','manage','technical_support']::text[]),
    status text NOT NULL DEFAULT 'active' CHECK (status IN ('active','retired')),
    version bigint NOT NULL DEFAULT 1,
    UNIQUE (tenant_key,id),
    UNIQUE (tenant_key,label)
);
ALTER TABLE qintopia_agent_os.collaboration_roles
    ADD COLUMN description text NOT NULL DEFAULT '',
    ADD COLUMN status text NOT NULL DEFAULT 'active' CHECK (status IN ('active','retired')),
    ADD COLUMN version bigint NOT NULL DEFAULT 1;

CREATE TABLE qintopia_agent_os.collaboration_role_duties (
    tenant_key text NOT NULL,
    role_id uuid NOT NULL,
    duty_id uuid NOT NULL,
    PRIMARY KEY (tenant_key,role_id,duty_id),
    FOREIGN KEY (tenant_key,role_id) REFERENCES qintopia_agent_os.collaboration_roles(tenant_key,id),
    FOREIGN KEY (tenant_key,duty_id) REFERENCES qintopia_agent_os.collaboration_duties(tenant_key,id)
);

ALTER TABLE qintopia_agent_os.agent_collaborations
    ADD COLUMN duty_id uuid,
    ADD COLUMN replaces_id uuid,
    ADD FOREIGN KEY (tenant_key,duty_id) REFERENCES qintopia_agent_os.collaboration_duties(tenant_key,id),
    ADD FOREIGN KEY (tenant_key,replaces_id) REFERENCES qintopia_agent_os.agent_collaborations(tenant_key,id);
DROP INDEX qintopia_agent_os.agent_collaboration_current;
CREATE UNIQUE INDEX agent_collaboration_current
    ON qintopia_agent_os.agent_collaborations(tenant_key,appointment_id,agent_key,domain_key,
      COALESCE(duty_id,'00000000-0000-0000-0000-000000000000'::uuid)) WHERE status='active';

ALTER TABLE qintopia_agent_os.collaboration_grants
    ADD COLUMN decision_mode text NOT NULL DEFAULT 'autonomous'
      CHECK (decision_mode IN ('autonomous','confirmation','denied')),
    ADD COLUMN reviewer_person_id uuid REFERENCES qintopia_identity.persons(id),
    ADD CONSTRAINT collaboration_permission_reviewer CHECK
      ((decision_mode='confirmation' AND reviewer_person_id IS NOT NULL) OR
       (decision_mode<>'confirmation' AND reviewer_person_id IS NULL));

-- Only replace the demo's incorrect business label. Environment isolation stays unchanged.
UPDATE qintopia_agent_os.collaboration_scopes s SET label='秦托邦',version=s.version+1
FROM qintopia_agent_os.collaboration_tenants t
WHERE s.tenant_key=t.tenant_key AND t.mode='synthetic'
  AND t.tenant_key='synthetic-collaboration-ui-v1' AND s.parent_scope_id IS NULL AND s.label='合成社区';

UPDATE qintopia_agent_os.agent_collaborations
SET responsibility_text='管理秦托邦的本地测试配置',version=version+1
WHERE tenant_key='synthetic-collaboration-ui-v1' AND responsibility_text='管理合成社区配置';

-- The existing local UI receives definitions only, never automatic execution grants.
INSERT INTO qintopia_agent_os.collaboration_duties
    (tenant_key,label,description,domain_key,available_actions)
SELECT t.tenant_key,d.label,d.label || '的工作职责；实际权限在具体工作连接中确认。',d.domain_key,d.actions
FROM qintopia_agent_os.collaboration_tenants t CROSS JOIN (VALUES
    ('居民服务','community_service',ARRAY['confirm_knowledge','train','change_rules','review','publish','designate']),
    ('客房协调','hospitality',ARRAY['confirm_knowledge','train','change_rules','review','publish','identity']),
    ('活动运营','activity_operations',ARRAY['confirm_knowledge','train','change_rules','review','publish','designate']),
    ('技术支持','technical_support',ARRAY['technical_support','train']),
    ('组织管理','organization',ARRAY['manage','identity','review'])
) AS d(label,domain_key,actions)
WHERE t.tenant_key='synthetic-collaboration-ui-v1' AND t.mode='synthetic';

INSERT INTO qintopia_agent_os.collaboration_role_duties(tenant_key,role_id,duty_id)
SELECT r.tenant_key,r.id,d.id
FROM qintopia_agent_os.collaboration_roles r JOIN qintopia_agent_os.collaboration_duties d ON d.tenant_key=r.tenant_key
WHERE r.tenant_key='synthetic-collaboration-ui-v1' AND (
    (r.label IN ('公司负责人','社区负责人') AND d.domain_key<>'technical_support') OR
    (r.label='舍长' AND d.domain_key='community_service') OR
    (r.label='小管家' AND d.domain_key IN ('community_service','hospitality')) OR
    (r.label='活动运营' AND d.domain_key='activity_operations') OR
    (r.label='技术负责人' AND d.domain_key='technical_support')
);

INSERT INTO qintopia_agent_os.schema_change_log
    (schema_version,migration_name,summary,design_doc_path,metadata)
VALUES ('2026-09-10.003','202609100003_collaboration_duties_permissions.sql',
    'Add reusable duties, role associations and explicit autonomous/confirmation/denied permissions; no external activation.',
    'runtime/postgres/docs/data-design/2026-09-10-person-agent-collaboration-v1.md',
    '{"external_effects":false,"bootstrap_grants":false}')
ON CONFLICT (schema_version) DO NOTHING;
