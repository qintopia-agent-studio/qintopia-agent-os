# Data Design

This directory records versioned database design decisions for the Qintopia message and
Agent OS data layer.

## Rules

- Every schema migration must have a matching design note in this directory.
- Every schema migration must add one entry to `CHANGELOG.md`.
- Every applied migration should insert or update a row in
  `qintopia_agent_os.schema_change_log`.
- Design notes should explain purpose, scope, compatibility, table ownership, privacy
  boundaries, and follow-up work.
- Database credentials and environment-specific URLs must not be committed.

## Versions

- `2026-09-23.009`: [Steward review delegation](2026-09-23-steward-review-delegation.md)

- `2026-09-23.008`: [Scoped text-rule lifecycle](2026-09-23-person-rule-lifecycle.md)

- `2026-09-23.007`:
  [Building history registration and replay-safe installation](2026-09-23-ontology-audience.md)

- `2026-09-23.006`:
  [Observed building history and dynamic contact audiences](2026-09-23-ontology-audience.md)

- `2026-09-22.005`: [Synthetic turn replay](2026-09-22-person-foundation-consumers.md)

- `2026-09-22.004`: [Welcome local executors](2026-09-22-foundation-welcome.md)

- `2026-09-22.003`: [Shared foundation welcome](2026-09-22-foundation-welcome.md)

- `2026-09-22.002`: [Person identity and memory](2026-09-22-person-memory.md)

- `2026-09-22.001`:
  [Person foundation consumers](2026-09-22-person-foundation-consumers.md)

- `2026-09-18.001`: [Workbench accounts](2026-09-18-workbench-accounts.md)

- `2026-09-11.001`:
  [Organization and person workbench A](2026-09-11-organization-person-workbench.md)

- `2026-09-10.003`:
  [Duties and permission modes](2026-09-10-person-agent-collaboration-v1.md)

- `2026-09-10.002`: [Role duty choices](2026-09-10-person-agent-collaboration-v1.md)

- `2026-09-10.001`:
  [Person–Agent Collaboration V1](2026-09-10-person-agent-collaboration-v1.md) (F1 local
  implementation)
- `2026-09-09.001`: `2026-09-09-resident-welcome-v1.md`

- Change history: `CHANGELOG.md`
- `2026-06-18.001`: `2026-06-18-message-capture-v1.md`
- `2026-06-24.002`: `2026-06-24-agent-os-data-layer-v2.md`
- `2026-06-26.003`: `2026-06-26-identity-observations-v3.md`
- `2026-06-26.004`: `2026-06-26-profile-digest-archive-v1.md`
- `2026-06-27.005`: `2026-06-27-event-signals-v2.md`
- `2026-06-29.006`: `2026-06-29-erhua-training-memory.md`
- `2026-06-30.007`: `2026-06-30-operations-control-plane.md`
- `2026-07-02.001`: `2026-07-02-operations-human-actor-guards.md`
- `2026-07-13.002`: `2026-07-13-huabaosi-image-generation.md`
- `2026-07-14.001`: `2026-07-14-xiaoman-event-signal-mutations.md`
- `2026-07-14.002`: `2026-07-14-qiwe-image-send-state.md`
- `2026-07-14.003`: `2026-07-14-qiwe-upload-attempt-lifecycle.md`
- `2026-08-08.001`: `2026-08-08-xiaoman-daily-case-report-auto-publish.md`
- `2026-08-14.001`: `2026-08-14-erhua-conversational-self-extension.md`
- `2026-08-15.001`: `2026-08-15-space-execution-runner-contract.md`
