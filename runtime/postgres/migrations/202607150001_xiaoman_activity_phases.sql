-- Design: runtime/postgres/docs/data-design/2026-07-15-xiaoman-activity-phases.md
ALTER TABLE qintopia_agent_os.event_signals
    ADD COLUMN IF NOT EXISTS activity_phase text;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'event_signals_activity_phase_check'
          AND conrelid = 'qintopia_agent_os.event_signals'::regclass
    ) THEN
        ALTER TABLE qintopia_agent_os.event_signals
            ADD CONSTRAINT event_signals_activity_phase_check
            CHECK (
                activity_phase IS NULL
                OR activity_phase IN ('pre_event', 'in_event', 'post_event')
            );
    END IF;
END $$;

UPDATE qintopia_agent_os.event_signals
SET activity_phase = 'pre_event',
    updated_at = now()
WHERE owner_agent = 'xiaoman'
  AND signal_type = '活动/聚会'
  AND activity_phase IS NULL;

CREATE INDEX IF NOT EXISTS event_signals_xiaoman_activity_phase_idx
    ON qintopia_agent_os.event_signals
        (activity_phase, status, signal_date, created_at)
    WHERE owner_agent = 'xiaoman'
      AND signal_type = '活动/聚会';

ALTER TABLE qintopia_agent_os.event_signal_mutations
    DROP CONSTRAINT IF EXISTS event_signal_mutations_operation_check;

ALTER TABLE qintopia_agent_os.event_signal_mutations
    ADD CONSTRAINT event_signal_mutations_operation_check
    CHECK (operation IN ('status-update', 'gap-update', 'phase-update'));

UPDATE qintopia_agent_os.capabilities
SET allowed_work_item_types = ARRAY[
        'activity_promotion_request',
        'activity_live_support_request',
        'activity_recap_request'
    ]::text[],
    input_schema = '{
        "required": ["brief_summary"],
        "properties": {
            "brief_summary": {"type": "string"},
            "activity_phase": {
                "type": "string",
                "enum": ["pre_event", "in_event", "post_event"]
            },
            "source_refs": {"type": "object"}
        }
    }'::jsonb,
    output_schema = '{
        "work_item_types": [
            "activity_promotion_request",
            "activity_live_support_request",
            "activity_recap_request"
        ]
    }'::jsonb,
    metadata = metadata || '{
        "activity_phase_fact_source": "event_signals",
        "activity_route_policy": "fixed_allowlist"
    }'::jsonb,
    updated_at = now()
WHERE capability_key = 'xiaoman.create_activity_request';

INSERT INTO qintopia_agent_os.schema_change_log
    (schema_version, migration_name, summary, design_doc_path, metadata)
VALUES
    (
        '2026-07-15.001',
        '202607150001_xiaoman_activity_phases.sql',
        'Adds audited Xiaoman activity lifecycle phases and phase-specific internal work-item routing.',
        'docs/data-design/2026-07-15-xiaoman-activity-phases.md',
        '{"change_type":"additive","domain":"xiaoman_activity_lifecycle","fact_source":"postgres","external_sends":false,"new_timers":false}'::jsonb
    )
ON CONFLICT (schema_version) DO UPDATE SET
    migration_name = EXCLUDED.migration_name,
    status = 'applied',
    summary = EXCLUDED.summary,
    design_doc_path = EXCLUDED.design_doc_path,
    metadata = EXCLUDED.metadata,
    applied_at = now();
