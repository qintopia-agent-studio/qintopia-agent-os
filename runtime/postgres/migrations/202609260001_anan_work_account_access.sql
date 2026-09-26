-- Design: docs/data-design/2026-09-26-anan-work-account-access.md
-- Add work-account subjects to the existing business authorization and action ledger.
ALTER TABLE qintopia_agent_os.business_operation_grants
    ADD COLUMN IF NOT EXISTS work_account_id uuid REFERENCES qintopia_identity.work_accounts(id),
    ADD COLUMN IF NOT EXISTS work_account_version bigint,
    ADD COLUMN IF NOT EXISTS account_role text;
ALTER TABLE qintopia_agent_os.business_operation_grants
    DROP CONSTRAINT IF EXISTS business_operation_account_subject;
ALTER TABLE qintopia_agent_os.business_operation_grants
    ADD CONSTRAINT business_operation_account_subject CHECK
    ((work_account_id IS NULL AND work_account_version IS NULL AND account_role IS NULL) OR
     (work_account_id IS NOT NULL AND work_account_version IS NOT NULL AND account_role IN ('operator','admin')));
DROP INDEX IF EXISTS qintopia_agent_os.business_operation_grant_current;
CREATE UNIQUE INDEX IF NOT EXISTS business_operation_grant_person_current
    ON qintopia_agent_os.business_operation_grants(tenant_key,authority_grant_id,binding_id,operation_key)
    WHERE revoked_at IS NULL AND work_account_id IS NULL;
CREATE UNIQUE INDEX IF NOT EXISTS business_operation_grant_account_current
    ON qintopia_agent_os.business_operation_grants(tenant_key,work_account_id,binding_id,operation_key)
    WHERE revoked_at IS NULL AND work_account_id IS NOT NULL;

ALTER TABLE qintopia_agent_os.business_turn_evidence
    ALTER COLUMN person_id DROP NOT NULL,
    ADD COLUMN IF NOT EXISTS work_account_id uuid REFERENCES qintopia_identity.work_accounts(id),
    ADD COLUMN IF NOT EXISTS work_account_version bigint;
ALTER TABLE qintopia_agent_os.business_turn_evidence
    DROP CONSTRAINT IF EXISTS business_turn_subject;
ALTER TABLE qintopia_agent_os.business_turn_evidence
    ADD CONSTRAINT business_turn_subject CHECK
    ((person_id IS NOT NULL AND work_account_id IS NULL AND work_account_version IS NULL) OR
     (person_id IS NULL AND work_account_id IS NOT NULL AND work_account_version IS NOT NULL));

ALTER TABLE qintopia_agent_os.business_actions
    ALTER COLUMN actor_person_id DROP NOT NULL,
    ADD COLUMN IF NOT EXISTS actor_work_account_id uuid REFERENCES qintopia_identity.work_accounts(id),
    ADD COLUMN IF NOT EXISTS actor_work_account_version bigint,
    ADD COLUMN IF NOT EXISTS confirmed_by_work_account uuid REFERENCES qintopia_identity.work_accounts(id);
ALTER TABLE qintopia_agent_os.business_actions
    DROP CONSTRAINT IF EXISTS business_action_subject;
ALTER TABLE qintopia_agent_os.business_actions
    ADD CONSTRAINT business_action_subject CHECK
    ((actor_person_id IS NOT NULL AND actor_work_account_id IS NULL AND actor_work_account_version IS NULL) OR
     (actor_person_id IS NULL AND actor_work_account_id IS NOT NULL AND actor_work_account_version IS NOT NULL));
ALTER TABLE qintopia_agent_os.business_actions
    DROP CONSTRAINT IF EXISTS business_action_confirmation_subject;
ALTER TABLE qintopia_agent_os.business_actions
    ADD CONSTRAINT business_action_confirmation_subject CHECK
    (confirmed_by IS NULL OR confirmed_by_work_account IS NULL);

INSERT INTO qintopia_agent_os.schema_change_log(schema_version,migration_name,summary,design_doc_path,metadata)
VALUES ('2026-09-26.001','202609260001_anan_work_account_access.sql',
'Allow observed work accounts to hold scoped Anan operations and retain action evidence without inventing a person.',
'runtime/postgres/docs/data-design/2026-09-26-anan-work-account-access.md',
'{"external_effects":false,"bootstrap_grants":false}')
ON CONFLICT(schema_version) DO NOTHING;
