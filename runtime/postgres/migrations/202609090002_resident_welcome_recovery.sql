-- Separate additive slice; keep the already-tested V1 migration immutable.
-- Design: docs/data-design/2026-09-09-resident-welcome-v1.md
CREATE TABLE IF NOT EXISTS qintopia_agent_os.welcome_rebuilds (
    source_instance text NOT NULL,
    property_id text NOT NULL,
    generation uuid NOT NULL,
    head_cursor text NOT NULL,
    after_id text,
    scan_complete boolean NOT NULL DEFAULT false,
    PRIMARY KEY (source_instance, property_id),
    FOREIGN KEY (source_instance, property_id)
        REFERENCES qintopia_agent_os.welcome_sources(source_instance, property_id)
);
CREATE TABLE IF NOT EXISTS qintopia_agent_os.welcome_scan_items (
    source_instance text NOT NULL,
    property_id text NOT NULL,
    generation uuid NOT NULL,
    order_id text NOT NULL,
    projection jsonb NOT NULL,
    completed boolean NOT NULL DEFAULT false,
    PRIMARY KEY (source_instance, property_id, generation, order_id),
    FOREIGN KEY (source_instance, property_id)
        REFERENCES qintopia_agent_os.welcome_rebuilds(source_instance, property_id)
);
