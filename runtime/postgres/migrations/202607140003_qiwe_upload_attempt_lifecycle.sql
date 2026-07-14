-- Design: runtime/postgres/docs/data-design/2026-07-14-qiwe-upload-attempt-lifecycle.md
ALTER TABLE qintopia_agent_os.qiwe_image_send_attempts
    ALTER COLUMN request_id_sha256 DROP NOT NULL;

ALTER TABLE qintopia_agent_os.qiwe_image_send_attempts
    DROP CONSTRAINT IF EXISTS qiwe_image_send_status_check,
    DROP CONSTRAINT IF EXISTS qiwe_image_send_request_hash_check,
    DROP CONSTRAINT IF EXISTS qiwe_image_send_failure_code_check,
    DROP CONSTRAINT IF EXISTS qiwe_image_send_request_lifecycle_check;

ALTER TABLE qintopia_agent_os.qiwe_image_send_attempts
    ADD CONSTRAINT qiwe_image_send_status_check CHECK (
        status IN (
            'uploading',
            'awaiting_callback',
            'sending',
            'sent',
            'failed',
            'ambiguous',
            'expired'
        )
    ),
    ADD CONSTRAINT qiwe_image_send_request_hash_check CHECK (
        request_id_sha256 IS NULL
        OR request_id_sha256 ~ '^sha256:[0-9a-f]{64}$'
    ),
    ADD CONSTRAINT qiwe_image_send_failure_code_check CHECK (
        failure_code IS NULL
        OR failure_code IN (
            'callback_invalid',
            'claim_expired',
            'policy_changed',
            'qiwe_upload_rejected',
            'qiwe_upload_outcome_ambiguous',
            'send_rejected',
            'send_outcome_ambiguous'
        )
    ),
    ADD CONSTRAINT qiwe_image_send_request_lifecycle_check CHECK (
        status NOT IN ('awaiting_callback', 'sending', 'sent', 'expired')
        OR request_id_sha256 IS NOT NULL
    );

DROP INDEX IF EXISTS qintopia_agent_os.qiwe_image_send_attempts_status_idx;
CREATE INDEX qiwe_image_send_attempts_status_idx
    ON qintopia_agent_os.qiwe_image_send_attempts (status, created_at)
    WHERE status IN ('uploading', 'awaiting_callback', 'sending');

DROP INDEX IF EXISTS qintopia_agent_os.qiwe_image_send_attempts_one_active_idx;
CREATE UNIQUE INDEX qiwe_image_send_attempts_one_active_idx
    ON qintopia_agent_os.qiwe_image_send_attempts (work_item_id)
    WHERE status IN ('uploading', 'awaiting_callback', 'sending');

INSERT INTO qintopia_agent_os.schema_change_log
    (schema_version, migration_name, summary, design_doc_path, metadata)
VALUES
    (
        '2026-07-14.003',
        '202607140003_qiwe_upload_attempt_lifecycle.sql',
        'Persists a QiWe uploading attempt before external I/O and makes uncertain upload recovery terminal.',
        'docs/data-design/2026-07-14-qiwe-upload-attempt-lifecycle.md',
        '{"change_type":"additive","domain":"qiwe_image_send","automatic_unknown_upload_retry":false,"external_send_enabled":false}'::jsonb
    )
ON CONFLICT (schema_version) DO UPDATE SET
    migration_name = EXCLUDED.migration_name,
    status = 'applied',
    summary = EXCLUDED.summary,
    design_doc_path = EXCLUDED.design_doc_path,
    metadata = EXCLUDED.metadata,
    applied_at = now();
