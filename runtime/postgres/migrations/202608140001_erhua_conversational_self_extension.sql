-- Design: docs/data-design/2026-08-14-erhua-conversational-self-extension.md
CREATE SCHEMA IF NOT EXISTS qintopia_agent_os;

ALTER TABLE qintopia_messages.raw_events
    ADD COLUMN IF NOT EXISTS space_id uuid
        REFERENCES qintopia_messages.conversations(id) ON DELETE SET NULL;

ALTER TABLE qintopia_messages.raw_events
    ADD COLUMN IF NOT EXISTS ingress_auth_verified boolean NOT NULL DEFAULT false;

ALTER TABLE qintopia_agent_os.work_items
    ADD COLUMN IF NOT EXISTS space_id uuid
        REFERENCES qintopia_messages.conversations(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS raw_events_space_received_idx
    ON qintopia_messages.raw_events (space_id, received_at DESC)
    WHERE space_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS work_items_space_status_created_idx
    ON qintopia_agent_os.work_items (space_id, status, created_at DESC)
    WHERE space_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS qintopia_agent_os.space_policy_versions (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    space_id uuid NOT NULL
        REFERENCES qintopia_messages.conversations(id) ON DELETE CASCADE,
    definition_key text NOT NULL,
    version integer NOT NULL CHECK (version > 0),
    policy_config jsonb NOT NULL DEFAULT '{}'::jsonb,
    status text NOT NULL DEFAULT 'draft',
    definition_digest text NOT NULL,
    created_by_person_id uuid NOT NULL
        REFERENCES qintopia_identity.persons(id) ON DELETE RESTRICT,
    created_from_work_item_id uuid
        REFERENCES qintopia_agent_os.work_items(id) ON DELETE SET NULL,
    activated_at timestamptz,
    retired_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT space_policy_versions_definition_key_check CHECK (
        definition_key ~ '^[a-z][a-z0-9_.-]{0,119}$'
    ),
    CONSTRAINT space_policy_versions_config_object_check CHECK (
        jsonb_typeof(policy_config) = 'object'
    ),
    CONSTRAINT space_policy_versions_status_check CHECK (
        status IN ('draft', 'shadow', 'active', 'paused', 'retired')
    ),
    CONSTRAINT space_policy_versions_digest_check CHECK (
        definition_digest ~ '^[0-9a-f]{64}$'
    ),
    UNIQUE (space_id, definition_key, version)
);

CREATE UNIQUE INDEX IF NOT EXISTS space_policy_versions_one_active_idx
    ON qintopia_agent_os.space_policy_versions (space_id, definition_key)
    WHERE status = 'active';

CREATE INDEX IF NOT EXISTS space_policy_versions_digest_idx
    ON qintopia_agent_os.space_policy_versions
        (space_id, definition_key, definition_digest);

CREATE TABLE IF NOT EXISTS qintopia_agent_os.business_definition_versions (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    space_id uuid NOT NULL
        REFERENCES qintopia_messages.conversations(id) ON DELETE CASCADE,
    definition_key text NOT NULL,
    version integer NOT NULL CHECK (version > 0),
    execution_mode text NOT NULL,
    definition jsonb NOT NULL,
    allowed_capabilities text[] NOT NULL DEFAULT ARRAY[]::text[],
    approval_policy text NOT NULL DEFAULT 'space_admin_confirmation',
    status text NOT NULL DEFAULT 'draft',
    definition_digest text NOT NULL,
    created_by_person_id uuid NOT NULL
        REFERENCES qintopia_identity.persons(id) ON DELETE RESTRICT,
    created_from_work_item_id uuid
        REFERENCES qintopia_agent_os.work_items(id) ON DELETE SET NULL,
    activated_at timestamptz,
    retired_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT business_definition_versions_definition_key_check CHECK (
        definition_key ~ '^[a-z][a-z0-9_.-]{0,119}$'
    ),
    CONSTRAINT business_definition_versions_execution_mode_check CHECK (
        execution_mode IN ('deterministic', 'agent_turn')
    ),
    CONSTRAINT business_definition_versions_definition_object_check CHECK (
        jsonb_typeof(definition) = 'object'
    ),
    CONSTRAINT business_definition_versions_approval_policy_check CHECK (
        approval_policy IN (
            'none',
            'space_admin_confirmation',
            'before_external_use',
            'human_final_confirmation'
        )
    ),
    CONSTRAINT business_definition_versions_status_check CHECK (
        status IN ('draft', 'shadow', 'active', 'paused', 'retired')
    ),
    CONSTRAINT business_definition_versions_digest_check CHECK (
        definition_digest ~ '^[0-9a-f]{64}$'
    ),
    UNIQUE (space_id, definition_key, version),
    CONSTRAINT business_definition_versions_id_space_unique
        UNIQUE (id, space_id)
);

DO $migration$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'business_definition_versions_id_space_unique'
          AND conrelid = 'qintopia_agent_os.business_definition_versions'::regclass
    ) THEN
        ALTER TABLE qintopia_agent_os.business_definition_versions
            ADD CONSTRAINT business_definition_versions_id_space_unique
            UNIQUE (id, space_id);
    END IF;
