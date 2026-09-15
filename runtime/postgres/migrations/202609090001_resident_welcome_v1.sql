-- Additive only. No grants, timers, production admission or external execution.
CREATE TABLE IF NOT EXISTS qintopia_agent_os.welcome_sources (
    source_instance text NOT NULL,
    property_id text NOT NULL,
    mode text NOT NULL CHECK (mode IN ('synthetic', 'shadow', 'live')),
    cursor text,
    rebuild_head text,
    rebuilding boolean NOT NULL DEFAULT true,
    admission_after timestamptz,
    execution_epoch bigint NOT NULL DEFAULT 0,
    enabled boolean NOT NULL DEFAULT false,
    PRIMARY KEY (source_instance, property_id)
);

CREATE TABLE IF NOT EXISTS qintopia_agent_os.welcome_inbox (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    source_instance text NOT NULL,
    property_id text NOT NULL,
    event_id text NOT NULL,
    body_hash text NOT NULL CHECK (body_hash ~ '^[0-9a-f]{64}$'),
    envelope jsonb NOT NULL,
    work_item_id uuid UNIQUE REFERENCES qintopia_agent_os.work_items(id),
    status text NOT NULL DEFAULT 'queued'
        CHECK (status IN ('queued','processing','completed','quarantined')),
    fence bigint NOT NULL DEFAULT 0,
    received_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (source_instance, event_id),
    FOREIGN KEY (source_instance, property_id)
        REFERENCES qintopia_agent_os.welcome_sources(source_instance, property_id)
);

CREATE TABLE IF NOT EXISTS qintopia_agent_os.welcome_source_versions (
    source_instance text NOT NULL,
    property_id text NOT NULL,
    aggregate_type text NOT NULL,
    aggregate_id text NOT NULL,
    revision numeric NOT NULL CHECK (revision >= 0 AND revision = trunc(revision)),
    projection_hash text NOT NULL,
    projection jsonb NOT NULL,
    invalidated boolean NOT NULL DEFAULT false,
    PRIMARY KEY (source_instance, property_id, aggregate_type, aggregate_id),
    FOREIGN KEY (source_instance, property_id)
        REFERENCES qintopia_agent_os.welcome_sources(source_instance, property_id)
);

-- Account identity extension. Legacy rows retain their old uniqueness/unknown scope.
CREATE TABLE IF NOT EXISTS qintopia_agent_os.welcome_identity_scopes (
    namespace text NOT NULL,
    source_instance text NOT NULL,
    property_id text NOT NULL,
    PRIMARY KEY (namespace, source_instance, property_id),
    FOREIGN KEY (source_instance, property_id)
        REFERENCES qintopia_agent_os.welcome_sources(source_instance, property_id)
);

CREATE TABLE IF NOT EXISTS qintopia_identity.source_identity_links (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    namespace text NOT NULL,
    subject_type text NOT NULL CHECK (subject_type IN
        ('pms_member','pms_occupant','wecom_external','wecom_internal','qiwe_sender','feishu_open')),
    source_ref text NOT NULL,
    person_id uuid REFERENCES qintopia_identity.persons(id),
    channel_identity_id uuid REFERENCES qintopia_identity.channel_identities(id),
    status text NOT NULL DEFAULT 'pending' CHECK (status IN ('pending','confirmed','revoked')),
    version bigint NOT NULL DEFAULT 1,
    evidence_ref uuid,
    confirmed_by uuid REFERENCES qintopia_identity.persons(id),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (namespace, subject_type, source_ref),
    CHECK (status <> 'confirmed' OR
        (person_id IS NOT NULL AND evidence_ref IS NOT NULL AND confirmed_by IS NOT NULL))
);

