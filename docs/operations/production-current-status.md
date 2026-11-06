# Production Current Status

Updated: 2026-11-06

This page is a sanitized operator-facing status index. It is not production evidence and
must not contain live `jobs.json` content, group ids, prompts, env values, database
URLs, tokens, raw logs, or raw script output.

## Release State

- Current production release:
  [v0.3.12](https://github.com/qintopia-agent-studio/qintopia-agent-os/releases/tag/v0.3.12),
  `9c77a51da30211c7183243e37a7225b86bddd122`, published at `2026-11-06T05:58:24Z`.
- [Deploy Production run 42173811323](https://github.com/qintopia-agent-studio/qintopia-agent-os/actions/runs/42173811323)
  succeeded at 14:24:18 Asia/Shanghai, with all four artifact identities matching that
  SHA. Previous release is `c71f68da1ff3e4472623be7c6f30a3aeb4598e95` / v0.3.11.
- Erhua was cleanly drained and stopped; 256 activities and 154 reminders were privately
  backed up. The release-local helper explicitly restarted `hermes-erhua`. New PID
  `4044019`, fresh `running` status, local QiWe health, plugin/source pins and retained
  ledger checks passed before the owned drain marker was cleared. Hermes core remains
  `v2026.9.21`. No credentials, Profile configuration or send gates were changed.
- The change resolves an explicit clock without a date against the original solitaire
  creation day and explains that date in the acknowledgement. Final source, release and
  installed-code checks passed. New real group acknowledgements remain for human
  acceptance; no live test message was sent. See the
  [dated report](../reports/2026-11-06-erhua-clock-default-date.md) for run links,
  retained failure evidence and the separately authorized single-record correction.

## Current Cron Override

- The owner explicitly requested pausing only `小满·每日案例日报`. The official cron
  pause operation has completed, and `enabled=false`, `state=paused` were confirmed in
  this session; the pause record reports `09:18:25` on 2026-11-06.
- This pause was verified before and after the v0.3.12 switch. Preserve it. Do not
  enable the job, backfill it, or wait for a new scheduled daily report as an acceptance
  requirement while the pause remains in force.
- The August activation and next-action entries below are retained historical evidence.
  They do not override this decision or authorize new task activation.

## Historical Release Observations — 2026-08-31

- The latest published release recorded at that historical checkpoint was `v0.2.173`.
- Production `Deploy Production` for `v0.2.173`
  (`9f9423a7fa93fc802b17bcdd3c42cb628ee23ee8`) completed successfully on 2026-08-28.
- Earlier `v0.2.134` through `v0.2.137` production deploy attempts failed during
  release-smoke/bootstrap windows and rolled back to the previous production release.
  They are superseded by the successful `v0.2.173` deploy; keep their detailed evidence
  in the corresponding workflow runs rather than expanding this status index with raw
  logs.
- Hermes cron live apply against `v0.2.173` completed successfully on 2026-08-28. The
  follow-up live-parity observation reported `reviewed_count=8`, `live_count=8`, and
  `enabled_count=3`.
- Xiaoman daily case-report `2026-08-28` was backfilled successfully through the
  reviewed runtime one-shot path after the 09:00 scheduled run had already been missed.
  The scheduled-run freshness observation still reports `scheduled_run_missing` for
  2026-08-28 because it checks the normal 09:00 Hermes sentinel, not backfill success.
- As of 2026-08-31, the latest `Observe Production Runtime` workflow run remains the
  successful 2026-08-28 observation. No retained observation after the 2026-08-29,
  2026-08-30, or 2026-08-31 09:00 Asia/Shanghai scheduled windows is recorded in GitHub
  Actions, so the Xiaoman daily case-report Rust cleanup boundary is still open.
- Repository-local Xiaoman production evidence chain check passed on 2026-08-11 with
  `node tools/deploy/check-xiaoman-production-evidence-chain-local.mjs`. This proves
  local contracts, fixtures, and the character-universe daily-report PR body only; it is
  not production deployment evidence.

## Historical Hermes Cron Migration State — 2026-08-31

These implementation and observation entries describe the August checkpoint. They are
not a current enablement inventory; the current daily-report pause above takes priority.

- Reviewed registry: implemented. Eight reviewed jobs live in
  `runtime/hermes/cron/reviewed-cron-jobs.json`.
- Reviewed wrappers: implemented. Reviewed wrappers live in `runtime/hermes/scripts/`
  and bind workers to `release/current`.
- Apply workflow: implemented. `Apply Production Hermes Crons` accepts only fixed
  targets and `install` or `enable`.
- Apply runner: implemented and deployed in `v0.2.173`.
- Live `jobs.json` install: completed for the reviewed registry through the production
  apply workflow.
- Live enablement: partially enabled. As of 2026-08-28, three reviewed jobs are enabled;
  Xiaoman daily case-report is enabled on the Hermes path, while the retired Xiaoman
  systemd timer remains disabled by design.
- Snapshot sync: implemented in code. Use the `hermes-cron-snapshot` observation target
  after the release containing it is deployed.
- Live declaration parity observation: passed against `v0.2.173` with eight reviewed
  jobs and eight live jobs.
- Worker-run observation: implemented. Use `Observe Production Runtime` worker-run
  targets after scheduled triggers. The Xiaoman daily case-report completion path now
  requires the retained production observation deploy result so the final checker can
  verify `xiaoman-character-universe-v1`, `daily_case_report_second_pass`,
  `raw_messages_included=false`, `profile_fact_text_included=false`,
  `raw_message_payload_read=false`, and `attachment_public_surface_allowed=false`.

## Reviewed Recurring Jobs

This is the reviewed declaration catalog, not a statement that each job is currently
enabled. In particular, `xiaoman-daily-case-report` is now paused.

- `xiaoman` / `xiaoman-weekly-preview`: `30 9 * * 1`,
  `qintopia_xiaoman_weekly_preview.sh`.
- `xiaoman` / `xiaoman-weekly-recruitment`: `0 10 * * 6`,
  `qintopia_xiaoman_weekly_recruitment.sh`.
- `xiaoman` / `xiaoman-weekly-plan-confirmation`: `0 20 * * 0`,
  `qintopia_xiaoman_weekly_plan_confirmation.sh`.
- `xiaoman` / `xiaoman-daily-case-report`: `0 9 * * *`,
  `qintopia_xiaoman_daily_case_report.sh`.
- `erhua` / `erhua-morning-brief`: `10 8 * * *`, `qintopia_erhua_morning_brief.sh`.
- `erhua` / `erhua-activity-recruitment-sat-noon`: `0 12 * * 6`,
  `qintopia_erhua_activity_recruitment.sh`.
- `erhua` / `erhua-activity-recruitment-sat-evening`: `0 21 * * 6`,
  `qintopia_erhua_activity_recruitment.sh`.
- `erhua` / `erhua-activity-recruitment-sun-noon`: `0 12 * * 0`,
  `qintopia_erhua_activity_recruitment.sh`.

## Current Follow-up State

1. The separately authorized record correction is complete: one existing activity now
   starts on November 6 at 19:30, with one pending reminder due at 19:00 Asia/Shanghai.
   Existing reminders are unchanged; transaction evidence is server-private. Actual
   reminder delivery remains pending and must not be inferred from creation.
2. Keep Xiaoman daily case-report paused unless the owner separately asks to resume it.
3. Observe real new-solitaire recognition and acknowledgements for human acceptance.
   Deployment health and reminder creation do not prove later group delivery. Do not
   create live test activities or send test messages for this check.

## Historical Next Actions — Superseded Where Inconsistent with Current Pause

The following August checklist remains for historical context. The daily-report
scheduled-run and completion observations are deferred while the owner-requested pause
is active. None of these items authorizes resuming or replaying the daily report.

1. Observe the next normal Xiaoman daily case-report Hermes scheduled run with the
   `xiaoman-daily-case-report-worker-run` target, then retain the successful observation
   result before treating the Rust migration as complete.
2. For Xiaoman daily case-report completion, retain the production observation deploy
   result that includes `xiaoman-daily-case-report-worker-run` and pass it as
   `--daily-case-report-observation` to the final production completion evidence
   builder/checker.
3. Keep the old Xiaoman daily case-report systemd timer disabled while the Hermes job is
   enabled; do not re-enable both scheduling paths.
4. Enable remaining reviewed Hermes jobs only through `Apply Production Hermes Crons`
   after their owner-reviewed activation criteria are satisfied.

## Improvement Backlog

- Run the read-only snapshot timer observation target after the release containing it is
  deployed.
- Keep Hermes cron apply script boundary checks centralized in
  `tools/deploy/check-deploy-contracts.mjs` so apply scripts cannot drift silently.
