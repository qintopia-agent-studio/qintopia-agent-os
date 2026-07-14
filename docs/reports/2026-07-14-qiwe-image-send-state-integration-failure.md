# QiWe Image Send State Integration Failure

Date: 2026-07-14

## Observed Evidence

PR #116 CI run `29300466513` failed only in the `QiWe image-send state integration`
step. Migration execution and the existing group send-ready and callback-redaction
PostgreSQL tests passed. The failing assertion was:

```text
stored state leaked integration-group-id
```

The Rust quality, coverage, Clippy, and repository checks passed in the same run.

Local OrbStack was reachable, but starting the CI-equivalent `pgvector/pgvector:pg16`
container failed before container creation because Docker Hub returned `Bad Gateway`.
The host had only plain PostgreSQL images without the required vector extension, so they
were not used as a weaker substitute.

## Root Cause

The integration test serialized the new `qiwe_image_send_attempts` row together with
every historical event attached to its source work item. The source fixture correctly
contains the raw target group id in the earlier `group_message_send_ready_recorded`
event because that pre-existing AgentOS event is the policy fact used to authorize the
target. The assertion intended to inspect only the new image-send state and its audit
events, so it reported a false positive before checking the actual boundary.

The production state machine did not persist the raw group id in
`qiwe_image_send_attempts` or its `qiwe_image_send_*` events. It stores the canonical
target hash there.

## Resolution

Limit the integration query's event aggregation to `qiwe_image_*` events and include the
work item's image-send metadata while keeping the raw group id in the forbidden-value
assertions. This continues to fail if the new state table, work-item metadata, or its
own audit events leak the target, without treating an older authorization event outside
that storage boundary as new leakage.

The same repair also addresses the PR Reviewer Guide findings: group allowlists use
exact case-sensitive matching, and ambiguous sends record an unknown execution outcome
instead of a definite false.

After synchronizing the branch with `master`, the Reviewer Guide for commit `fd10cac`
identified two additional convergence gaps. A callback after the ten-minute upload claim
TTL left its attempt permanently `awaiting_callback`, and terminal send recording still
depended on the two-minute send TTL even though the external request may already have
executed. Both paths could retain an active attempt forever.

The follow-up repair atomically marks a late callback attempt `expired`, stores only the
callback hash, clears the stale claim, and requeues the work item for a new correlation.
After `sending` is committed, success and failure finalization still lock and require
the same attempt, work item, artifact, and claim token, but no longer reject the
terminal write only because the short TTL elapsed.

The Reviewer Guide for commit `bcf0b0e` then identified the remaining no-callback case:
without any callback invocation, an expired `awaiting_callback` attempt still blocked
reclaim forever. The claim transaction now locks and expires one stale awaiting attempt
before selecting work, requeues it, and creates a new correlation with the next attempt
number. It never applies this recovery to `sending`.

## Validation

Run the focused disposable PostgreSQL tests in CI:

```text
cargo test --manifest-path runtime/sidecar/Cargo.toml --features postgres-integration-tests qiwe_image_send_state::tests::postgres_qiwe_send_state_is_idempotent_and_redacted -- --ignored --exact
cargo test --manifest-path runtime/sidecar/Cargo.toml --features postgres-integration-tests qiwe_image_send_state::tests::postgres_qiwe_send_state_rejects_stale_claim -- --ignored --exact
cargo test --manifest-path runtime/sidecar/Cargo.toml --features postgres-integration-tests qiwe_image_send_state::tests::postgres_qiwe_send_state_recovers_expired_callback_and_terminalizes_ambiguous_send -- --ignored --exact
cargo test --manifest-path runtime/sidecar/Cargo.toml --features postgres-integration-tests qiwe_image_send_state::tests::postgres_qiwe_send_state_expires_missing_callback_during_reclaim -- --ignored --exact
```

Also run the full Rust suite, Clippy with warnings denied, pre-commit checks, CI
contract checks, secret scanning, and `git diff --check`. The replacement GitHub Actions
run is the authoritative disposable PostgreSQL gate while the local CI-equivalent
pgvector image cannot be acquired.