END
$migration$;

CREATE UNIQUE INDEX IF NOT EXISTS business_definition_versions_one_active_idx
    ON qintopia_agent_os.business_definition_versions (space_id, definition_key)
    WHERE status = 'active';

CREATE INDEX IF NOT EXISTS business_definition_versions_digest_idx
    ON qintopia_agent_os.business_definition_versions
        (space_id, definition_key, definition_digest);

CREATE TABLE IF NOT EXISTS qintopia_agent_os.channel_event_mapping_versions (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    provider text NOT NULL,
    definition_key text NOT NULL,
    version integer NOT NULL CHECK (version > 0),
    selector jsonb NOT NULL,
    extractor jsonb NOT NULL,
    official_sources jsonb NOT NULL DEFAULT '[]'::jsonb,
    validation_evidence jsonb NOT NULL DEFAULT '{}'::jsonb,
    status text NOT NULL DEFAULT 'draft',
    definition_digest text NOT NULL,
    created_by_person_id uuid NOT NULL
        REFERENCES qintopia_identity.persons(id) ON DELETE RESTRICT,
    created_from_work_item_id uuid
        REFERENCES qintopia_agent_os.work_items(id) ON DELETE SET NULL,
    activated_at timestamptz,
    retired_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT channel_event_mapping_versions_provider_check CHECK (
        provider ~ '^[a-z][a-z0-9_.-]{0,63}$'
    ),
    CONSTRAINT channel_event_mapping_versions_definition_key_check CHECK (
        definition_key ~ '^[a-z][a-z0-9_.-]{0,119}$'
    ),
    CONSTRAINT channel_event_mapping_versions_selector_object_check CHECK (
        jsonb_typeof(selector) = 'object'
    ),
    CONSTRAINT channel_event_mapping_versions_extractor_object_check CHECK (
        jsonb_typeof(extractor) = 'object'
    ),
    CONSTRAINT channel_event_mapping_versions_sources_array_check CHECK (
        jsonb_typeof(official_sources) = 'array'
    ),
    CONSTRAINT channel_event_mapping_versions_evidence_object_check CHECK (
        jsonb_typeof(validation_evidence) = 'object'
    ),
    CONSTRAINT channel_event_mapping_versions_status_check CHECK (
        status IN ('draft', 'shadow', 'active', 'paused', 'retired')
    ),
    CONSTRAINT channel_event_mapping_versions_digest_check CHECK (
        definition_digest ~ '^[0-9a-f]{64}$'
    ),
    UNIQUE (provider, definition_key, version)
);

CREATE UNIQUE INDEX IF NOT EXISTS channel_event_mapping_versions_one_active_idx
    ON qintopia_agent_os.channel_event_mapping_versions (provider, definition_key)
    WHERE status = 'active';

CREATE INDEX IF NOT EXISTS channel_event_mapping_versions_digest_idx
    ON qintopia_agent_os.channel_event_mapping_versions
        (provider, definition_key, definition_digest);

