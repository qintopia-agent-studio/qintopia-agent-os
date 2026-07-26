SET search_path TO qintopia_messages, public;

ALTER TABLE qintopia_agent_os.event_signal_mutations
    DROP CONSTRAINT IF EXISTS event_signal_mutations_operation_check;

ALTER TABLE qintopia_agent_os.event_signal_mutations
    ADD CONSTRAINT event_signal_mutations_operation_check
    CHECK (operation IN (
        'status-update',
        'gap-update',
        'phase-update',
        'promotion-details-update'
    ));

INSERT INTO qintopia_agent_os.schema_change_log
    (schema_version, migration_name, summary, design_doc_path, metadata)
VALUES
    (
        '2026-07-15.002',
        '202607150002_xiaoman_promotion_details.sql',
        'Adds an atomic, idempotent Xiaoman promotion-details mutation using existing event-signal owner and metadata fields.',
        'docs/data-design/2026-07-15-xiaoman-promotion-details.md',
        '{"change_type":"additive","domain":"xiaoman_activity_promotion","fact_source":"postgres","feishu_writeback":false,"external_send":false}'::jsonb
    )
ON CONFLICT (schema_version) DO UPDATE SET
    migration_name = EXCLUDED.migration_name,
    status = 'applied',
    summary = EXCLUDED.summary,
    design_doc_path = EXCLUDED.design_doc_path,
    metadata = EXCLUDED.metadata,
    applied_at = now();
