# CI Tools

`tools/ci` owns repository checks that decide which Agent OS domains changed and which
validation commands are required.

CI helpers must:

- treat docs-only changes differently from runtime/artifact builds where safe;
- keep required checks explicit for skills, workflows, MCP, runtime, deploy, and agents;
- fail closed when production-adjacent files change;
- enforce Conventional Commits commit message types for local and CI validation;
- validate pull request bodies so agents cannot submit empty templates;
- never require secrets for pull request validation.

## Validation

```bash
pnpm ci:low-risk:test
pnpm ci:low-risk:eligibility:test
pnpm check:pr:auto
pnpm check:pr:quick
pnpm check:pr:heavy
pnpm check:pr:postgres
pnpm tools:ci:check
pnpm commitlint:check
pnpm pr:check-body
pnpm release-please:check
pnpm pr:tools:check
```

## Low-Risk Classification

`pnpm ci:low-risk:classify -- --base-ref <base> --head-ref <head>` reads the two
explicit Git commits and emits deterministic JSON eligibility evidence. The base must be
an ancestor of the head. This command only classifies a committed diff: it does not
merge a pull request, publish a Release, deploy, send a message, or inspect uncommitted
working-tree files.

Version 3 allows only these dedicated file roles:

- mappings: `fixtures/qiwe/event-mappings/**/*.mapping.json`;
- sanitized synthetic inputs: `fixtures/qiwe/system/**/*.fixture.json`;
- corresponding canonical outputs: `fixtures/qiwe/event-mappings/**/*.expected.json`;
- optionally, one restricted parser recipe:
  `fixtures/qiwe/event-mappings/_primitives/**/*.primitive.json`;
- optionally, one fixed-format mapping summary:
  `fixtures/qiwe/event-mappings/**/*.mapping.md`.

One candidate is exactly one commit with three required JSON files, optionally one
primitive and optionally one mapping summary, for a maximum of five files. The files are
append-only and must form one cross-referenced bundle. Each fixture declares
`sanitized=true` and `synthetic=true`, and must contain more input records than the
expectation emits so at least one adjacent selector non-match is exercised. Each
expectation binds the exact fixture and mapping and contains only canonical event output
fields. Strict JSON parsing rejects duplicate keys, including escaped duplicate
spellings.

Mappings use the bounded selector/extractor DSL and may cite only HTTPS pages on
`doc.qiweapi.com`. An optional primitive must be referenced by that same mapping and may
compose only the fixed `base64_utf8`, `json_parse`, `json_pointer`, `split`,
`string_trim`, and `array_flatten` kernel. Recipes cannot call other recipes or add
runtime code. The only accepted Markdown is the fixed mapping summary, which may name
only the same mapping, fixture, expectation, definition key, and declarative scope in
that bundle. Every other file type is outside this class. Exact details are in
`docs/engineering/qiwe-restricted-parser-primitives.md`.

The classifier fails closed for every path outside that list, deletes, renames,
mutations of mapping or replay JSON, executable files, symlinks, invalid or duplicate
JSON, unsafe integer identifiers, privileged fields, unbounded transforms, and
non-official URLs. Python, Rust, shell, SQL migrations, workflows, authentication,
dependencies, deployment, and send-path code therefore cannot receive low-risk
eligibility.

### Low-Risk Auto Release

`Low-Risk Auto Release` is the sole default-off exception to manual merge and Release
publication. It is disabled unless `QINTOPIA_LOW_RISK_AUTO_RELEASE_ENABLED` is exactly
`1`, `QINTOPIA_LOW_RISK_AUTO_RELEASE_OWNER_ACKNOWLEDGEMENT` is exactly
`approved-low-risk-auto-release-v1`, a fixed automation actor is configured, and the
dedicated repository-scoped token belongs to that actor.

The workflow advances three exact-head stages. First it verifies the fixed actor,
same-repository branch, label provenance, single candidate commit, required checks, and
classifier result before squash-merging the mapping PR. The candidate squash must be the
only commit after the latest published SHA. Second it authenticates the exact Release
Please PR, requires CI and PR-Agent checks, validates the bot-created
`Release Please validation` status against its exact run, workflow path, repository,
branch, head SHA, and unique successful required jobs, then merges exactly one metadata
squash. Third it requires the publication range to contain only those two squashes and
rechecks the draft tag, current `master`, latest published Release, and the complete
`previous_published_tag..candidate_master_sha` range before publishing that draft.

