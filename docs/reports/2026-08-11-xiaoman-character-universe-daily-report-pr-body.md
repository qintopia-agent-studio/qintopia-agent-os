# Xiaoman Character-Universe Daily Report PR

## Summary

Port the useful `wx-cli` daily-report patterns into Xiaoman's DB-backed daily
case-report path while keeping production evidence and privacy boundaries explicit.

- add current-window character cards, private Markdown daily report output, and a
  private `xiaoman-character-universe-v1` graph export derived from the daily second
  pass;
- forward only safe character-universe counters/schema flags through the production
  auto-publish worker and worker-run observation evidence; and
- require retained production observation evidence before claiming the upgraded Xiaoman
  daily case-report path is production-complete.

## Planning

- [x] Read `AGENTS.md`
- [x] Compared the reference `wx-cli` approach with Xiaoman's existing Postgres-backed
      latest-message path
- [x] Kept latest QiWe messages as the production source of truth
- [x] Documented the production evidence and privacy boundaries
- [ ] Documentation-first exception: typo, formatting, or mechanical change only

Branch: `codex/deploy-runner-snapshot-bootstrap`

## Domain

- [ ] agents
- [ ] skills
- [x] workflows
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
node tools/deploy/check-xiaoman-production-evidence-chain-local.mjs
node tools/deploy/check-xiaoman-daily-case-report-character-universe-local.mjs
node tools/deploy/check-deploy-contracts.mjs
node tools/deploy/test-production-observation-runner.mjs
node tools/deploy/test-production-worker-run-evidence-smoke.mjs
node tools/deploy/test-xiaoman-production-completion-evidence.mjs
node tools/deploy/test-xiaoman-production-completion-manifest-builder.mjs
node tools/deploy/test-finalize-xiaoman-production-completion-evidence.mjs
python3 tools/deploy/test_xiaoman_daily_case_report_backfill_worker.py
env PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s workflows/xiaoman-daily-case-report/tests -v
node tools/workflows/check-workflows.mjs
node_modules/.bin/markdownlint-cli2 AGENTS.md docs/operations/xiaoman-production-evidence-runbook.md docs/operations/release-acceptance-checklist.md docs/plans/active/xiaoman-production-completion-gate.md docs/operations/production-current-status.md tools/deploy/README.md docs/reports/templates/xiaoman-production-completion-evidence.json
git diff --check
```

## Production Boundary

- [ ] Does not touch production boundary
- [ ] External sends
- [ ] Database writes or migrations
- [x] Hermes profile runtime
- [x] systemd / nginx / deploy
- [x] Feishu / QiWe / external integrations
- [ ] Secrets or runtime configuration

Notes: Repository-only work. This PR changes the Xiaoman daily case-report renderer,
auto-publish metadata, deploy-runner observation allowlist, production worker-run
evidence smoke, and completion evidence checkers. It does not publish a Release, deploy
to production, install or enable Hermes crons, call QiWe/Feishu, send externally, or
write live Postgres state. Production completion still requires a retained
`--daily-case-report-observation` deploy result after the released worker runs in
production.

## Architecture / Tooling Boundary

- [x] Uses only approved language/tooling families
- [x] Does not introduce Java / Gradle / Maven / Kotlin / Go / other new stack
- [x] Does not add a top-level language bucket
- [ ] Architecture exception approved by owner

## Changelog

- [ ] Updated `CHANGELOG.md`
- [x] Not user-visible / not needed
