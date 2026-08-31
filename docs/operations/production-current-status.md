# Production Current Status

Updated: 2026-08-31

This page is a sanitized operator-facing status index. It is not production evidence and
must not contain live `jobs.json` content, group ids, prompts, env values, database
URLs, tokens, raw logs, or raw script output.

## Release State

- Latest published Release tag in git: `v0.2.173`.
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

## Hermes Cron Migration State

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

## Next Production Actions

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
