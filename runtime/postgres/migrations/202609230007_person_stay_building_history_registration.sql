-- Design: docs/data-design/2026-09-23-ontology-audience.md
-- Additive repair: preserve applied migration 006; make the capture installation replay-safe.
CREATE TABLE IF NOT EXISTS qintopia_identity.person_stay_building_history (
    source_instance text NOT NULL,
    property_id text NOT NULL,
    stay_id text NOT NULL,
    occupant_id text NOT NULL,
    building_code text NOT NULL,
    order_id text NOT NULL,
    first_in_house_observed_at timestamptz,
    last_in_house_observed_at timestamptz,
    evidence_kind text NOT NULL DEFAULT 'in_house_observation'
        CHECK(evidence_kind IN ('in_house_observation','legacy_last_building')),
    source_revision numeric NOT NULL,
    PRIMARY KEY(source_instance,property_id,stay_id,occupant_id,building_code),
    FOREIGN KEY(source_instance,property_id)
        REFERENCES qintopia_agent_os.welcome_sources(source_instance,property_id)
);

-- Legacy rows prove the recorded last building only, never an inferred earlier one.
INSERT INTO qintopia_identity.person_stay_building_history
(source_instance,property_id,stay_id,occupant_id,building_code,order_id,
 first_in_house_observed_at,last_in_house_observed_at,source_revision,evidence_kind)
SELECT source_instance,property_id,stay_id,occupant_id,last_building,order_id,
       NULL,NULL,source_revision,'legacy_last_building'
FROM qintopia_identity.person_stay_history WHERE last_building <> ''
ON CONFLICT(source_instance,property_id,stay_id,occupant_id,building_code) DO NOTHING;

CREATE OR REPLACE FUNCTION qintopia_identity.capture_person_stay_building_history()
RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE occupant jsonb; observed timestamptz;
BEGIN
    IF NEW.aggregate_type <> 'order' OR NEW.invalidated OR NEW.conflicted
       OR NEW.projection->>'state' IS DISTINCT FROM 'InHouse'
       OR NEW.projection->>'current_arrangement' IS DISTINCT FROM 'true'
       OR COALESCE(NEW.projection->>'stay','') = ''
       OR COALESCE(NEW.projection->>'building','') = ''
       OR NOT EXISTS (SELECT 1 FROM qintopia_agent_os.welcome_sources s
           WHERE s.source_instance=NEW.source_instance AND s.property_id=NEW.property_id
             AND s.enabled AND NOT s.rebuilding) THEN
        RETURN NEW;
    END IF;
    observed := (NEW.projection->>'observed_at')::timestamptz;
    IF observed IS NULL OR observed > clock_timestamp()+interval '5 seconds' THEN RETURN NEW; END IF;
    FOR occupant IN SELECT value FROM jsonb_array_elements(NEW.projection->'occupants') LOOP
        IF occupant->>'active'='true' AND COALESCE(occupant->>'id','') <> '' THEN
            INSERT INTO qintopia_identity.person_stay_building_history
            (source_instance,property_id,stay_id,occupant_id,building_code,order_id,
             first_in_house_observed_at,last_in_house_observed_at,source_revision)
            VALUES(NEW.source_instance,NEW.property_id,NEW.projection->>'stay',occupant->>'id',
                   NEW.projection->>'building',NEW.aggregate_id,observed,observed,NEW.revision)
            ON CONFLICT(source_instance,property_id,stay_id,occupant_id,building_code) DO UPDATE
            SET first_in_house_observed_at=COALESCE(person_stay_building_history.first_in_house_observed_at,EXCLUDED.first_in_house_observed_at),
                last_in_house_observed_at=EXCLUDED.last_in_house_observed_at,
                source_revision=EXCLUDED.source_revision,evidence_kind='in_house_observation'
            WHERE (person_stay_building_history.last_in_house_observed_at IS NULL OR EXCLUDED.last_in_house_observed_at >= person_stay_building_history.last_in_house_observed_at)
              AND EXCLUDED.source_revision >= person_stay_building_history.source_revision;
        END IF;
    END LOOP;
    RETURN NEW;
END $$;
DROP TRIGGER IF EXISTS person_stay_building_history_capture
ON qintopia_agent_os.welcome_source_versions;
CREATE TRIGGER person_stay_building_history_capture AFTER INSERT OR UPDATE
ON qintopia_agent_os.welcome_source_versions FOR EACH ROW
EXECUTE FUNCTION qintopia_identity.capture_person_stay_building_history();

INSERT INTO qintopia_agent_os.schema_change_log
    (schema_version,migration_name,summary,design_doc_path,metadata)
VALUES
    ('2026-09-23.006','202609230006_person_stay_building_history.sql',
     'Preserve observed occupancy by building for tenant-scoped dynamic collaboration audiences; current membership is re-evaluated from existing PMS projections.',
     'docs/data-design/2026-09-23-ontology-audience.md','{"external_effects":false,"legacy_backfill":"recorded_last_building_only"}'),
    ('2026-09-23.007','202609230007_person_stay_building_history_registration.sql',
     'Register the applied building-history migration and install its capture function and trigger idempotently without rewriting the applied checksum.',
     'docs/data-design/2026-09-23-ontology-audience.md','{"external_effects":false,"repairs":"202609230006","preserves_applied_checksum":true}')
ON CONFLICT(schema_version) DO NOTHING;
