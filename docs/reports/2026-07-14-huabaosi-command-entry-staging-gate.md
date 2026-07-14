# Huabaosi Command-Entry Staging Gate

Date: 2026-07-14

## Observed State

The guarded Huabaosi image-generation path has a staging smoke that checks an explicit
approval phrase, a staging env-file path, an approved database URL hash, and a staging
database name. The Rust worker entrypoint does not enforce those staging facts itself.

A caller can bypass the smoke script, set
`QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED=1`, and invoke
`run-huabaosi-image-generation-worker --once --apply` directly. The current command
connects to Postgres before checking the enable flag, and the default production binary
contains the live provider and media adapter.

No production misuse was observed. This is a code-level prevention gap: shell policy and
an off-by-default environment variable are not a sufficient boundary for an externally
billable adapter that writes a user-visible artifact.

## Root Cause

The first staging implementation treated the smoke script as the only live entrypoint.
That assumption is not preserved by the deployed CLI: operators and automation can call
the subcommand directly, while runtime environment variables cannot prove which binary
or database boundary the caller intended to use.

## Resolution

The live Huabaosi provider/media code will compile only with a non-default
`huabaosi-staging-adapter` Cargo feature. Default and production builds will reject an
apply request as `staging_adapter_not_compiled` before configuration, Postgres, claim
mutation, or network access.

A staging-feature apply with generation enabled will require, in this order:

1. the exact one-shot owner approval phrase;
2. a lowercase SHA-256 approval for the exact database URL;
3. a parsed PostgreSQL URL whose database name contains `staging`; and
4. the complete provider/media configuration and allowlists.

Only after those checks pass may the command connect to Postgres. Disabled and dry-run
execution remains read-only and may inspect eligible internal work without opening an
external connection.

Production artifact and server-source build contracts will continue to use the empty
Cargo feature set and will explicitly reject the Huabaosi staging feature. The staging
smoke must either receive a reviewed staging-feature binary or build with that exact
feature; its preflight must prove the live adapter is compiled before apply.

## Validation

- Rust unit tests for exact approval, database URL hash/name validation, default-build
  compile rejection, and gate ordering before Postgres;
- default and all-feature complete Rust suites plus warning-denied Clippy;
- fake provider/media tests under the staging feature;
- staging-smoke contract tests for the feature build and preflight assertion;
- release artifact, server build, CI, deploy, Markdown, and secret contracts.

No real provider, media storage, production database, Feishu, QiWe, service, or timer is
used by this repair. Owner-approved staging evidence remains a separate operational
decision after the code boundary is reviewed.

## Validation Evidence

The focused default and staging-feature image-generation suites passed with 35 and 34
tests respectively. The first restricted runs failed only when three existing fake
provider/media tests attempted to bind loopback and received `PermissionDenied`. The
same commands were rerun with loopback permission and passed; no test was skipped or
weakened.

The complete default suite passed 306 tests. The all-feature suite passed 302 tests and
kept eight guarded disposable-PostgreSQL tests ignored. Warning-denied Clippy passed for
both no-default-features and all-features builds. The first default Clippy run
identified a conditional-compilation tail expression with a redundant `return`; the code
now uses the branch expression directly instead of suppressing the lint.

Direct CLI checks proved that default apply reports `staging_adapter_not_compiled`
without a database URL, and staging-feature apply with generation enabled rejects a
missing owner phrase before attempting the supplied unreachable database address.

Deploy contracts, deploy preflight, CI contracts, production observation fixtures,
release/current modeling, deploy runner checks, Xiaoman preflight readiness, Markdown
lint, Prettier, secret scanning, Rust formatting, shell parsing, and `git diff --check`
passed. The managed shell refused direct execution of `.husky/pre-commit` even though
git and the filesystem retain mode `100755`; invoking the same tracked hook body with
`sh .husky/pre-commit` passed every check.

## PostgreSQL CI Follow-up

The first PR workflow failed in `Xiaoman PostgreSQL integration` because the existing
operations apply smoke invoked Huabaosi `--apply` with a default binary. The new compile
gate correctly returned `staging_adapter_not_compiled`, while the smoke still expected
the former disabled-state success and later expected to exercise provider retry state.
No database assertion, provider call, or production boundary failed.

The smoke now compiles the Huabaosi staging adapter together with the existing
`postgres-integration-tests` feature. Rust permits its non-staging database exception
only when the explicit apply-smoke flag is set, the exact hashed database URL names
`qintopia_test` on a literal loopback IP without query parameters, and every
provider/media endpoint and allowlist host is a literal loopback IP. The smoke uses the
same owner phrase and URL-hash gate as staging, then targets the existing refused
loopback provider port. A production/default binary, a normal staging-feature binary, an
external host, or any other database cannot use this path.