CREATE TABLE IF NOT EXISTS qintopia_agent_os.welcome_applications (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    source_instance text NOT NULL,
    resource_ref text NOT NULL,
    record_ref text NOT NULL,
    person_id uuid REFERENCES qintopia_identity.persons(id),
    revision bigint NOT NULL CHECK (revision > 0),
    valid boolean NOT NULL DEFAULT false,
    consent_version bigint NOT NULL DEFAULT 0,
    consent_active boolean NOT NULL DEFAULT false,
    field_hash text NOT NULL,
    UNIQUE (source_instance, resource_ref, record_ref)
);

CREATE TABLE IF NOT EXISTS qintopia_agent_os.welcome_cases (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    source_instance text NOT NULL,
    property_id text NOT NULL,
    order_id text NOT NULL,
    stay_id text NOT NULL,
    occupant_id text NOT NULL,
    person_id uuid REFERENCES qintopia_identity.persons(id),
    identity_link_id uuid REFERENCES qintopia_identity.source_identity_links(id),
    identity_version bigint,
    application_id uuid REFERENCES qintopia_agent_os.welcome_applications(id),
    version bigint NOT NULL DEFAULT 1,
    admitted boolean NOT NULL DEFAULT false,
    manual_hold boolean NOT NULL DEFAULT false,
    readiness_reasons jsonb NOT NULL DEFAULT '[]',
    UNIQUE (source_instance, property_id, stay_id, occupant_id),
    FOREIGN KEY (source_instance, property_id)
        REFERENCES qintopia_agent_os.welcome_sources(source_instance, property_id)
);

CREATE TABLE IF NOT EXISTS qintopia_agent_os.welcome_targets (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    source_instance text NOT NULL,
    property_id text NOT NULL,
    kind text NOT NULL CHECK (kind IN ('community','building','internal')),
    building_code text NOT NULL DEFAULT '',
    display_name text NOT NULL,
    namespace text NOT NULL,
    conversation_ref text NOT NULL,
    version bigint NOT NULL DEFAULT 1,
    enabled boolean NOT NULL DEFAULT false,
    UNIQUE (source_instance, property_id, namespace, conversation_ref),
    FOREIGN KEY (source_instance, property_id)
        REFERENCES qintopia_agent_os.welcome_sources(source_instance, property_id)
);

CREATE TABLE IF NOT EXISTS qintopia_agent_os.welcome_members (
    target_id uuid NOT NULL REFERENCES qintopia_agent_os.welcome_targets(id),
    identity_link_id uuid NOT NULL REFERENCES qintopia_identity.source_identity_links(id),
    episode bigint NOT NULL DEFAULT 1,
    current boolean NOT NULL DEFAULT false,
    observed_at timestamptz NOT NULL,
    version bigint NOT NULL DEFAULT 1,
    PRIMARY KEY (target_id, identity_link_id)
);

CREATE TABLE IF NOT EXISTS qintopia_agent_os.welcome_grants (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    person_id uuid NOT NULL REFERENCES qintopia_identity.persons(id),
    membership_id uuid REFERENCES qintopia_identity.person_memberships(id),
    source_instance text NOT NULL,
    property_id text NOT NULL,
    target_id uuid REFERENCES qintopia_agent_os.welcome_targets(id),
    action text NOT NULL CHECK (action IN ('identity','appoint','review','publish')),
    version bigint NOT NULL DEFAULT 1,
    valid_from timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    revoked_at timestamptz,
    appointed_by uuid NOT NULL REFERENCES qintopia_identity.persons(id),
    CHECK (expires_at > valid_from),
    CHECK (action NOT IN ('review','publish') OR target_id IS NOT NULL),
    FOREIGN KEY (source_instance, property_id)
        REFERENCES qintopia_agent_os.welcome_sources(source_instance, property_id)
);

CREATE TABLE IF NOT EXISTS qintopia_agent_os.welcome_artifact_bindings (
    artifact_id uuid PRIMARY KEY REFERENCES qintopia_agent_os.artifacts(id),
    case_id uuid NOT NULL REFERENCES qintopia_agent_os.welcome_cases(id),
    application_id uuid NOT NULL REFERENCES qintopia_agent_os.welcome_applications(id),
    application_revision bigint NOT NULL,
    case_version bigint NOT NULL,
    consent_version bigint NOT NULL,
    template_version text NOT NULL,
    revoked_at timestamptz
);

