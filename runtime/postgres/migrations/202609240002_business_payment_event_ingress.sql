-- Design: docs/data-design/2026-09-24-business-payment-event-ingress.md
-- No inferred historical scopes: adding NOT NULL without defaults fails for old
-- unscoped rows instead of silently associating them with a mutable current binding.
ALTER TABLE qintopia_agent_os.business_feed_checkpoints
    ADD COLUMN IF NOT EXISTS source_instance text NOT NULL,
    ADD COLUMN IF NOT EXISTS property_id text NOT NULL,
    ADD COLUMN IF NOT EXISTS binding_version bigint NOT NULL;
ALTER TABLE qintopia_agent_os.business_event_inbox
    ADD COLUMN IF NOT EXISTS source_instance text NOT NULL,
    ADD COLUMN IF NOT EXISTS property_id text NOT NULL,
    ADD COLUMN IF NOT EXISTS binding_version bigint NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS business_event_trusted_identity
    ON qintopia_agent_os.business_event_inbox(tenant_key,source_instance,property_id,feed,event_id);
CREATE UNIQUE INDEX IF NOT EXISTS business_event_trusted_sequence
    ON qintopia_agent_os.business_event_inbox(tenant_key,source_instance,property_id,feed,source_revision);
CREATE INDEX IF NOT EXISTS business_event_trusted_subject
    ON qintopia_agent_os.business_event_inbox(tenant_key,source_instance,property_id,feed,subject_ref);
INSERT INTO qintopia_agent_os.schema_change_log(schema_version,migration_name,summary,design_doc_path,metadata)
VALUES ('2026-09-24.002','202609240002_business_payment_event_ingress.sql',
'Immutable payment source scope, durable receipt and contiguous feed checkpoint boundaries.',
'runtime/postgres/docs/data-design/2026-09-24-business-payment-event-ingress.md','{"external_effects":false,"local_only":true}')
ON CONFLICT(schema_version) DO NOTHING;
