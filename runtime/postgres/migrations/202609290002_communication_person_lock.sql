-- Design: runtime/postgres/docs/data-design/2026-09-29-communication-person-lock.md
CREATE FUNCTION qintopia_identity.management_ui_lock_communication_person(
    p_tenant text, p_scope uuid, p_link uuid, p_person uuid
) RETURNS boolean
LANGUAGE plpgsql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE matched_rows bigint;
BEGIN
    PERFORM l.id
    FROM qintopia_identity.source_identity_links l
    JOIN qintopia_identity.person_identity_gateways g
      ON g.namespace = l.namespace AND g.subject_type = l.subject_type
    JOIN qintopia_identity.persons p ON p.id = l.person_id
    JOIN qintopia_agent_os.collaboration_scopes s
      ON s.id = g.scope_id AND s.tenant_key = g.tenant_key
    WHERE g.tenant_key = p_tenant AND g.scope_id = p_scope
      AND l.id = p_link AND p.id = p_person
      AND g.active AND s.status = 'active' AND p.status = 'active'
      AND g.account_kind IN ('personal','employee')
      AND g.subject_type IN ('wecom_internal','qiwe_sender')
      AND l.status = 'confirmed' AND l.evidence_ref IS NOT NULL
      AND (l.confirmed_by IS NOT NULL OR l.confirmed_by_work_account IS NOT NULL)
      AND l.adapter_metadata ? 'first_observation_ref'
      AND (SELECT count(*) FROM qintopia_identity.person_identity_gateways x
           WHERE x.namespace = g.namespace AND x.subject_type = g.subject_type
             AND x.active) = 1
    FOR SHARE OF l, g, p, s;
    GET DIAGNOSTICS matched_rows = ROW_COUNT;
    RETURN matched_rows = 1;
END;
$$;
REVOKE ALL ON FUNCTION qintopia_identity.management_ui_lock_communication_person(text, uuid, uuid, uuid) FROM PUBLIC;

INSERT INTO qintopia_agent_os.schema_change_log
    (schema_version,migration_name,summary,design_doc_path,metadata)
VALUES
    ('2026-09-29.002','202609290002_communication_person_lock.sql',
     'Add a fixed restricted-UI lock for current cross-namespace communication contacts.',
     'runtime/postgres/docs/data-design/2026-09-29-communication-person-lock.md',
     '{"external_effects":false,"bootstrap_grants":false,"management_ui_enabled":false}'::jsonb)
ON CONFLICT (schema_version) DO NOTHING;
