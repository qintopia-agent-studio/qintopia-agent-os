CREATE SCHEMA IF NOT EXISTS qintopia_identity;
CREATE SCHEMA IF NOT EXISTS qintopia_agent_os;

CREATE TABLE IF NOT EXISTS qintopia_identity.erhua_training_notes (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    caller_profile text NOT NULL DEFAULT 'erhua',
    platform text NOT NULL DEFAULT 'qiwe',
    chat_id text NOT NULL DEFAULT '',
    trainer_user_id text NOT NULL,
    target_channel_user_id text NOT NULL DEFAULT '',
    target_member_name text NOT NULL DEFAULT '',
    target_person_id uuid REFERENCES qintopia_identity.persons(id) ON DELETE SET NULL,
    training_type text NOT NULL,
    training_text text NOT NULL,
    sanitized_summary text NOT NULL,
    status text NOT NULL DEFAULT 'pending',
    risk_level text NOT NULL DEFAULT 'low',
    reason text NOT NULL DEFAULT '',
    source_message_id uuid REFERENCES qintopia_messages.messages(id) ON DELETE SET NULL,
    source_platform_message_id text NOT NULL DEFAULT '',
    applied_member_fact_id uuid REFERENCES qintopia_identity.member_facts(id) ON DELETE SET NULL,
    applied_profile_snapshot_id uuid REFERENCES qintopia_identity.member_profile_snapshots(id) ON DELETE SET NULL,
    applied_persona_overlay_id uuid,
    reviewed_by text,
    reviewed_at timestamptz,
    revoked_at timestamptz,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT erhua_training_notes_training_type_check CHECK (
        training_type IN ('member_preference', 'member_fact', 'reply_example', 'persona_rule')
    ),
    CONSTRAINT erhua_training_notes_status_check CHECK (
        status IN ('pending', 'active', 'rejected', 'revoked')
    ),
    CONSTRAINT erhua_training_notes_risk_level_check CHECK (
        risk_level IN ('low', 'medium', 'high')
    )
);

CREATE INDEX IF NOT EXISTS erhua_training_notes_status_idx
    ON qintopia_identity.erhua_training_notes (status, training_type, created_at DESC);

CREATE INDEX IF NOT EXISTS erhua_training_notes_trainer_idx
    ON qintopia_identity.erhua_training_notes (trainer_user_id, created_at DESC);

CREATE INDEX IF NOT EXISTS erhua_training_notes_target_idx
    ON qintopia_identity.erhua_training_notes (target_person_id, target_channel_user_id, created_at DESC);

ALTER TABLE qintopia_identity.erhua_training_notes
    ADD COLUMN IF NOT EXISTS applied_persona_overlay_id uuid;

CREATE TABLE IF NOT EXISTS qintopia_identity.erhua_persona_overlays (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    training_note_id uuid REFERENCES qintopia_identity.erhua_training_notes(id) ON DELETE SET NULL,
    overlay_text text NOT NULL,
    status text NOT NULL DEFAULT 'pending',
    risk_level text NOT NULL DEFAULT 'medium',
    priority integer NOT NULL DEFAULT 100,
    valid_from timestamptz NOT NULL DEFAULT now(),
    valid_until timestamptz,
    reviewed_by text,
    reviewed_at timestamptz,
    revoked_at timestamptz,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT erhua_persona_overlays_status_check CHECK (
        status IN ('pending', 'active', 'rejected', 'revoked')
    ),
    CONSTRAINT erhua_persona_overlays_risk_level_check CHECK (
        risk_level IN ('low', 'medium', 'high')
    )
);

CREATE INDEX IF NOT EXISTS erhua_persona_overlays_active_idx
    ON qintopia_identity.erhua_persona_overlays (status, priority, created_at DESC)
    WHERE status = 'active' AND revoked_at IS NULL;

ALTER TABLE qintopia_identity.erhua_training_notes
    DROP CONSTRAINT IF EXISTS erhua_training_notes_applied_persona_overlay_id_fkey;

ALTER TABLE qintopia_identity.erhua_training_notes
    ADD CONSTRAINT erhua_training_notes_applied_persona_overlay_id_fkey
    FOREIGN KEY (applied_persona_overlay_id)
    REFERENCES qintopia_identity.erhua_persona_overlays(id)
    ON DELETE SET NULL;

INSERT INTO qintopia_agent_os.schema_change_log
    (schema_version, migration_name, summary, design_doc_path, metadata)
VALUES
    (
        '2026-06-29.006',
        '202606290006_erhua_training_memory.sql',
        'Adds controlled Erhua trainer memory tables for audited trainer-submitted notes and active persona overlays.',
        'docs/data-design/2026-06-29-erhua-training-memory.md',
        '{"change_type":"additive","domain":"erhua_training_memory"}'::jsonb
    )
ON CONFLICT (schema_version) DO UPDATE SET
    migration_name = EXCLUDED.migration_name,
    status = 'applied',
    summary = EXCLUDED.summary,
    design_doc_path = EXCLUDED.design_doc_path,
    metadata = EXCLUDED.metadata,
    applied_at = now();
