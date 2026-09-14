-- Synthetic local UI bootstrap only. Never apply to a live database.
DO $$ BEGIN
    IF current_database() <> 'qintopia_test' THEN
        RAISE EXCEPTION 'isolated qintopia_test required';
    END IF;
END $$;

INSERT INTO qintopia_agent_os.welcome_sources
    (source_instance,property_id,mode,rebuilding,enabled,admission_after,execution_epoch)
VALUES ('synthetic-workbench','fixture-property','synthetic',false,true,now(),1)
ON CONFLICT DO NOTHING;

INSERT INTO qintopia_identity.persons(id,display_name)
VALUES ('a1000000-0000-4000-8000-000000000001','合成审核员')
ON CONFLICT DO NOTHING;

INSERT INTO qintopia_identity.source_identity_links
    (id,namespace,subject_type,source_ref,person_id,status,evidence_ref,confirmed_by)
VALUES ('a1000000-0000-4000-8000-000000000002','synthetic-workbench/feishu',
    'feishu_open','synthetic-operator','a1000000-0000-4000-8000-000000000001',
    'confirmed','a1000000-0000-4000-8000-000000000003','a1000000-0000-4000-8000-000000000001')
ON CONFLICT DO NOTHING;

INSERT INTO qintopia_agent_os.welcome_identity_scopes(namespace,source_instance,property_id)
VALUES ('synthetic-workbench/feishu','synthetic-workbench','fixture-property')
ON CONFLICT DO NOTHING;

INSERT INTO qintopia_agent_os.welcome_grants
    (id,person_id,source_instance,property_id,action,expires_at,appointed_by)
VALUES ('a1000000-0000-4000-8000-000000000004',
    'a1000000-0000-4000-8000-000000000001','synthetic-workbench','fixture-property',
    'identity',now()+interval '1 day','a1000000-0000-4000-8000-000000000001')
ON CONFLICT DO NOTHING;
