-- Design: runtime/postgres/docs/data-design/2026-07-14-qiwe-image-send-state.md
CREATE TABLE IF NOT EXISTS qintopia_agent_os.qiwe_image_send_attempts (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    work_item_id uuid NOT NULL REFERENCES qintopia_agent_os.work_items(id) ON DELETE RESTRICT,
    generated_image_artifact_id uuid NOT NULL REFERENCES qintopia_agent_os.artifacts(id) ON DELETE RESTRICT,
    attempt_number integer NOT NULL,
    status text NOT NULL,
    claim_token text NOT NULL,
    request_id_sha256 text NOT NULL UNIQUE,
    callback_payload_sha256 text UNIQUE,
    target_group_sha256 text NOT NULL,
    artifact_content_hash text NOT NULL,
    artifact_uri_sha256 text NOT NULL,
    artifact_file_md5 text NOT NULL,
    artifact_byte_size bigint NOT NULL,
    provider_message_id_sha256 text,
    failure_code text,
    audit_metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    callback_received_at timestamptz,
    send_started_at timestamptz,
    completed_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT qiwe_image_send_attempt_number_check CHECK (attempt_number > 0),
    CONSTRAINT qiwe_image_send_status_check CHECK (
        status IN ('awaiting_callback', 'sending', 'sent', 'failed', 'ambiguous', 'expired')
    ),
    CONSTRAINT qiwe_image_send_claim_token_check CHECK (
        claim_token LIKE 'qiwe-image-send-adapter:%' AND length(claim_token) <= 128
    ),
    CONSTRAINT qiwe_image_send_request_hash_check CHECK (
        request_id_sha256 ~ '^sha256:[0-9a-f]{64}$'
    ),
    CONSTRAINT qiwe_image_send_callback_hash_check CHECK (
        callback_payload_sha256 IS NULL
        OR callback_payload_sha256 ~ '^sha256:[0-9a-f]{64}$'
    ),
    CONSTRAINT qiwe_image_send_target_hash_check CHECK (
        target_group_sha256 ~ '^sha256:[0-9a-f]{64}$'
    ),
    CONSTRAINT qiwe_image_send_content_hash_check CHECK (
        artifact_content_hash ~ '^sha256:[0-9a-f]{64}$'
    ),
    CONSTRAINT qiwe_image_send_uri_hash_check CHECK (
        artifact_uri_sha256 ~ '^sha256:[0-9a-f]{64}$'
    ),
    CONSTRAINT qiwe_image_send_file_md5_check CHECK (
        artifact_file_md5 ~ '^[0-9a-f]{32}$'
    ),
    CONSTRAINT qiwe_image_send_artifact_byte_size_check CHECK (
        artifact_byte_size > 0
    ),
    CONSTRAINT qiwe_image_send_message_hash_check CHECK (
        provider_message_id_sha256 IS NULL
        OR provider_message_id_sha256 ~ '^sha256:[0-9a-f]{64}$'
    ),
    CONSTRAINT qiwe_image_send_failure_code_check CHECK (
        failure_code IS NULL
        OR failure_code IN (
            'callback_invalid',
            'claim_expired',
            'policy_changed',
            'send_rejected',
            'send_outcome_ambiguous'
        )
    ),
    CONSTRAINT qiwe_image_send_audit_metadata_object_check CHECK (
        jsonb_typeof(audit_metadata) = 'object'
    ),
    UNIQUE (work_item_id, attempt_number)
);

CREATE INDEX IF NOT EXISTS qiwe_image_send_attempts_work_item_idx
    ON qintopia_agent_os.qiwe_image_send_attempts (work_item_id, created_at DESC);

CREATE INDEX IF NOT EXISTS qiwe_image_send_attempts_status_idx
    ON qintopia_agent_os.qiwe_image_send_attempts (status, created_at)
    WHERE status IN ('awaiting_callback', 'sending');

CREATE UNIQUE INDEX IF NOT EXISTS qiwe_image_send_attempts_one_active_idx
    ON qintopia_agent_os.qiwe_image_send_attempts (work_item_id)
    WHERE status IN ('awaiting_callback', 'sending');

CREATE UNIQUE INDEX IF NOT EXISTS qiwe_image_send_attempts_one_sent_idx
    ON qintopia_agent_os.qiwe_image_send_attempts (work_item_id)
    WHERE status = 'sent';

INSERT INTO qintopia_agent_os.schema_change_log
    (schema_version, migration_name, summary, design_doc_path, metadata)
VALUES
    (
        '2026-07-14.002',
        '202607140002_qiwe_image_send_state.sql',
        'Adds durable hashed QiWe image-upload correlation, callback idempotency, claim validation, and sanitized send audit state.',
        'docs/data-design/2026-07-14-qiwe-image-send-state.md',
        '{"change_type":"additive","domain":"qiwe_image_send","external_send_enabled":false,"callback_credentials_persisted":false}'::jsonb
    )
ON CONFLICT (schema_version) DO UPDATE SET
    migration_name = EXCLUDED.migration_name,
    status = 'applied',
    summary = EXCLUDED.summary,
    design_doc_path = EXCLUDED.design_doc_path,
    metadata = EXCLUDED.metadata,
    applied_at = now();
