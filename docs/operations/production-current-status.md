# Production Current Status

Updated: 2026-08-11

This page is a sanitized operator-facing status index. It is not production evidence and
must not contain live `jobs.json` content, group ids, prompts, env values, database
URLs, tokens, raw logs, or raw script output.

## Release State

- Latest published Release: `v0.2.116`.
- Production deploy: `v0.2.116` deploy workflow succeeded on 2026-08-11.
- Master after Release: `#514` is merged after `v0.2.116`; publish the next Release
  before relying on its runner-path normalization in production.
- Hermes cron live apply: not complete after `v0.2.116`; the latest three
  `Apply Production Hermes Crons` runs failed before this page was added.
- Hermes cron enablement: not complete; run only after install and declaration parity
  pass.

## Hermes Cron Migration State

- Reviewed registry: implemented. Five reviewed jobs live in
  `runtime/hermes/cron/reviewed-cron-jobs.json`.
- Reviewed wrappers: implemented. Five wrappers live in `runtime/hermes/scripts/` and
  bind workers to `release/current`.
- Apply workflow: implemented. `Apply Production Hermes Crons` accepts only fixed
  targets and `install` or `enable`.
- Apply runner: implemented, latest hardening pending Release after `v0.2.116`. `#514`
  normalizes deploy-runner write paths.
- Live `jobs.json` install: pending production apply success. Install should write
  reviewed jobs disabled first.
- Live enablement: pending. Enable only after live declaration parity is proven.
- Snapshot sync: implemented in code. Production timer installation and recent commit
  evidence still need an explicit observation path.
- Worker-run observation: implemented. Use `Observe Production Runtime` worker-run
  targets after scheduled triggers.

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

1. Publish and deploy the next Release after `v0.2.116` so the merged `#514` runner path
   normalization is active in production.
2. Run `Apply Production Hermes Crons` with `apply_mode=install` for the selected fixed
   targets.
3. Verify live declarations through a reviewed read-only path before enabling.
4. Run `Apply Production Hermes Crons` with `apply_mode=enable`, preferably in small
   target groups.
5. Run `Observe Production Runtime` after the first scheduled trigger for each enabled
   target.

## Improvement Backlog

- Add a read-only snapshot timer observation target that reports only safe facts: timer
  state, snapshot repo existence, remote absence, and latest commit time.
- Add a read-only live declaration parity observation target for
  `reviewed-cron-jobs.json` versus live Hermes `jobs.json`.
- Keep Hermes cron apply script boundary checks centralized in
  `tools/deploy/check-deploy-contracts.mjs` so the five apply scripts cannot drift
  silently.
