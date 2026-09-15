# PR #704 integration and merge readiness

Owner: repository maintainers. Started 2026-09-15.

## Scope and acceptance

Integrate the preserved local person collaboration and resident welcome foundation at
`6e6250ee` with master `adb29207`. Preserve the original commits and local-only
execution boundary. This is acceptance of an isolated foundation, not activation of real
login, channel identity, Agent consumers, or production welcome delivery.

- Resolve the five merge conflicts without dropping master capabilities or rules.
- Restore root `AGENTS.md` to master and document whether the proposed reduction
  preserves the existing operating contract; keep policy restructuring separate.
- Fix misleading group retirement impact and add regression coverage.
- Run the new PostgreSQL suites reproducibly and include them in CI/local checks.
- Validate on Rust 1.96, run repository checks and inspect the local browser flow.
- Update PR #704 with current evidence and remaining production exclusions; do not
  merge, publish a release, or activate production.

## Execution

- [x] Inspect PR, package rules, roadmap, guardrails and current testing harness.
- [x] Integrate master and restore root rules.
- [x] Correct lifecycle preview and portable database tests.
- [ ] Run focused tests, browser acceptance and broad checks.
- [x] Review final diff for authorization, migration and compatibility regressions.
- [ ] Update the original PR and inspect current CI/review results.

## Validation

Use `pnpm check:pr:heavy`, the explicit isolated foundation PostgreSQL suites,
`pnpm test:harness`, and browser inspection. Test databases must be disposable,
loopback-only `qintopia_test`; no existing application database is a test target. Retain
sanitized results only. Original recovery hashes prove provenance, not that the
integrated files are unchanged or have passed new tests.

## Rollback

Revert the integration/fix commits before release if needed. Stop local listeners and
remove only the task-created disposable database container. Preserve source history and
do not reverse applied production schema or delete identity history.

Current evidence: [integration report](../../reports/2026-09-15-pr704-integration.md).
Browser acceptance is blocked by the Chrome plugin authentication error; retain draft
until this acceptance and remote checks are complete.

## Reviewer follow-up

- Fix admission of an existing baseline/readback case when a later eligible live event
  arrives; cover negative admission gates, stable case identity, and replay behavior.
- Move the two study documents into `docs/architecture/study-notes/`, label their
  historical scope, repair links, and connect the architecture index.
- Fix PostgreSQL CI's expiry fixture to expire the current reassigned appointment,
  rather than a previous appointment that may have been replaced.
- Run targeted PostgreSQL suites and repository checks, then push and re-read CI and the
  reviewer guide for the updated head. Update the description with actual results.