## Remaining Boundary

This repair changes validation, audit semantics, and documentation only. It does not
call QiWe, persist callback credentials, enable an external adapter, install a timer,
write Feishu, or send a message. The image-send path remains disabled until the staged
adapter, callback listener, guarded smoke, and owner-reviewed runtime enablement exist.

## Follow-Up Owner Action

Before merging #116, read the Reviewer Guide, reviews, conversation comments, and inline
threads again for the replacement head SHA. Merge only after every actionable item has a
recorded disposition and all required checks pass.

## PR #119 Production Compile Gate

The Reviewer Guide for PR #119 commit `f99cb42` found that runtime disablement was not a
strong enough production boundary. The default release binary contained the real QiWe
upload and `/msg/sendImage` calls, so changing environment variables could make the
binary externally executable even though staging had not yet proved the final JPEG path
or complete `cmd=20000` callback credentials.

The root cause was treating an enable flag, endpoint allowlist, and missing systemd
timer as equivalent to removing the capability from production. Those controls reduce
accidental execution but do not prevent a configuration change from activating code that
is already present.

The repair places all live QiWe apply helpers behind the non-default
`qiwe-staging-adapter` Cargo feature. Default apply now returns
`staging_adapter_not_compiled` before adapter configuration, Postgres access, claims,
state mutation, or network connection. Production artifact builds retain default
features, record `cargo_features: []` in the artifact manifest, and deployment preflight
rejects both artifact and server-source builders if they mention the staging feature.
Fake-server tests still compile the protocol under `cfg(test)`, while Clippy validates
the staging-feature path with all features enabled.

Local artifact validation also exposed that the builder could compile a dirty worktree
while recording only `git rev-parse HEAD` as `commit_sha`. The builder now fails before
compilation unless `git status --porcelain` is empty, and deployment preflight preserves
that check. This prevents compile-gate changes or any other uncommitted bytes from being
misattributed to an approved SHA.

Validation must cover both sides of the boundary:

```text
cargo check --manifest-path runtime/sidecar/Cargo.toml
cargo check --manifest-path runtime/sidecar/Cargo.toml --features qiwe-staging-adapter
cargo test --manifest-path runtime/sidecar/Cargo.toml qiwe_image_send::tests
cargo clippy --manifest-path runtime/sidecar/Cargo.toml --all-targets --no-default-features -- -D warnings
cargo clippy --manifest-path runtime/sidecar/Cargo.toml --all-targets --all-features -- -D warnings
node tools/deploy/preflight.mjs --ci
sh .husky/pre-commit
```

The Reviewer Guide for commit `684ba87` predicted an unused `api_url` binding in the
default build. The exact warning-denied default-feature Clippy command passed locally:
the URL is used to enforce the API-host allowlist before its staging-only struct field
is omitted. The code now explicitly drops that validated URL under the default cfg to
make the ownership boundary unambiguous. More importantly, CI previously ran Clippy only
with all features, so a real default-only regression would not have been covered. The
quality job and its contract now require separate warning-denied default and all-feature
invocations.

Local replacement-head validation passed warning-denied Clippy for both feature sets,
`297/297` default-feature sidecar tests, and `295` all-feature tests with the seven
guarded PostgreSQL tests ignored as designed. Focused QiWe tests passed `23/23` in the
default build and `21/21` with `qiwe-staging-adapter`. The first restricted staging-test
attempt could not bind three loopback fake servers; the exact command passed after the
required loopback-enabled rerun. Deploy preflight, CI contracts, repository pre-commit,
Markdown lint, secret scanning, formatting, and diff checks also passed.

This does not approve building or running the staging feature. Owner-approved provider
and media evidence, the exact callback credential shape, an isolated test group, staging
secrets, rollback ownership, and a reviewed staging command remain required. Production
scheduling and enablement remain separate later decisions.
