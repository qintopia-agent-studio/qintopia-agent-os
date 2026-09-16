# Hermes production recovery

Started: 2026-09-16. Status: implementation; production recovery not yet accepted.

## Intent and boundaries

Restore the existing dashboard, messaging, tools, and scheduled work without local
patches to the official Hermes core. Preserve all seven profiles and the existing
five-enabled/two-disabled WeCom contract. Work independently of PR #704.

The owner approved implementation, versioned rollout one service at a time, and
acceptance using messages the owner sends. Do not initiate external test messages,
replay uncertain jobs, clear execution locks, overwrite live configuration, or restore
the old mixed checkout wholesale. Do not restart a busy gateway; wait at most ten
minutes and defer it if still busy. Observe each accepted gateway for fifteen minutes
before proceeding. Incident closure requires a further 24-hour observation window.

## Baseline

Production rechecked at 2026-09-16 11:51 CST:

- Qintopia release: `c3c605ab1ba8ca46155e467fbe22c5ef5ad03bb4`.
- Official Hermes: `2237be355906fbe6065ce1815711eee52b2d646e`.
- Seven gateways active; this is process evidence, not functional acceptance.
- Dashboard process predates the September 13 core replacement; its missing frontend
  produces HTTP 500.
- QiWe discovery fails because the deployed package omits `space_agent_completion.py`.
- Silaoshi WeCom reports lost subscription and failed final/fallback delivery.
- Default cron acknowledgement warnings reconcile to completed executions; the
  publication race is reproduced.
- Snapshot timer fails reading a root-owned mode-0700 legacy wrapper.

## Execution checklist

- [x] Create an isolated branch from current master; recheck production identity.
- [ ] Record sanitized per-profile evidence and server-local recovery backups.
- [x] Build matching dashboard artifact and pass isolated HTTP smoke; upstream typecheck
      exception remains open.
- [x] Add versioned dashboard activation, preflight, smoke, and rollback with
      transaction tests.
- [ ] Repair only reviewed wrapper permissions through a release-owned installer.
- [x] Reproduce and fix packaged plugin discovery across three fresh process consumers.
- [ ] Complete WeCom lifecycle replay; uncertain-send duplication is reproduced and
      blocked upstream.
- [ ] Classify tool errors and verify the Silaoshi bridge without business actions.
- [x] Reproduce acknowledgement race and reconcile historical execution outcomes.
- [ ] Run targeted tests, repository checks, and review the deployment boundary.
- [x] Merge independent recovery PR #706 (672199b); production remains unchanged.
- [ ] Merge follow-up dashboard deploy-bundle delivery, deploy verified artifacts, and
      record component acceptance.
- [ ] Complete owner-triggered messaging/browser checks and observation window.

## Validation and stop conditions

Use `pnpm runtime:hermes:check`, `pnpm test:qiwe`, `pnpm deploy:hermes-core:check`,
`pnpm deploy:contracts:check`, `pnpm check:pr:quick`, and `pnpm check:pr:heavy`, plus
focused new regressions. No production database migration is intended. Use the Codex
Chrome plugin, never Playwright, for dashboard acceptance; unavailable browser access
remains an explicit pending check.

Keep upstream defects distinct from integration defects. If no qualified official core
fixes an upstream defect, record it as blocked rather than patching production or
repeatedly restarting. Stop rollout on configuration drift, duplicate effects, startup
failure, or unresolved data compatibility. Roll back only the affected component to its
recorded code/configuration tuple; do not roll back live data.

Maintain the incident report and recovery runbook alongside implementation. A passed
preflight, quiet logs, or active systemd unit is never sufficient for closure.
