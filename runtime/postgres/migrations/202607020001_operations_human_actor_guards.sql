CREATE SCHEMA IF NOT EXISTS qintopia_agent_os;

CREATE OR REPLACE FUNCTION qintopia_agent_os.is_human_actor_id(actor_id text)
RETURNS boolean
LANGUAGE sql
IMMUTABLE
AS $$
    SELECT COALESCE(
        actor_id IS NULL
        OR btrim(actor_id) = ''
        OR (
            btrim(actor_id) !~* '^(cli_|app_|bot_)'
            AND btrim(actor_id) !~* '^(system|service|worker)([-_:]|$)'
        ),
        false
    );
$$;

DO $$
BEGIN
    IF to_regclass('qintopia_agent_os.work_items') IS NOT NULL
       AND NOT EXISTS (
           SELECT 1
           FROM pg_constraint
           WHERE conname = 'work_items_human_owner_human_actor'
             AND conrelid = 'qintopia_agent_os.work_items'::regclass
       ) THEN
        ALTER TABLE qintopia_agent_os.work_items
            ADD CONSTRAINT work_items_human_owner_human_actor
            CHECK (qintopia_agent_os.is_human_actor_id(human_owner));
    END IF;

    IF to_regclass('qintopia_agent_os.artifacts') IS NOT NULL
       AND NOT EXISTS (
           SELECT 1
           FROM pg_constraint
           WHERE conname = 'artifacts_reviewed_by_human_actor'
             AND conrelid = 'qintopia_agent_os.artifacts'::regclass
       ) THEN
        ALTER TABLE qintopia_agent_os.artifacts
            ADD CONSTRAINT artifacts_reviewed_by_human_actor
            CHECK (qintopia_agent_os.is_human_actor_id(reviewed_by));
    END IF;

    IF to_regclass('qintopia_agent_os.work_item_events') IS NOT NULL
       AND NOT EXISTS (
           SELECT 1
           FROM pg_constraint
           WHERE conname = 'work_item_events_human_actor_id'
             AND conrelid = 'qintopia_agent_os.work_item_events'::regclass
       ) THEN
        ALTER TABLE qintopia_agent_os.work_item_events
            ADD CONSTRAINT work_item_events_human_actor_id
            CHECK (
                actor_type <> 'human'
                OR qintopia_agent_os.is_human_actor_id(actor_id)
            );
    END IF;
END $$;

INSERT INTO qintopia_agent_os.schema_change_log
    (schema_version, migration_name, summary, design_doc_path, metadata)
SELECT
    '2026-07-02.001',
    '202607020001_operations_human_actor_guards.sql',
    'Adds database guardrails so AgentOS human owner, reviewer, confirmer, and human workbench actor fields cannot be persisted as bot/app/service identities.',
    'docs/data-design/2026-07-02-operations-human-actor-guards.md',
    '{"change_type":"additive","domain":"operations_control_plane","risk":"guardrail_only","does_not_enable_external_adapters":true}'::jsonb
WHERE to_regclass('qintopia_agent_os.schema_change_log') IS NOT NULL
ON CONFLICT (schema_version) DO UPDATE SET
    migration_name = EXCLUDED.migration_name,
    status = 'applied',
    summary = EXCLUDED.summary,
    design_doc_path = EXCLUDED.design_doc_path,
    metadata = EXCLUDED.metadata,
    applied_at = now();
