# Production Current Status

Updated: 2026-08-11

This page is a sanitized operator-facing status index. It is not production evidence and
must not contain live `jobs.json` content, group ids, prompts, env values, database
URLs, tokens, raw logs, or raw script output.

## Release State

- Latest published Release tag in git: `v0.2.117`.
- Repository-local Xiaoman production evidence chain check passed on 2026-08-11 with
  `node tools/deploy/check-xiaoman-production-evidence-chain-local.mjs`. This proves
  local contracts, fixtures, and the character-universe daily-report PR body only; it is
  not production deployment evidence.
- Production deploy evidence in this repo-local status page still needs to be refreshed
  after `v0.2.117`.
- Master after Release: `#516` is merged after `v0.2.117`; publish the next Release
  before relying on the production status page and new observation targets in
  production.
- Hermes cron live apply: no successful apply is recorded in this status page; the
  latest three `Apply Production Hermes Crons` runs failed before this page was added.
- Hermes cron enablement: not complete; run only after install and declaration parity
  pass.

## Hermes Cron Migration State

- Reviewed registry: implemented. Five reviewed jobs live in
  `runtime/hermes/cron/reviewed-cron-jobs.json`.
- Reviewed wrappers: implemented. Five wrappers live in `runtime/hermes/scripts/` and
  bind workers to `release/current`.
- Apply workflow: implemented. `Apply Production Hermes Crons` accepts only fixed
  targets and `install` or `enable`.
- Apply runner: implemented. `v0.2.117` includes `#514` deploy-runner write-path
  normalization; production deploy evidence still needs to be refreshed.
- Live `jobs.json` install: pending production apply success. Install should write
  reviewed jobs disabled first.
- Live enablement: pending. Enable only after live declaration parity is proven.
- Snapshot sync: implemented in code. Use the `hermes-cron-snapshot` observation target
  after the release containing it is deployed.
- Live declaration parity observation: implemented in code. Use the
  `hermes-cron-live-parity` observation target after install and before enablement.
- Worker-run observation: implemented. Use `Observe Production Runtime` worker-run
  targets after scheduled triggers. The Xiaoman daily case-report completion path now
  requires the retained production observation deploy result so the final checker can
  verify `xiaoman-character-universe-v1`, `daily_case_report_second_pass`,
  `raw_messages_included=false`, and `profile_fact_text_included=false`.

## Reviewed Recurring Jobs

- `xiaoman` / `xiaoman-weekly-preview`: `30 9 * * 1`,
  `qintopia_xiaoman_weekly_preview.sh`.
- `xiaoman` / `xiaoman-weekly-recruitment`: `0 10 * * 6`,
  `qintopia_xiaoman_weekly_recruitment.sh`.
- `xiaoman` / `xiaoman-weekly-plan-confirmation`: `0 20 * * 0`,
  `qintopia_xiaoman_weekly_plan_confirmation.sh`.
- `xiaoman` / `xiaoman-daily-case-report`: `0 8 * * *`,
  `qintopia_xiaoman_daily_case_report.sh`.
- `erhua` / `erhua-morning-brief`: `10 8 * * *`, `qintopia_erhua_morning_brief.sh`.

## Next Production Actions

1. Publish and deploy the next Release so the new observation targets are active in
   production.
2. Run `Apply Production Hermes Crons` with `apply_mode=install` for the selected fixed
   targets.
3. Run `Observe Production Runtime` with `hermes-cron-snapshot` and
   `hermes-cron-live-parity`.
4. Run `Apply Production Hermes Crons` with `apply_mode=enable`, preferably in small
   target groups.
5. Run `Observe Production Runtime` after the first scheduled trigger for each enabled
   target.
6. For Xiaoman daily case-report completion, retain the production observation deploy
   result that includes `xiaoman-daily-case-report-worker-run` and pass it as
   `--daily-case-report-observation` to the final production completion evidence
   builder/checker.

## Improvement Backlog

- Run the read-only snapshot timer observation target after the release containing it is
  deployed.
- Run the read-only live declaration parity observation target after the reviewed Hermes
  install path succeeds.
- Keep Hermes cron apply script boundary checks centralized in
  `tools/deploy/check-deploy-contracts.mjs` so the five apply scripts cannot drift
  silently.
