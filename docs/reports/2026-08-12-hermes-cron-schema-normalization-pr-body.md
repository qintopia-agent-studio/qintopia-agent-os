<!-- markdownlint-disable MD041 -->

## Summary

Normalize reviewed Hermes cron apply scripts for legacy live `jobs.json` envelopes:

- accept object envelopes that contain `jobs` but omit `schema_version`, then write back
  `schema_version: 1`
- keep rejecting any explicit unsupported `schema_version`
- make Xiaoman install mode idempotent for already reviewed jobs, refreshing the release
  wrapper without appending duplicate job declarations
- extend focused fixtures for Erhua/Xiaoman cron apply schema normalization and repeat
  install behavior

## Planning

- [x] Read `AGENTS.md`
- [x] Read `docs/plans/active/current-roadmap.md`
- [x] Read `docs/engineering/programming-agent-guardrails.md`
- [x] Documented the change before implementation
- [ ] Documentation-first exception: typo, formatting, or mechanical change only

Branch: `codex/erhua-hermes-cron-schema-normalization`

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
rtk node tools/deploy/test-xiaoman-daily-case-report-hermes-cron.mjs
rtk node tools/deploy/test-xiaoman-weekly-recruitment-hermes-cron.mjs
rtk node tools/deploy/test-xiaoman-weekly-preview-hermes-cron.mjs
rtk node tools/deploy/test-xiaoman-weekly-plan-confirmation-hermes-cron.mjs
rtk node tools/deploy/test-erhua-morning-brief-hermes-cron.mjs
rtk node tools/deploy/test-hermes-cron-live-parity-observation.mjs
rtk node tools/deploy/test-production-hermes-cron-apply-runner.mjs
rtk node tools/deploy/check-deploy-contracts.mjs
rtk node tools/deploy/check-xiaoman-daily-case-report-character-universe-local.mjs
rtk node tools/deploy/check-xiaoman-production-evidence-chain-local.mjs
rtk env PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s workflows/xiaoman-daily-case-report/tests -v
rtk node node_modules/prettier/bin/prettier.cjs --check tools/deploy/test-xiaoman-daily-case-report-hermes-cron.mjs tools/deploy/test-xiaoman-weekly-plan-confirmation-hermes-cron.mjs tools/deploy/test-xiaoman-weekly-preview-hermes-cron.mjs tools/deploy/test-xiaoman-weekly-recruitment-hermes-cron.mjs
rtk git diff --check
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
