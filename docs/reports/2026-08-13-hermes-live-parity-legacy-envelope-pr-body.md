<!-- markdownlint-disable MD041 -->

## Summary

Allow the Hermes live parity observation to accept legacy live `jobs.json` envelopes
that omit `schema_version`, while still rejecting any explicit unsupported schema
version.

This matches the production compatibility boundary for Hermes cron files: legacy live
envelopes may be `{ "jobs": [...] }`, but reviewed apply paths normalize writes to
`schema_version: 1` when they touch the file.

## Planning

- [x] Read `AGENTS.md`
- [x] Read the Hermes live parity observation script
- [x] Added focused fixture coverage for legacy and unsupported schema envelopes
- [ ] Documentation-first exception: typo, formatting, or mechanical change only

Branch: `codex/hermes-live-parity-legacy-envelope`

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
node tools/deploy/test-hermes-cron-live-parity-observation.mjs
node tools/deploy/check-deploy-contracts.mjs
git diff --check
```

Commit hook also ran:

```text
formatting
markdownlint
qintopia tools check
workflow check
deploy contract check
Xiaoman preflight readiness check
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

This PR changes only a read-only observation smoke and its local fixture. It does not
mutate production, install cron jobs, call QiWe/Feishu, or print live cron contents,
group ids, prompts, env values, or raw messages.

## Architecture / Tooling Boundary

- [x] Uses only approved language/tooling families
- [x] Does not introduce Java / Gradle / Maven / Kotlin / Go / other new stack
- [x] Does not add a top-level language bucket
- [ ] Architecture exception approved by owner

## Changelog

- [ ] Updated `CHANGELOG.md`
- [x] Not user-visible / not needed