CREATE TABLE IF NOT EXISTS qintopia_agent_os.automation_definition_versions (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    space_id uuid NOT NULL
        REFERENCES qintopia_messages.conversations(id) ON DELETE CASCADE,
    definition_key text NOT NULL,
    version integer NOT NULL CHECK (version > 0),
    business_definition_id uuid NOT NULL
        REFERENCES qintopia_agent_os.business_definition_versions(id) ON DELETE RESTRICT,
    channel_event_mapping_id uuid
        REFERENCES qintopia_agent_os.channel_event_mapping_versions(id) ON DELETE RESTRICT,
    trigger_kind text NOT NULL,
    trigger_config jsonb NOT NULL,
    timezone text NOT NULL DEFAULT 'Asia/Shanghai',
    misfire_policy text NOT NULL DEFAULT 'run_once',
    status text NOT NULL DEFAULT 'draft',
    next_run_at timestamptz,
    last_dispatched_at timestamptz,
    definition_digest text NOT NULL,
    created_by_person_id uuid NOT NULL
        REFERENCES qintopia_identity.persons(id) ON DELETE RESTRICT,
    created_from_work_item_id uuid
        REFERENCES qintopia_agent_os.work_items(id) ON DELETE SET NULL,
    activated_at timestamptz,
    retired_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT automation_definition_versions_definition_key_check CHECK (
        definition_key ~ '^[a-z][a-z0-9_.-]{0,119}$'
    ),
    CONSTRAINT automation_definition_versions_trigger_kind_check CHECK (
        trigger_kind IN ('event', 'schedule')
    ),
    CONSTRAINT automation_definition_versions_trigger_object_check CHECK (
        jsonb_typeof(trigger_config) = 'object'
    ),
    CONSTRAINT automation_definition_versions_timezone_check CHECK (
        char_length(timezone) BETWEEN 1 AND 64
    ),
    CONSTRAINT automation_definition_versions_misfire_policy_check CHECK (
        misfire_policy = 'run_once'
    ),
    CONSTRAINT automation_definition_versions_status_check CHECK (
        status IN ('draft', 'shadow', 'active', 'paused', 'retired')
    ),
    CONSTRAINT automation_definition_versions_digest_check CHECK (
        definition_digest ~ '^[0-9a-f]{64}$'
    ),
    CONSTRAINT automation_definition_versions_event_mapping_check CHECK (
        (trigger_kind = 'event' AND channel_event_mapping_id IS NOT NULL)
        OR (trigger_kind = 'schedule' AND channel_event_mapping_id IS NULL)
    ),
    CONSTRAINT automation_definition_versions_business_space_fk
        FOREIGN KEY (business_definition_id, space_id)
        REFERENCES qintopia_agent_os.business_definition_versions(id, space_id)
        ON DELETE RESTRICT,
    UNIQUE (space_id, definition_key, version)
);

DO $migration$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'automation_definition_versions_business_space_fk'
          AND conrelid = 'qintopia_agent_os.automation_definition_versions'::regclass
    ) THEN
        ALTER TABLE qintopia_agent_os.automation_definition_versions
            ADD CONSTRAINT automation_definition_versions_business_space_fk
            FOREIGN KEY (business_definition_id, space_id)
            REFERENCES qintopia_agent_os.business_definition_versions(id, space_id)
            ON DELETE RESTRICT;
    END IF;
END
$migration$;

CREATE UNIQUE INDEX IF NOT EXISTS automation_definition_versions_one_active_idx
    ON qintopia_agent_os.automation_definition_versions (space_id, definition_key)
    WHERE status = 'active';

CREATE INDEX IF NOT EXISTS automation_definition_versions_dispatch_idx
    ON qintopia_agent_os.automation_definition_versions (next_run_at, id)
    WHERE status = 'active'
      AND trigger_kind = 'schedule'
      AND next_run_at IS NOT NULL;

CREATE INDEX IF NOT EXISTS automation_definition_versions_event_idx
    ON qintopia_agent_os.automation_definition_versions
        (space_id, channel_event_mapping_id)
    WHERE status IN ('shadow', 'active')
      AND trigger_kind = 'event';

CREATE INDEX IF NOT EXISTS automation_definition_versions_digest_idx
    ON qintopia_agent_os.automation_definition_versions
        (space_id, definition_key, definition_digest);

INSERT INTO qintopia_agent_os.capabilities
    (
        capability_key,
        provider_agent,
        display_name,
        description,
        allowed_callers,
        allowed_work_item_types,
        risk_level,
        review_policy,
        input_schema,
        output_schema,
        enabled,
        metadata
    )
