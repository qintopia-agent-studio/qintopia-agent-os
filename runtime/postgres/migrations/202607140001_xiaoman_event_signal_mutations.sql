-- Design: runtime/postgres/docs/data-design/2026-07-14-xiaoman-event-signal-mutations.md
ALTER TABLE qintopia_agent_os.event_signals
    ADD COLUMN IF NOT EXISTS gap_summary text NOT NULL DEFAULT '';

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'event_signals_gap_summary_length'
          AND conrelid = 'qintopia_agent_os.event_signals'::regclass
    ) THEN
        ALTER TABLE qintopia_agent_os.event_signals
            ADD CONSTRAINT event_signals_gap_summary_length
            CHECK (char_length(gap_summary) <= 500);
    END IF;
END $$;

CREATE TABLE IF NOT EXISTS qintopia_agent_os.event_signal_mutations (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    event_signal_id uuid NOT NULL REFERENCES qintopia_agent_os.event_signals(id) ON DELETE CASCADE,
    mutation_id uuid NOT NULL UNIQUE,
    idempotency_key text NOT NULL UNIQUE,
    operation text NOT NULL CHECK (operation IN ('status-update', 'gap-update')),
    actor_agent text NOT NULL CHECK (actor_agent = 'xiaoman'),
    previous_value jsonb NOT NULL,
    new_value jsonb NOT NULL,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS event_signal_mutations_signal_created_idx
    ON qintopia_agent_os.event_signal_mutations (event_signal_id, created_at DESC);

INSERT INTO qintopia_agent_os.schema_change_log
    (schema_version, migration_name, summary, design_doc_path, metadata)
VALUES
    (
        '2026-07-13.002',
        '202607130002_huabaosi_image_generation.sql',
        'Registers the guarded Huabaosi image-generation capability without enabling an external provider, media upload, publication, or sending.',
        'docs/data-design/2026-07-13-huabaosi-image-generation.md',
        '{"change_type":"additive","domain":"huabaosi_image_generation","recorded_by":"202607140001_xiaoman_event_signal_mutations.sql","external_provider_default_enabled":false}'::jsonb
    )
ON CONFLICT (schema_version) DO NOTHING;

INSERT INTO qintopia_agent_os.schema_change_log
    (schema_version, migration_name, summary, design_doc_path, metadata)
VALUES
    (
        '2026-07-14.001',
        '202607140001_xiaoman_event_signal_mutations.sql',
        'Adds Xiaoman event-signal status and gap mutation support with explicit idempotency and append-only AgentOS audit rows.',
        'docs/data-design/2026-07-14-xiaoman-event-signal-mutations.md',
        '{"change_type":"additive","domain":"event_signals","fact_source":"postgres","feishu_writeback":false,"external_sends":false}'::jsonb
    )
ON CONFLICT (schema_version) DO UPDATE SET
    migration_name = EXCLUDED.migration_name,
    status = 'applied',
    summary = EXCLUDED.summary,
    design_doc_path = EXCLUDED.design_doc_path,
    metadata = EXCLUDED.metadata,
    applied_at = now();