The draft contract binds its bot author, release id, exact tag and name, target, exact
changelog section, and zero assets into a canonical digest. The workflow rechecks that
digest immediately before publication and again after publication, then refetches the
tag SHA. Any mismatch fails closed.

Every stage reruns the append-only classifier before mutation. The workflow cannot
create a deploy request, activate ingress, capabilities, mappings, automations,
services, timers, credentials, or sends. Any file outside the
mapping/fixture/expectation/optional primitive/optional mapping-summary contract stops
the lane and returns the change to an explicit manual owner decision.

## Local Pre-PR Tiers

- `pnpm check:pr:quick`: local mirror of the ordinary PR light gate before opening a PR.
- `pnpm check:pr:heavy`: quick tier plus the sidecar-heavy Rust checks and the local
  PostgreSQL tier. Use it before high-risk PRs that touch sidecar, Postgres, deploy
  scripts, or CI itself.
- `pnpm check:pr:postgres`: reruns only the disposable PostgreSQL tier against local
  `qintopia_test` at `127.0.0.1:5432`.
- `pnpm check:pr:auto`: inspects the current branch diff, always runs the quick tier,
  escalates to the heavy Rust tier when production-adjacent high-risk paths changed, and
  runs the PostgreSQL tier too when the disposable local database is ready.

The local PostgreSQL tier uses only `qintopia_test`, the reviewed ignored Rust tests,
and the guarded apply smoke. It does not need production secrets and must fail closed
when the disposable database is not ready.

## Release Please PRs

Release Please PRs are generated by `github-actions[bot]` from merged Conventional
Commits. An authenticated manual dispatch binds validation to the exact open bot-owned
head and forces the complete light, runtime, Rust quality, and disposable PostgreSQL
tier. The release metadata check additionally validates the release manifest, changelog
shape, and Xiaoman production-complete claim boundary.

A generated release may mention Xiaoman `production-complete` only when the release
materials also identify the owner-retained evidence bundle, the completion gate, the
`xiaoman-production-completion-evidence-v1` schema, and
`tools/deploy/check-xiaoman-production-completion-evidence.mjs`. Infrastructure and
activation-ready releases should keep using explicit non-completion wording.

Ordinary PRs still run `pnpm pr:check-body` and `pnpm check:light`; runtime-sensitive
changes still add `pnpm check:runtime` through the existing changed-file gate. Heavy
Rust quality and disposable PostgreSQL integration jobs are risk-tiered separately so
ordinary Hermes, deploy-runner, documentation, or metadata changes do not pay the full
sidecar/PostgreSQL cost unless they touch the sidecar, Postgres, deploy sidecar scripts,
or the CI workflow itself. Manual workflow dispatches and authenticated Release Please
validation always force the full heavy tier. The PR-attached `check` and release
statuses are published after their corresponding jobs finish; the manual PR-Agent
dispatch publishes its existing required status after the authenticated no-review job
succeeds. This keeps workflow-dispatch validation visible to the master ruleset without
adding another check tier.

The matching local path is `pnpm check:pr:auto` for day-to-day work and
`pnpm check:pr:heavy` when you want the full local Rust/PostgreSQL mirror before pushing
a high-risk PR.

If the bot update also suppresses the ruleset-required `PR-Agent review assistant`, run
`pr-agent.yml` manually with the same exact release head and explicit PR number. The
workflow authenticates the Release Please PR and skips the external PR-Agent action; its
successful job satisfies the required check without reviewing or editing generated
release metadata.

## Rust Quality And Xiaoman Integration

Sidecar, Postgres, deploy sidecar scripts, or CI workflow changes run a Rust 1.96
quality baseline. It uploads LCOV and a coverage summary, then executes the non-ignored
sidecar suite with all features so staging-only adapter tests run before strict
default/all-feature Clippy. The all-feature test is not a production build and must not
execute ignored PostgreSQL tests. The Xiaoman downstream integration job owns those
ignored tests and the guarded apply smoke against a disposable GitHub Actions PostgreSQL
service. It must not accept production database URLs, secrets, Feishu credentials, QiWe
credentials, or external adapters.