VALUES
    (
        'erhua.manage_space_configuration',
        'erhua',
        'Erhua Space configuration proposal',
        'Prepare and confirm versioned Space policies, business definitions, automations, and reusable provider event mappings through a trusted conversation boundary.',
        ARRAY['erhua']::text[],
        ARRAY['space_change_request', 'space_programming_extension_request']::text[],
        'high',
        'space_admin_confirmation',
        '{"required":["intent"],"properties":{"intent":{"type":"object"}}}'::jsonb,
        '{"artifact_types":["space_change_proposal"],"external_send":false}'::jsonb,
        false,
        '{"space_scoped":true,"trusted_session_required":true,"external_send":false,"default_automation_state":"inactive"}'::jsonb
    ),
    (
        'erhua.execute_space_business',
        'erhua',
        'Erhua Space business execution',
        'Execute one active, version-bound Space business definition from a trusted event or schedule work item.',
        ARRAY['erhua', 'system']::text[],
        ARRAY['space_automation_run', 'space_event_shadow_observation']::text[],
        'high',
        'definition_policy',
        '{"required":["business_definition_id","automation_definition_id"],"properties":{"business_definition_id":{"type":"string"},"automation_definition_id":{"type":"string"}}}'::jsonb,
        '{"external_send":"requires_definition_and_capability_policy"}'::jsonb,
        false,
        '{"space_scoped":true,"definition_bound":true,"external_send_default":false,"enablement":"owner_review_required"}'::jsonb
    ),
    (
        'erhua.qiwe_text_template',
        'erhua',
        'Erhua Space QiWe text template',
        'Render one confirmed Space-owned text template, verify canonical event subjects against the exact current QiWe room roster, and send once to the room derived from the Space.',
        ARRAY['system']::text[],
        ARRAY['space_automation_run']::text[],
        'high',
        'space_admin_confirmation',
        '{"required":["text_template"],"properties":{"text_template":{"type":"string"},"subject_name_separator":{"type":"string"}}}'::jsonb,
        '{"external_send":true,"target":"derived_from_space","ambiguous_retry":false}'::jsonb,
        false,
        '{"space_scoped":true,"space_invocable":true,"space_scope_binding":"work_item_space_id","invocation_boundary":"erhua.execute_space_business","direct_chat_invocation":false,"roster_verification":"exact_current_room","enablement":"owner_review_required"}'::jsonb
    ),
    (
        'erhua.space_agent_turn',
        'erhua',
        'Erhua constrained Space agent turn',
        'Create a version-bound internal Agent work item with a fixed output contract and the enabled capability intersection from the active Space policy.',
        ARRAY['system']::text[],
        ARRAY['space_agent_turn']::text[],
        'medium',
        'definition_policy',
        '{"required":["business_definition_id","automation_definition_id","output_contract"],"properties":{"business_definition_id":{"type":"string"},"automation_definition_id":{"type":"string"},"output_contract":{"type":"object"}}}'::jsonb,
        '{"external_send":false,"unrestricted_model_invocation":false,"fixed_output_contract":true}'::jsonb,
        false,
        '{"space_scoped":true,"space_invocable":true,"space_scope_binding":"work_item_space_id","invocation_boundary":"erhua.execute_space_business","direct_chat_invocation":false,"enablement":"owner_review_required"}'::jsonb
    ),
    (
        'erhua.qiwe_send_location_card',
        'erhua',
        'Erhua current-Space QiWe location card',
        'Allow the ordinary Erhua turn to invoke the registered QiWe location-card tool only from its trusted current Space.',
        ARRAY['erhua']::text[],
        ARRAY['qiwe_group_turn']::text[],
        'high',
        'space_policy',
        '{"type":"object"}'::jsonb,
        '{"type":"object"}'::jsonb,
        false,
        '{"space_scoped":true,"space_turn_invocable":true,"space_scope_binding":"trusted_session_space_id","invocation_boundary":"erhua.space_turn","external_send":true,"enablement":"owner_review_required"}'::jsonb
    ),
    (
        'erhua.qiwe_send_direct_message',
        'erhua',
        'Erhua current-Space QiWe direct message',
        'Allow the ordinary Erhua turn to invoke the registered QiWe direct-message tool only from its trusted current Space.',
        ARRAY['erhua']::text[],
        ARRAY['qiwe_group_turn']::text[],
        'high',
        'space_policy',
        '{"type":"object"}'::jsonb,
        '{"type":"object"}'::jsonb,
        false,
        '{"space_scoped":true,"space_turn_invocable":true,"space_scope_binding":"trusted_session_space_id","invocation_boundary":"erhua.space_turn","external_send":true,"enablement":"owner_review_required"}'::jsonb
    ),
    (
        'erhua.qiwe_send_rich_message',
        'erhua',
        'Erhua current-Space QiWe rich message',
        'Allow the ordinary Erhua turn to invoke the registered QiWe rich-message tool only from its trusted current Space.',
        ARRAY['erhua']::text[],
        ARRAY['qiwe_group_turn']::text[],
        'high',
        'space_policy',
        '{"type":"object"}'::jsonb,
        '{"type":"object"}'::jsonb,
        false,
        '{"space_scoped":true,"space_turn_invocable":true,"space_scope_binding":"trusted_session_space_id","invocation_boundary":"erhua.space_turn","external_send":true,"enablement":"owner_review_required"}'::jsonb
    ),
    (
        'erhua.qiwe_revoke_message',
        'erhua',
        'Erhua current-Space QiWe message revocation',
        'Allow the ordinary Erhua turn to invoke the registered QiWe message-revocation tool only from its trusted current Space.',
        ARRAY['erhua']::text[],
        ARRAY['qiwe_group_turn']::text[],
        'high',
        'space_policy',
        '{"type":"object"}'::jsonb,
        '{"type":"object"}'::jsonb,
        false,
        '{"space_scoped":true,"space_turn_invocable":true,"space_scope_binding":"trusted_session_space_id","invocation_boundary":"erhua.space_turn","external_send":true,"destructive_channel_mutation":true,"enablement":"owner_review_required"}'::jsonb
    ),
    (
        'erhua.qiwe_voice_to_text',
        'erhua',
        'Erhua current-Space QiWe voice transcription',
        'Allow the ordinary Erhua turn to invoke the registered QiWe voice-transcription tool only from its trusted current Space.',
        ARRAY['erhua']::text[],
        ARRAY['qiwe_group_turn']::text[],
        'medium',
        'space_policy',
        '{"type":"object"}'::jsonb,
        '{"type":"object"}'::jsonb,
        false,
        '{"space_scoped":true,"space_turn_invocable":true,"space_scope_binding":"trusted_session_space_id","invocation_boundary":"erhua.space_turn","external_send":false,"external_io":true,"enablement":"owner_review_required"}'::jsonb
    ),
    (
        'erhua.qiwe_handoff_to_human',
        'erhua',
        'Erhua current-Space QiWe human handoff',
        'Allow the ordinary Erhua turn to invoke the registered QiWe human-handoff tool only from its trusted current Space.',
        ARRAY['erhua']::text[],
        ARRAY['qiwe_group_turn']::text[],
        'high',
        'space_policy',
        '{"type":"object"}'::jsonb,
        '{"type":"object"}'::jsonb,
        false,
        '{"space_scoped":true,"space_turn_invocable":true,"space_scope_binding":"trusted_session_space_id","invocation_boundary":"erhua.space_turn","external_send":true,"enablement":"owner_review_required"}'::jsonb
    ),
    (
        'erhua.qiwe_request_direct_contact',
        'erhua',
        'Erhua current-Space QiWe direct-contact request',
        'Allow the ordinary Erhua turn to invoke the registered QiWe direct-contact tool only from its trusted current Space.',
        ARRAY['erhua']::text[],
        ARRAY['qiwe_group_turn']::text[],
        'high',
        'space_policy',
        '{"type":"object"}'::jsonb,
        '{"type":"object"}'::jsonb,
        false,
        '{"space_scoped":true,"space_turn_invocable":true,"space_scope_binding":"trusted_session_space_id","invocation_boundary":"erhua.space_turn","external_send":true,"contact_mutation":true,"enablement":"owner_review_required"}'::jsonb
    ),
    (
        'erhua.knowledge.public',
        'erhua',
        'Erhua current-Space public knowledge',
        'Allow an ordinary Erhua turn to use the registered public-knowledge category when its current Space grants it.',
        ARRAY['erhua']::text[],
        ARRAY['qiwe_group_turn']::text[],
        'low',
        'space_policy',
        '{"type":"object"}'::jsonb,
        '{"type":"object"}'::jsonb,
        false,
        '{"space_scoped":true,"space_turn_invocable":true,"space_scope_binding":"trusted_session_space_id","invocation_boundary":"erhua.space_turn","external_send":false,"enablement":"owner_review_required"}'::jsonb
    ),
    (
        'erhua.knowledge.community',
        'erhua',
        'Erhua current-Space community knowledge',
        'Allow an ordinary Erhua turn to use the registered current-community knowledge category when its current Space grants it.',
        ARRAY['erhua']::text[],
        ARRAY['qiwe_group_turn']::text[],
        'medium',
        'space_policy',
        '{"type":"object"}'::jsonb,
        '{"type":"object"}'::jsonb,
        false,
        '{"space_scoped":true,"space_turn_invocable":true,"space_scope_binding":"trusted_session_space_id","invocation_boundary":"erhua.space_turn","external_send":false,"knowledge_scope_enforced":false,"enablement":"owner_review_required"}'::jsonb
    ),
    (
        'erhua.workflow.complaint',
        'erhua',
        'Erhua current-Space complaint workflow',
        'Allow an ordinary Erhua turn to use the registered complaint-workflow category when its current Space grants it.',
        ARRAY['erhua']::text[],
        ARRAY['qiwe_group_turn']::text[],
        'medium',
        'space_policy',
        '{"type":"object"}'::jsonb,
        '{"type":"object"}'::jsonb,
        false,
        '{"space_scoped":true,"space_turn_invocable":true,"space_scope_binding":"trusted_session_space_id","invocation_boundary":"erhua.space_turn","external_send":false,"enablement":"owner_review_required"}'::jsonb
    ),
    (
        'erhua.workflow.sales',
        'erhua',
        'Erhua current-Space sales workflow',
        'Allow an ordinary Erhua turn to use the registered sales-workflow category when its current Space grants it.',
        ARRAY['erhua']::text[],
        ARRAY['qiwe_group_turn']::text[],
        'medium',
        'space_policy',
        '{"type":"object"}'::jsonb,
        '{"type":"object"}'::jsonb,
        false,
        '{"space_scoped":true,"space_turn_invocable":true,"space_scope_binding":"trusted_session_space_id","invocation_boundary":"erhua.space_turn","external_send":false,"enablement":"owner_review_required"}'::jsonb
    )
