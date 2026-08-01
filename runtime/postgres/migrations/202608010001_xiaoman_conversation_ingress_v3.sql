BEGIN;

ALTER TABLE qintopia_messages.messages
    ADD COLUMN IF NOT EXISTS sender_type text;
ALTER TABLE qintopia_messages.messages
    ADD COLUMN IF NOT EXISTS thread_root_message_id text;
ALTER TABLE qintopia_messages.messages
    ADD COLUMN IF NOT EXISTS parent_message_id text;

UPDATE qintopia_messages.messages
SET sender_type = 'unknown'
WHERE sender_type IS NULL OR btrim(sender_type) = '';

ALTER TABLE qintopia_messages.messages
    ALTER COLUMN sender_type SET DEFAULT 'unknown';
ALTER TABLE qintopia_messages.messages
    ALTER COLUMN sender_type SET NOT NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'messages_sender_type_check'
          AND conrelid = 'qintopia_messages.messages'::regclass
    ) THEN
        ALTER TABLE qintopia_messages.messages
            ADD CONSTRAINT messages_sender_type_check
            CHECK (sender_type IN ('user', 'bot', 'system', 'unknown'));
    END IF;
END
$$;

CREATE INDEX IF NOT EXISTS messages_thread_root_idx
    ON qintopia_messages.messages (platform, chat_id, thread_root_message_id, sent_at DESC)
    WHERE thread_root_message_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS qintopia_agent_os.conversation_policies (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    platform text NOT NULL,
    conversation_ref text NOT NULL,
    conversation_type text NOT NULL,
    audience_class text NOT NULL,
    allowed_capabilities text[] NOT NULL DEFAULT ARRAY[]::text[],
    return_mode text NOT NULL,
    initiation_rule text NOT NULL,
    status_visibility text NOT NULL,
    policy_version bigint NOT NULL,
    policy_digest text NOT NULL,
    enabled boolean NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (platform, conversation_ref, policy_version),
    CONSTRAINT conversation_policies_platform_check CHECK (platform = 'feishu'),
    CONSTRAINT conversation_policies_ref_check CHECK (
        conversation_ref ~ '^sha256:[0-9a-f]{64}$'
    ),
    CONSTRAINT conversation_policies_type_check CHECK (
        conversation_type IN ('direct', 'group')
    ),
    CONSTRAINT conversation_policies_audience_check CHECK (
        audience_class IN ('private', 'internal_collaboration', 'external_community')
    ),
    CONSTRAINT conversation_policies_return_mode_check CHECK (
        return_mode IN ('direct_chat', 'thread_reply', 'none')
    ),
    CONSTRAINT conversation_policies_initiation_check CHECK (
        initiation_rule IN ('direct_message', 'explicit_bot_mention', 'disabled')
    ),
    CONSTRAINT conversation_policies_visibility_check CHECK (
        status_visibility IN ('requester', 'conversation_members', 'none')
    ),
    CONSTRAINT conversation_policies_version_check CHECK (policy_version > 0),
    CONSTRAINT conversation_policies_digest_check CHECK (
        policy_digest ~ '^sha256:[0-9a-f]{64}$'
    ),
    CONSTRAINT conversation_policies_semantics_check CHECK (
        (
            conversation_type = 'direct'
            AND audience_class = 'private'
            AND return_mode = 'direct_chat'
            AND initiation_rule = 'direct_message'
            AND status_visibility = 'requester'
        )
        OR
        (
            conversation_type = 'group'
            AND audience_class = 'internal_collaboration'
            AND return_mode = 'thread_reply'
            AND initiation_rule = 'explicit_bot_mention'
            AND status_visibility = 'conversation_members'
        )
        OR
        (
            audience_class = 'external_community'
            AND return_mode = 'none'
            AND initiation_rule = 'disabled'
            AND status_visibility = 'none'
        )
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS conversation_policies_one_active_idx
    ON qintopia_agent_os.conversation_policies (platform, conversation_ref)
    WHERE enabled;

CREATE INDEX IF NOT EXISTS conversation_policies_capabilities_idx
    ON qintopia_agent_os.conversation_policies USING gin (allowed_capabilities)
    WHERE enabled;

REVOKE ALL ON qintopia_agent_os.conversation_policies FROM PUBLIC;

CREATE TABLE IF NOT EXISTS qintopia_agent_os.conversation_policy_actors (
    policy_id uuid NOT NULL
        REFERENCES qintopia_agent_os.conversation_policies(id) ON DELETE CASCADE,
    actor_ref text NOT NULL,
    actor_role text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (policy_id, actor_ref, actor_role),
    CONSTRAINT conversation_policy_actors_ref_check CHECK (
        actor_ref ~ '^sha256:[0-9a-f]{64}$'
    ),
    CONSTRAINT conversation_policy_actors_role_check CHECK (actor_role = 'reviewer')
);

REVOKE ALL ON qintopia_agent_os.conversation_policy_actors FROM PUBLIC;

CREATE TABLE IF NOT EXISTS qintopia_agent_os.poster_workflow_participants (
    workflow_root_id uuid NOT NULL
        REFERENCES qintopia_agent_os.work_items(id) ON DELETE CASCADE,
    actor_ref text NOT NULL,
    participant_role text NOT NULL,
    conversation_ref text NOT NULL,
    policy_id uuid NOT NULL
        REFERENCES qintopia_agent_os.conversation_policies(id) ON DELETE RESTRICT,
    policy_version bigint NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (workflow_root_id, actor_ref, participant_role),
    CONSTRAINT poster_workflow_participants_actor_ref_check CHECK (
        actor_ref ~ '^sha256:[0-9a-f]{64}$'
    ),
    CONSTRAINT poster_workflow_participants_conversation_ref_check CHECK (
        conversation_ref ~ '^sha256:[0-9a-f]{64}$'
    ),
    CONSTRAINT poster_workflow_participants_role_check CHECK (
        participant_role IN ('requester', 'reviewer')
    ),
    CONSTRAINT poster_workflow_participants_version_check CHECK (policy_version > 0)
);

CREATE INDEX IF NOT EXISTS poster_workflow_participants_actor_idx
    ON qintopia_agent_os.poster_workflow_participants
        (conversation_ref, actor_ref, participant_role);

REVOKE ALL ON qintopia_agent_os.poster_workflow_participants FROM PUBLIC;

CREATE TABLE IF NOT EXISTS qintopia_agent_os.feishu_message_ingress_nonces (
    nonce_hash text PRIMARY KEY,
    payload_hash text NOT NULL,
    signed_at timestamptz NOT NULL,
    expires_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT feishu_message_ingress_nonces_nonce_hash_check CHECK (
        nonce_hash ~ '^sha256:[0-9a-f]{64}$'
    ),
    CONSTRAINT feishu_message_ingress_nonces_payload_hash_check CHECK (
        payload_hash ~ '^sha256:[0-9a-f]{64}$'
    ),
    CONSTRAINT feishu_message_ingress_nonces_expiry_check CHECK (expires_at > signed_at)
);

CREATE INDEX IF NOT EXISTS feishu_message_ingress_nonces_expiry_idx
    ON qintopia_agent_os.feishu_message_ingress_nonces (expires_at);

REVOKE ALL ON qintopia_agent_os.feishu_message_ingress_nonces FROM PUBLIC;

CREATE TABLE IF NOT EXISTS qintopia_agent_os.feishu_message_ingress_receipts (
    source_message_ref text PRIMARY KEY,
    message_row_id uuid NOT NULL UNIQUE
        REFERENCES qintopia_messages.messages(id) ON DELETE CASCADE,
    conversation_ref text NOT NULL,
    policy_id uuid NOT NULL
        REFERENCES qintopia_agent_os.conversation_policies(id) ON DELETE RESTRICT,
    policy_version bigint NOT NULL,
    payload_hash text NOT NULL,
    first_received_at timestamptz NOT NULL DEFAULT now(),
    last_received_at timestamptz NOT NULL DEFAULT now(),
    duplicate_count integer NOT NULL DEFAULT 0,
    CONSTRAINT feishu_message_ingress_receipts_message_ref_check CHECK (
        source_message_ref ~ '^sha256:[0-9a-f]{64}$'
    ),
    CONSTRAINT feishu_message_ingress_receipts_conversation_ref_check CHECK (
        conversation_ref ~ '^sha256:[0-9a-f]{64}$'
    ),
    CONSTRAINT feishu_message_ingress_receipts_payload_hash_check CHECK (
        payload_hash ~ '^sha256:[0-9a-f]{64}$'
    ),
    CONSTRAINT feishu_message_ingress_receipts_version_check CHECK (policy_version > 0),
    CONSTRAINT feishu_message_ingress_receipts_duplicate_check CHECK (duplicate_count >= 0)
);

REVOKE ALL ON qintopia_agent_os.feishu_message_ingress_receipts FROM PUBLIC;

ALTER TABLE qintopia_agent_os.poster_return_targets
    ADD COLUMN IF NOT EXISTS audience_class text;
ALTER TABLE qintopia_agent_os.poster_return_targets
    ADD COLUMN IF NOT EXISTS conversation_ref text;
ALTER TABLE qintopia_agent_os.poster_return_targets
    ADD COLUMN IF NOT EXISTS policy_version bigint;
ALTER TABLE qintopia_agent_os.poster_return_targets
    ADD COLUMN IF NOT EXISTS delivery_mode text;
ALTER TABLE qintopia_agent_os.poster_return_targets
    ADD COLUMN IF NOT EXISTS thread_root_message_id text;

UPDATE qintopia_agent_os.poster_return_targets
SET audience_class = COALESCE(audience_class, 'private'),
    conversation_ref = COALESCE(conversation_ref, origin_ref),
    policy_version = COALESCE(policy_version, 0),
    delivery_mode = COALESCE(delivery_mode, 'direct_chat')
WHERE audience_class IS NULL
   OR conversation_ref IS NULL
   OR policy_version IS NULL
   OR delivery_mode IS NULL;

ALTER TABLE qintopia_agent_os.poster_return_targets
    ALTER COLUMN audience_class SET NOT NULL;
ALTER TABLE qintopia_agent_os.poster_return_targets
    ALTER COLUMN conversation_ref SET NOT NULL;
ALTER TABLE qintopia_agent_os.poster_return_targets
    ALTER COLUMN policy_version SET NOT NULL;
ALTER TABLE qintopia_agent_os.poster_return_targets
    ALTER COLUMN delivery_mode SET NOT NULL;

ALTER TABLE qintopia_agent_os.poster_return_targets
    DROP CONSTRAINT IF EXISTS poster_return_targets_conversation_type_check;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'poster_return_targets_conversation_type_check'
          AND conrelid = 'qintopia_agent_os.poster_return_targets'::regclass
    ) THEN
        ALTER TABLE qintopia_agent_os.poster_return_targets
            ADD CONSTRAINT poster_return_targets_conversation_type_check
            CHECK (conversation_type IN ('direct', 'group'));
    END IF;
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'poster_return_targets_audience_class_check'
          AND conrelid = 'qintopia_agent_os.poster_return_targets'::regclass
    ) THEN
        ALTER TABLE qintopia_agent_os.poster_return_targets
            ADD CONSTRAINT poster_return_targets_audience_class_check
            CHECK (audience_class IN ('private', 'internal_collaboration'));
    END IF;
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'poster_return_targets_delivery_mode_check'
          AND conrelid = 'qintopia_agent_os.poster_return_targets'::regclass
    ) THEN
        ALTER TABLE qintopia_agent_os.poster_return_targets
            ADD CONSTRAINT poster_return_targets_delivery_mode_check
            CHECK (delivery_mode IN ('direct_chat', 'thread_reply'));
    END IF;
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'poster_return_targets_policy_version_check'
          AND conrelid = 'qintopia_agent_os.poster_return_targets'::regclass
    ) THEN
        ALTER TABLE qintopia_agent_os.poster_return_targets
            ADD CONSTRAINT poster_return_targets_policy_version_check
            CHECK (policy_version >= 0);
    END IF;
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'poster_return_targets_delivery_semantics_check'
          AND conrelid = 'qintopia_agent_os.poster_return_targets'::regclass
    ) THEN
        ALTER TABLE qintopia_agent_os.poster_return_targets
            ADD CONSTRAINT poster_return_targets_delivery_semantics_check
            CHECK (
                (
                    conversation_type = 'direct'
                    AND audience_class = 'private'
                    AND delivery_mode = 'direct_chat'
                    AND thread_root_message_id IS NULL
                )
                OR
                (
                    conversation_type = 'group'
                    AND audience_class = 'internal_collaboration'
                    AND delivery_mode = 'thread_reply'
                    AND NULLIF(btrim(thread_root_message_id), '') IS NOT NULL
                    AND policy_version > 0
                )
            );
    END IF;
END
$$;

CREATE INDEX IF NOT EXISTS poster_return_targets_conversation_ref_idx
    ON qintopia_agent_os.poster_return_targets
        (platform, conversation_ref, policy_version);

INSERT INTO qintopia_agent_os.schema_change_log
    (schema_version, migration_name, summary, design_doc_path, metadata)
VALUES
    (
        '2026-08-01.001',
        '202608010001_xiaoman_conversation_ingress_v3.sql',
        'Adds authenticated Feishu message ingress policy, replay receipts, workflow participants, message thread fields, and group-capable poster target metadata.',
        'docs/data-design/2026-08-01-xiaoman-conversation-ingress-v3.md',
        '{"change_type":"additive","domain":"xiaoman_conversation_ingress","fact_source":"postgres","hermes_fork":false,"internal_group_enabled":false,"automatic_group_send":false}'::jsonb
    )
ON CONFLICT (schema_version) DO UPDATE SET
    migration_name = EXCLUDED.migration_name,
    status = 'applied',
    summary = EXCLUDED.summary,
    design_doc_path = EXCLUDED.design_doc_path,
    metadata = EXCLUDED.metadata,
    applied_at = now();

COMMIT;
