-- Design: runtime/postgres/docs/data-design/2026-09-28-management-ui-lock-capabilities.md
-- Fixed lock capabilities for the restricted management UI; owner/EXECUTE grants
-- are prepared separately before that UI is enabled.
CREATE OR REPLACE FUNCTION qintopia_identity.management_ui_lock_actor(
    p_tenant text, p_link uuid
) RETURNS boolean
LANGUAGE plpgsql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE matched_rows bigint;
BEGIN
    PERFORM l.id
    FROM qintopia_agent_os.collaboration_tenants t
    JOIN qintopia_identity.source_identity_links l
      ON l.namespace = t.identity_namespace
    JOIN qintopia_identity.persons p ON p.id = l.person_id
    WHERE t.tenant_key = p_tenant AND t.mode = 'live'
      AND l.id = p_link AND l.evidence_ref IS NOT NULL
    FOR SHARE OF l, p;
    GET DIAGNOSTICS matched_rows = ROW_COUNT;
    RETURN matched_rows > 0;
END;
$$;
REVOKE ALL ON FUNCTION qintopia_identity.management_ui_lock_actor(text, uuid) FROM PUBLIC;

CREATE OR REPLACE FUNCTION qintopia_identity.management_ui_lock_gateway_registry()
RETURNS void
LANGUAGE plpgsql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
BEGIN
    LOCK TABLE qintopia_identity.person_identity_gateways IN SHARE MODE;
END;
$$;
REVOKE ALL ON FUNCTION qintopia_identity.management_ui_lock_gateway_registry() FROM PUBLIC;

CREATE OR REPLACE FUNCTION qintopia_identity.management_ui_lock_identity_candidate(
    p_tenant text, p_link uuid
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
    JOIN qintopia_agent_os.collaboration_scopes s
      ON s.id = g.scope_id AND s.tenant_key = g.tenant_key
    WHERE g.tenant_key = p_tenant AND l.id = p_link
    FOR UPDATE OF l FOR SHARE OF g, s;
    GET DIAGNOSTICS matched_rows = ROW_COUNT;
    RETURN matched_rows > 0;
END;
$$;
REVOKE ALL ON FUNCTION qintopia_identity.management_ui_lock_identity_candidate(text, uuid) FROM PUBLIC;

CREATE OR REPLACE FUNCTION qintopia_identity.management_ui_lock_business_source(
    p_tenant text, p_gateway text, p_link uuid
) RETURNS boolean
LANGUAGE plpgsql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE matched_rows bigint;
BEGIN
    PERFORM l.id
    FROM qintopia_identity.person_identity_gateways g
    JOIN qintopia_identity.source_identity_links l
      ON l.namespace = g.namespace AND l.subject_type = g.subject_type
    JOIN qintopia_agent_os.collaboration_scopes s
      ON s.id = g.scope_id AND s.tenant_key = g.tenant_key
    WHERE g.tenant_key = p_tenant AND g.gateway_key = p_gateway AND l.id = p_link
      AND g.active AND g.account_kind = 'shared' AND s.status = 'active'
      AND l.person_id IS NULL AND l.status = 'pending'
      AND l.adapter_metadata ? 'first_observation_ref'
      AND (SELECT count(*) FROM qintopia_identity.person_identity_gateways x
           WHERE x.namespace = g.namespace AND x.subject_type = g.subject_type
             AND x.active) = 1
    FOR SHARE OF g, l, s;
    GET DIAGNOSTICS matched_rows = ROW_COUNT;
    RETURN matched_rows > 0;
END;
$$;
REVOKE ALL ON FUNCTION qintopia_identity.management_ui_lock_business_source(text, text, uuid) FROM PUBLIC;

CREATE OR REPLACE FUNCTION qintopia_identity.management_ui_lock_business_account(
    p_tenant text, p_account uuid
) RETURNS boolean
LANGUAGE plpgsql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE matched_rows bigint;
BEGIN
    PERFORM w.id
    FROM qintopia_identity.work_accounts w
    JOIN qintopia_identity.person_identity_gateways g
      ON g.tenant_key = w.tenant_key AND g.gateway_key = w.gateway_key
    JOIN qintopia_identity.source_identity_links l ON l.id = w.source_link_id
    WHERE w.tenant_key = p_tenant AND w.id = p_account AND w.active
      AND g.active AND g.account_kind = 'shared' AND l.person_id IS NULL
      AND l.status <> 'revoked' AND l.adapter_metadata ? 'first_observation_ref'
    FOR SHARE OF w, g, l;
    GET DIAGNOSTICS matched_rows = ROW_COUNT;
    RETURN matched_rows > 0;
END;
$$;
REVOKE ALL ON FUNCTION qintopia_identity.management_ui_lock_business_account(text, uuid) FROM PUBLIC;

INSERT INTO qintopia_agent_os.schema_change_log
    (schema_version, migration_name, summary, design_doc_path, metadata)
VALUES
    ('2026-09-28.001', '202609280001_management_ui_lock_capabilities.sql',
     'Add fixed lock functions for restricted collaboration management UI without changing identity data or grants.',
     'runtime/postgres/docs/data-design/2026-09-28-management-ui-lock-capabilities.md',
     '{"external_effects":false,"bootstrap_grants":false,"management_ui_enabled":false}')
ON CONFLICT (schema_version) DO NOTHING;
