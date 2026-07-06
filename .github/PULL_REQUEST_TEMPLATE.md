<!-- markdownlint-disable MD041 -->

## Summary

## Planning

- [ ] Read `AGENTS.md`
- [ ] Read `docs/plans/active/current-roadmap.md`
- [ ] Read `docs/engineering/programming-agent-guardrails.md`
- [ ] Documented the change before implementation
- [ ] Documentation-first exception: typo, formatting, or mechanical change only

Branch:

## Domain

- [ ] agents
- [ ] skills
- [ ] workflows
- [ ] mcp
- [ ] runtime
- [ ] deploy
- [ ] docs
- [ ] fixtures
- [ ] tools
- [ ] deprecated

## Validation

Commands run:

```text

```

## Production Boundary

- [ ] Does not touch production boundary
- [ ] External sends
- [ ] Database writes or migrations
- [ ] Hermes profile runtime
- [ ] systemd / nginx / deploy
- [ ] Feishu / QiWe / external integrations
- [ ] Secrets or runtime configuration

Notes:

## Architecture / Tooling Boundary

- [ ] Uses only approved language/tooling families
- [ ] Does not introduce Java / Gradle / Maven / Kotlin / Go / other new stack
- [ ] Does not add a top-level language bucket
- [ ] Architecture exception approved by owner

## Changelog

- [ ] Updated `CHANGELOG.md`
- [ ] Not user-visible / not needed
