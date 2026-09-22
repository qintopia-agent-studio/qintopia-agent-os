-- Additive local foundation; no real accounts, grants or external source activation.
CREATE TABLE IF NOT EXISTS qintopia_identity.person_identity_gateways (
    tenant_key text NOT NULL REFERENCES qintopia_agent_os.collaboration_tenants(tenant_key),
    gateway_key text NOT NULL,
    namespace text NOT NULL,
    subject_type text NOT NULL CHECK(subject_type IN ('pms_member','pms_occupant','wecom_external','wecom_internal','qiwe_sender','feishu_open')),
    scope_id uuid NOT NULL REFERENCES qintopia_agent_os.collaboration_scopes(id),
    building_code text,
    account_kind text NOT NULL CHECK(account_kind IN ('personal','employee','shared')),
    active boolean NOT NULL DEFAULT false,
    version bigint NOT NULL DEFAULT 1,
    PRIMARY KEY(tenant_key,gateway_key)
);
ALTER TABLE qintopia_identity.source_identity_links
    ADD COLUMN IF NOT EXISTS adapter_metadata jsonb NOT NULL DEFAULT '{}'::jsonb;
CREATE TABLE IF NOT EXISTS qintopia_identity.person_memory_state (
    person_id uuid PRIMARY KEY REFERENCES qintopia_identity.persons(id),
    version bigint NOT NULL DEFAULT 0,
    status text NOT NULL DEFAULT 'active' CHECK(status IN ('active','stopped')),
    observed_through timestamptz,
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE IF NOT EXISTS qintopia_identity.person_memory_commands (
    operation_id uuid PRIMARY KEY,
    person_id uuid NOT NULL REFERENCES qintopia_identity.persons(id),
    source_link_id uuid NOT NULL REFERENCES qintopia_identity.source_identity_links(id),
    message_ref uuid NOT NULL,
    observed_at timestamptz NOT NULL,
    request_hash text NOT NULL,
    result jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE(person_id,message_ref)
);
ALTER TABLE qintopia_identity.member_facts
    ADD COLUMN IF NOT EXISTS fact_value jsonb,
    ADD COLUMN IF NOT EXISTS fact_condition text,
    ADD COLUMN IF NOT EXISTS fact_version bigint,
    ADD COLUMN IF NOT EXISTS fact_status text,
    ADD COLUMN IF NOT EXISTS use_purpose text,
    ADD COLUMN IF NOT EXISTS use_audience text;
CREATE UNIQUE INDEX IF NOT EXISTS person_reply_preference_current
    ON qintopia_identity.member_facts(person_id,fact_condition)
    WHERE fact_type='reply_preference' AND fact_status='active';

CREATE TABLE IF NOT EXISTS qintopia_identity.person_stay_history (
    source_instance text NOT NULL,
    property_id text NOT NULL,
    stay_id text NOT NULL,
    occupant_id text NOT NULL,
    order_id text NOT NULL,
    first_in_house_observed_at timestamptz NOT NULL,
    last_observed_at timestamptz NOT NULL,
    last_state text NOT NULL,
    last_building text NOT NULL,
    source_revision numeric NOT NULL,
    PRIMARY KEY(source_instance,property_id,stay_id,occupant_id),
    FOREIGN KEY(source_instance,property_id) REFERENCES qintopia_agent_os.welcome_sources(source_instance,property_id)
);
CREATE OR REPLACE FUNCTION qintopia_identity.capture_person_stay_history() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE occupant jsonb; observed timestamptz;
BEGIN
    IF NEW.aggregate_type <> 'order' OR NEW.invalidated OR NEW.conflicted
       OR NOT (NEW.projection ? 'stay' AND NEW.projection ? 'state' AND NEW.projection ? 'observed_at') THEN
        RETURN NEW;
    END IF;
    observed := (NEW.projection->>'observed_at')::timestamptz;
    -- Only an actual in-house observation establishes a visit; a reservation or
    -- cancellation never creates one. Existing stays retain their first evidence.
    FOR occupant IN SELECT value FROM jsonb_array_elements(NEW.projection->'occupants') LOOP
        IF NEW.projection->>'state'='InHouse' AND occupant->>'active'='true' THEN
            INSERT INTO qintopia_identity.person_stay_history
              (source_instance,property_id,stay_id,occupant_id,order_id,first_in_house_observed_at,last_observed_at,last_state,last_building,source_revision)
            VALUES(NEW.source_instance,NEW.property_id,NEW.projection->>'stay',occupant->>'id',NEW.aggregate_id,observed,observed,'InHouse',NEW.projection->>'building',NEW.revision)
            ON CONFLICT(source_instance,property_id,stay_id,occupant_id) DO UPDATE
              SET last_observed_at=EXCLUDED.last_observed_at,last_state=EXCLUDED.last_state,last_building=EXCLUDED.last_building,source_revision=EXCLUDED.source_revision
              WHERE EXCLUDED.last_observed_at >= qintopia_identity.person_stay_history.last_observed_at
                AND EXCLUDED.source_revision >= qintopia_identity.person_stay_history.source_revision;
        END IF;
    END LOOP;
    UPDATE qintopia_identity.person_stay_history h SET
      last_observed_at=observed,last_state=CASE WHEN NEW.projection->>'stay'=h.stay_id
        AND NEW.projection->>'state'='InHouse' AND EXISTS(SELECT 1 FROM jsonb_array_elements(NEW.projection->'occupants') o WHERE o->>'id'=h.occupant_id AND o->>'active'='true')
        THEN 'InHouse' ELSE 'Terminated' END,source_revision=NEW.revision
      WHERE h.source_instance=NEW.source_instance AND h.property_id=NEW.property_id
        AND h.order_id=NEW.aggregate_id AND observed>=h.last_observed_at AND NEW.revision>=h.source_revision;
    RETURN NEW;
END $$;
DROP TRIGGER IF EXISTS person_stay_history_capture ON qintopia_agent_os.welcome_source_versions;
CREATE TRIGGER person_stay_history_capture AFTER INSERT OR UPDATE
 ON qintopia_agent_os.welcome_source_versions FOR EACH ROW EXECUTE FUNCTION qintopia_identity.capture_person_stay_history();

INSERT INTO qintopia_agent_os.schema_change_log(schema_version,migration_name,summary,design_doc_path,metadata)
VALUES('2026-09-22.002','202609220002_person_memory.sql',
 'Add controlled gateway identity bindings, versioned self reply preference and observed PMS stay history; no live seeds.',
 'docs/data-design/2026-09-22-person-memory.md','{"external_effects":false}')
ON CONFLICT(schema_version) DO NOTHING;
