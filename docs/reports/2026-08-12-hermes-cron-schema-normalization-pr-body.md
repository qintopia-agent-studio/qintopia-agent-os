<!-- markdownlint-disable MD041 -->

## Summary

Make reviewed Xiaoman Hermes cron install idempotent against already installed live
jobs:

- accept object envelopes that contain `jobs` but omit `schema_version`, then write back
  `schema_version: 1`
- keep rejecting any explicit unsupported `schema_version`
- refresh release-local wrappers when install is rerun
- treat an already reviewed Xiaoman job as `already_installed` instead of failing
- keep failing closed on duplicate names, script-boundary drift, schedule drift, origin
  drift, or chat-id drift
- extend focused fixtures for Xiaoman schema normalization and repeat install behavior

## Planning

- [x] Read `AGENTS.md`
- [x] Read `docs/plans/active/current-roadmap.md`
- [x] Read `docs/engineering/programming-agent-guardrails.md`
- [x] Documented the change before implementation
- [ ] Documentation-first exception: typo, formatting, or mechanical change only

Branch: `codex/xiaoman-hermes-cron-idempotent-install`

## Domain

- [ ] agents
- [ ] skills
- [ ] workflows
- [ ] mcp
- [ ] runtime
- [x] deploy
- [x] docs
- [ ] fixtures
- [x] tools
- [ ] deprecated

## Validation

Commands run:

```text
node tools/deploy/test-xiaoman-daily-case-report-hermes-cron.mjs
node tools/deploy/test-xiaoman-weekly-recruitment-hermes-cron.mjs
node tools/deploy/test-xiaoman-weekly-preview-hermes-cron.mjs
node tools/deploy/test-xiaoman-weekly-plan-confirmation-hermes-cron.mjs
node tools/deploy/check-deploy-contracts.mjs
node tools/deploy/check-deploy-runner.mjs
bash -n deploy/sidecar/scripts/apply-xiaoman-daily-case-report-hermes-cron.sh deploy/sidecar/scripts/apply-xiaoman-weekly-recruitment-hermes-cron.sh deploy/sidecar/scripts/apply-xiaoman-weekly-plan-confirmation-hermes-cron.sh deploy/sidecar/scripts/apply-xiaoman-weekly-preview-hermes-cron.sh
node_modules/.bin/prettier --check tools/deploy/test-xiaoman-daily-case-report-hermes-cron.mjs tools/deploy/test-xiaoman-weekly-recruitment-hermes-cron.mjs tools/deploy/test-xiaoman-weekly-plan-confirmation-hermes-cron.mjs tools/deploy/test-xiaoman-weekly-preview-hermes-cron.mjs docs/reports/2026-08-12-hermes-cron-schema-normalization-pr-body.md docs/reports/2026-08-12-hermes-cron-normalization-rollout-notes.md
git diff --check
```

## Production Boundary

- [ ] Does not touch production boundary
- [ ] External sends
- [ ] Database writes or migrations
- [x] Hermes profile runtime
- [x] systemd / nginx / deploy
- [ ] Feishu / QiWe / external integrations
- [ ] Secrets or runtime configuration

Notes:

This PR changes only reviewed release-local apply scripts and local tests/docs. It does
not mutate production by itself, does not enable cron jobs, does not call QiWe/Feishu,
and does not print live `jobs.json`, group ids, prompts, env values, or raw script
output. Live production still requires the signed `Apply Production Hermes Crons`
workflow after release deployment.

## Architecture / Tooling Boundary

- [x] Uses only approved language/tooling families
- [x] Does not introduce Java / Gradle / Maven / Kotlin / Go / other new stack
- [x] Does not add a top-level language bucket
- [ ] Architecture exception approved by owner

## Changelog

- [ ] Updated `CHANGELOG.md`
- [x] Not user-visible / not needed
