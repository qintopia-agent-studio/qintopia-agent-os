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

## Validation

Run the focused disposable PostgreSQL tests in CI:

```text
cargo test --manifest-path runtime/sidecar/Cargo.toml --features postgres-integration-tests qiwe_image_send_state::tests::postgres_qiwe_send_state_is_idempotent_and_redacted -- --ignored --exact
cargo test --manifest-path runtime/sidecar/Cargo.toml --features postgres-integration-tests qiwe_image_send_state::tests::postgres_qiwe_send_state_rejects_stale_claim -- --ignored --exact
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
