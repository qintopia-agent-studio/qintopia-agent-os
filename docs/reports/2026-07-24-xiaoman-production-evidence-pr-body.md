# Xiaoman Production Evidence Handoff PR

## Summary

Finish the repository-side QiWe independent production enablement chain and Huabaosi
first-record production canary evidence chain handoff surface.

- add the reviewed QiWe production artifact builder, production evidence-chain local
  verification entrypoint, and one-shot completion finalizer;
- harden deploy/runtime/test/document contracts around the separated
  `huabaosi-production` and `qiwe-production` artifact identities; and
- add handoff-ready reports, an HTML production test map, and execution docs for the
  remaining owner-operated production evidence capture.

## Planning

- [x] Read `AGENTS.md`
- [x] Read `docs/plans/active/current-roadmap.md`
- [x] Read `docs/engineering/programming-agent-guardrails.md`
- [x] Documented the change before implementation
- [ ] Documentation-first exception: typo, formatting, or mechanical change only

Branch: `codex/qiwe-production-enablement-chain`

## Domain

- [ ] agents
- [ ] skills
- [ ] workflows
- [ ] mcp
- [x] runtime
- [x] deploy
- [x] docs
- [ ] fixtures
- [x] tools
- [ ] deprecated

## Validation

Commands run:

```text
node tools/deploy/check-xiaoman-production-evidence-chain-local.mjs
node tools/deploy/check-deploy-contracts.mjs
node tools/deploy/test-finalize-xiaoman-production-completion-evidence.mjs
```

## Production Boundary

- [ ] Does not touch production boundary
- [ ] External sends
- [ ] Database writes or migrations
- [ ] Hermes profile runtime
- [x] systemd / nginx / deploy
- [x] Feishu / QiWe / external integrations
- [ ] Secrets or runtime configuration

Notes: Repository-only work. This PR adds and hardens production-adjacent builders,
checkers, deploy runner/result guards, evidence contracts, and owner-operated execution
docs. It does not publish a Release, auto-merge Release Please, deploy to production,
enable timers, send externally, or write live Postgres/Feishu/QiWe state.

## Architecture / Tooling Boundary

- [x] Uses only approved language/tooling families
- [x] Does not introduce Java / Gradle / Maven / Kotlin / Go / other new stack
- [x] Does not add a top-level language bucket
- [ ] Architecture exception approved by owner

## Changelog

- [ ] Updated `CHANGELOG.md`
- [x] Not user-visible / not needed