ON CONFLICT (capability_key) DO UPDATE SET
    provider_agent = EXCLUDED.provider_agent,
    display_name = EXCLUDED.display_name,
    description = EXCLUDED.description,
    allowed_callers = EXCLUDED.allowed_callers,
    allowed_work_item_types = EXCLUDED.allowed_work_item_types,
    risk_level = EXCLUDED.risk_level,
    review_policy = EXCLUDED.review_policy,
    input_schema = EXCLUDED.input_schema,
    output_schema = EXCLUDED.output_schema,
    enabled = EXCLUDED.enabled,
    metadata = EXCLUDED.metadata,
    updated_at = now();

INSERT INTO qintopia_agent_os.schema_change_log
    (schema_version, migration_name, summary, design_doc_path, metadata)
VALUES
    (
        '2026-08-14.001',
        '202608140001_erhua_conversational_self_extension.sql',
        'Adds Space links, versioned Space definitions, and default-disabled ordinary-turn capability policy for trusted conversational configuration.',
        'docs/data-design/2026-08-14-erhua-conversational-self-extension.md',
        '{"change_type":"additive","domain":"erhua_conversational_self_extension","external_send":false,"default_enabled_automations":0,"default_enabled_space_turn_capabilities":0}'::jsonb
    )
ON CONFLICT (schema_version) DO UPDATE SET
    migration_name = EXCLUDED.migration_name,
    status = 'applied',
    summary = EXCLUDED.summary,
    design_doc_path = EXCLUDED.design_doc_path,
    metadata = EXCLUDED.metadata,
    applied_at = now();
