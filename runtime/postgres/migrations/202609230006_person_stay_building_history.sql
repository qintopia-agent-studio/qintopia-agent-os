-- Preserve only observed occupancy by building; current membership is always re-evaluated.
CREATE TABLE qintopia_identity.person_stay_building_history (
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
FROM qintopia_identity.person_stay_history WHERE last_building <> '';

CREATE FUNCTION qintopia_identity.capture_person_stay_building_history()
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
CREATE TRIGGER person_stay_building_history_capture AFTER INSERT OR UPDATE
ON qintopia_agent_os.welcome_source_versions FOR EACH ROW
EXECUTE FUNCTION qintopia_identity.capture_person_stay_building_history();