CREATE TABLE IF NOT EXISTS qintopia_agent_os.welcome_approvals (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    artifact_id uuid NOT NULL REFERENCES qintopia_agent_os.welcome_artifact_bindings(artifact_id),
    target_id uuid NOT NULL REFERENCES qintopia_agent_os.welcome_targets(id),
    target_version bigint NOT NULL,
    phase text NOT NULL CHECK (phase IN ('preview','formal','internal')),
    content_hash text NOT NULL,
    grant_id uuid NOT NULL REFERENCES qintopia_agent_os.welcome_grants(id),
    grant_version bigint NOT NULL,
    approved_by uuid NOT NULL REFERENCES qintopia_identity.persons(id),
    version bigint NOT NULL DEFAULT 1,
    revoked_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS qintopia_agent_os.welcome_actions (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    case_id uuid NOT NULL REFERENCES qintopia_agent_os.welcome_cases(id),
    target_id uuid NOT NULL REFERENCES qintopia_agent_os.welcome_targets(id),
    phase text NOT NULL CHECK (phase IN ('preview','formal','internal')),
    part text NOT NULL CHECK (part IN ('text','image')),
    work_item_id uuid NOT NULL UNIQUE REFERENCES qintopia_agent_os.work_items(id),
    approval_id uuid NOT NULL REFERENCES qintopia_agent_os.welcome_approvals(id),
    publish_grant_id uuid NOT NULL REFERENCES qintopia_agent_os.welcome_grants(id),
    publish_grant_version bigint NOT NULL,
    target_version bigint NOT NULL,
    execution_epoch bigint NOT NULL,
    status text NOT NULL DEFAULT 'prepared' CHECK
        (status IN ('prepared','claimed','sending','unknown','succeeded','retryable','cancelled','failed')),
    fence bigint NOT NULL DEFAULT 0,
    attempt_id uuid,
    receipt_hash text,
    version bigint NOT NULL DEFAULT 1,
    UNIQUE (case_id, phase, target_id, part)
);

CREATE TABLE IF NOT EXISTS qintopia_agent_os.welcome_upload_intents (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    artifact_id uuid NOT NULL UNIQUE REFERENCES qintopia_agent_os.welcome_artifact_bindings(artifact_id),
    status text NOT NULL DEFAULT 'prepared' CHECK
        (status IN ('prepared','uploading','unknown','registered','failed')),
    storage_ref text,
    content_hash text NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS qintopia_agent_os.welcome_operations (
    operation_id uuid PRIMARY KEY,
    actor_id uuid NOT NULL REFERENCES qintopia_identity.persons(id),
    request_hash text NOT NULL,
    result jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

INSERT INTO qintopia_agent_os.capabilities
    (capability_key, provider_agent, display_name, allowed_callers,
     allowed_work_item_types, risk_level, review_policy, enabled)
VALUES ('resident_welcome.coordinate','silaoshi','住宿欢迎受控流程',
    ARRAY[]::text[], ARRAY['welcome_event','welcome_review','welcome_delivery','welcome_card'],
    'high','human_final_confirmation',false)
ON CONFLICT (capability_key) DO NOTHING;

INSERT INTO qintopia_agent_os.schema_change_log
    (schema_version,migration_name,summary,design_doc_path,metadata)
VALUES ('2026-09-09.001','202609090001_resident_welcome_v1.sql',
    'Additive identity links, durable inbox and governed resident welcome state',
    'docs/data-design/2026-09-09-resident-welcome-v1.md',
    '{"change_type":"additive","external_execution_enabled":false}'::jsonb)
ON CONFLICT (schema_version) DO NOTHING;
