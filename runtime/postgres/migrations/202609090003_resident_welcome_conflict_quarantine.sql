-- Conflicts survive repeated reads of the same vector; higher authority revisions
-- or a future governed correction are required to release this quarantine.
-- Design: docs/data-design/2026-09-09-resident-welcome-v1.md
ALTER TABLE qintopia_agent_os.welcome_source_versions
    ADD COLUMN IF NOT EXISTS conflicted boolean NOT NULL DEFAULT false;
