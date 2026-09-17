# QiWe activity reminders: time resolution gap

## Evidence and cause

Read-only inspection on 2026-09-17 found Erhua active with a current cron heartbeat,
activity reminders enabled and live mode selected. Two recent activity records had
unresolved clock-only expressions and no reminder jobs. The latest recorded successful
activity reminder was September 10. No credentials, message bodies or ledgers are copied
here. Separately disabled recurring recruitment jobs are not this reminder mechanism.

PR #649 changed model output from normalized dates to verbatim time expressions. The
resolver covered only some expressions, while the scheduler accepted canonical dates.
Unsupported expressions silently produced no reminder. First-activity acknowledgements
still promised a reminder. This explains the inspected records, not every historical
missing delivery or the exact deployment date of the regression.

## Resolution and validation

Implementation and validation are tracked in
[the focused plan](../plans/completed/qiwe-reminder-lifecycle.md). The package
[operating contract](../../skills/qiwe/README.md#activity-reminder-lifecycle) defines
time resolution, durable state, delivery uncertainty and rollback boundaries.

Production remains unchanged. Owner acceptance requires a reviewed release, isolated
fixtures, then verification of newly received activity plans. Historical reminders must
not be automatically replayed; ambiguous dates need human clarification and uncertain
send outcomes need reconciliation.

## Validation result

- `pnpm test:qiwe`: 326 tests passed, with one Linux-only check skipped on macOS.
- `pnpm runtime:hermes:check`: passed, with one platform-specific skip.
- `pnpm check:pr:auto`: quick tier passed. The initial sandbox run failed because local
  NATS test sockets could not bind (`Operation not permitted`); the same project checks
  passed with local fixture execution permitted. No production endpoints were used by
  these tests.
- Operations/backward-compatibility review added a legacy terminal status for new
  delivery outcomes, stale-snapshot protection and a no-backfill rule on late edits.

Live acceptance remains pending release review and deployment; these results do not
prove recovery of historical reminders or authorize replay.
