# Xiaoman Downstream Dry-Run Report Record

Date: 2026-07-13

## Scope

After `v0.2.5` completed its systemd bootstrap deployment, the owner-approved, aggregate
Xiaoman production preflight stopped during the read-only evidence/visual worker
observation.

## Finding And Resolution

| Finding                                                                                                                                                                                                 | Classification                   | Resolution                                                                       | Prevention                                                                                                                                                                                                                      |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------- | -------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `run-evidence-worker --once --dry-run` and `run-collaboration-worker --work-item-type visual_asset_request --once --dry-run` returned `apply_requested=false` but incorrectly reported `dry_run=false`. | Read-only report-contract defect | Derive each worker report's `dry_run` value from `apply_requested` in one place. | Fixture unit tests and the disposable PostgreSQL apply smoke assert `dry_run=true`, `apply_requested=false`, zero artifact writes, and unchanged queued work items; the production preflight retains its fail-closed assertion. |

## Production Evidence

- The first two checks passed: Xiaoman activity signal timer observation and Xiaoman
  activity promotion starter timer observation.
- The aggregate smoke stopped at the downstream evidence/visual timer observation.
- Sanitized worker inspection showed `success=true`, `apply_requested=false`,
  `fixture_mode=false`, `action_status=dry_run_ok`, and zero artifact ids for both
  workers. The check did not apply writes.
- No Feishu write, QiWe call, poster publish, external send, or server-side edit was
  performed during the investigation.

## Required Follow-Up

1. Merge the report-contract fix through a reviewed PR.
2. Publish and deploy the resulting release through the normal release path.
3. Re-run the aggregate read-only preflight and record only sanitized queue counts and
   the pass or hold decision in
   `deploy/smoke/docs/xiaoman-production-preflight-record.md`.
