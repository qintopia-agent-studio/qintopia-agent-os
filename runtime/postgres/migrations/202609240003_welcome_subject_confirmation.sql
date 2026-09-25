-- Design: docs/data-design/2026-09-24-welcome-subject-confirmation.md
-- Local additive identity/operations review; no default permissions or sends.
CREATE TABLE IF NOT EXISTS qintopia_identity.work_accounts (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_key text NOT NULL REFERENCES qintopia_agent_os.collaboration_tenants(tenant_key),
    source_link_id uuid NOT NULL REFERENCES qintopia_identity.source_identity_links(id),
    source_version bigint NOT NULL,
    gateway_key text NOT NULL,
    gateway_version bigint NOT NULL,
    label text NOT NULL,
    active boolean NOT NULL DEFAULT true,
    version bigint NOT NULL DEFAULT 1,
    verified_by uuid NOT NULL REFERENCES qintopia_identity.persons(id),
    evidence_ref uuid NOT NULL,
    UNIQUE(tenant_key,source_link_id),
    UNIQUE(tenant_key,id),
    FOREIGN KEY(tenant_key,gateway_key) REFERENCES qintopia_identity.person_identity_gateways(tenant_key,gateway_key)
);
ALTER TABLE qintopia_identity.source_identity_links
    ADD COLUMN IF NOT EXISTS confirmed_by_work_account uuid REFERENCES qintopia_identity.work_accounts(id);
ALTER TABLE qintopia_identity.source_identity_links DROP CONSTRAINT IF EXISTS source_identity_links_check;
DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname='source_identity_links_confirmation_subject') THEN
        ALTER TABLE qintopia_identity.source_identity_links ADD CONSTRAINT source_identity_links_confirmation_subject
        CHECK (status <> 'confirmed' OR (person_id IS NOT NULL AND evidence_ref IS NOT NULL AND
            ((confirmed_by IS NOT NULL AND confirmed_by_work_account IS NULL) OR
             (confirmed_by IS NULL AND confirmed_by_work_account IS NOT NULL))));
    END IF;
END $$;
CREATE TABLE IF NOT EXISTS qintopia_agent_os.welcome_review_settings (
    tenant_key text NOT NULL REFERENCES qintopia_agent_os.collaboration_tenants(tenant_key),
    scope_id uuid NOT NULL REFERENCES qintopia_agent_os.collaboration_scopes(id),
    conversation_id uuid NOT NULL REFERENCES qintopia_messages.conversations(id),
    version bigint NOT NULL DEFAULT 1,
    configured_by uuid NOT NULL REFERENCES qintopia_identity.persons(id),
    authority_refs uuid[] NOT NULL,
    PRIMARY KEY(tenant_key,scope_id)
);
CREATE TABLE IF NOT EXISTS qintopia_agent_os.welcome_review_subject_grants (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_key text NOT NULL,
    scope_id uuid NOT NULL,
    subject_kind text NOT NULL CHECK(subject_kind IN ('person','work_account')),
    person_id uuid REFERENCES qintopia_identity.persons(id),
    work_account_id uuid REFERENCES qintopia_identity.work_accounts(id),
    effects text[] NOT NULL CHECK(cardinality(effects)>0 AND effects <@ ARRAY['identity','review']::text[]),
    valid_until timestamptz NOT NULL,
    revoked_at timestamptz,
    CHECK ((subject_kind='person' AND person_id IS NOT NULL AND work_account_id IS NULL) OR
           (subject_kind='work_account' AND person_id IS NULL AND work_account_id IS NOT NULL)),
    FOREIGN KEY(tenant_key,scope_id) REFERENCES qintopia_agent_os.welcome_review_settings(tenant_key,scope_id)
);
CREATE TABLE IF NOT EXISTS qintopia_agent_os.welcome_review_items (
    work_item_id uuid PRIMARY KEY REFERENCES qintopia_agent_os.work_items(id),
    tenant_key text NOT NULL,
    scope_id uuid NOT NULL,
    case_id uuid NOT NULL REFERENCES qintopia_agent_os.welcome_cases(id),
    application_id uuid NOT NULL REFERENCES qintopia_agent_os.welcome_applications(id),
    configuration_version bigint NOT NULL,
    version bigint NOT NULL DEFAULT 1,
    status text NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','identity_confirmed','confirmed','rejected','revoked')),
    snapshot jsonb NOT NULL,
    candidates jsonb NOT NULL DEFAULT '[]',
    confirmed_person uuid REFERENCES qintopia_identity.persons(id),
    confirmed_channel uuid REFERENCES qintopia_identity.source_identity_links(id),
    identity_receipt uuid,
    content_receipt uuid,
    FOREIGN KEY(tenant_key,scope_id) REFERENCES qintopia_agent_os.welcome_review_settings(tenant_key,scope_id)
);
CREATE TABLE IF NOT EXISTS qintopia_agent_os.welcome_review_receipts (
    id uuid PRIMARY KEY,
    tenant_key text NOT NULL REFERENCES qintopia_agent_os.collaboration_tenants(tenant_key),
    work_item_id uuid NOT NULL REFERENCES qintopia_agent_os.welcome_review_items(work_item_id),
    subject_kind text NOT NULL CHECK(subject_kind IN ('person','work_account')),
    subject_id uuid NOT NULL,
    subject_version bigint NOT NULL,
    subject_proof jsonb NOT NULL,
    grant_id uuid NOT NULL REFERENCES qintopia_agent_os.welcome_review_subject_grants(id),
    request_hash text NOT NULL,
    effects jsonb NOT NULL,
    result jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
INSERT INTO qintopia_agent_os.schema_change_log(schema_version,migration_name,summary,design_doc_path,metadata)
VALUES ('2026-09-24.003','202609240003_welcome_subject_confirmation.sql',
'Explicit welcome work-account subjects, scoped review and identity confirmation audit.',
'runtime/postgres/docs/data-design/2026-09-24-welcome-subject-confirmation.md','{"external_effects":false,"local_only":true}')
ON CONFLICT(schema_version) DO NOTHING;
