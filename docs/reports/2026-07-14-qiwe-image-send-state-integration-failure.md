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

## PR #119 Callback File Identity Gap

The post-CI manual review of commit `684ba87` found a file-integrity gap not reported by
the Reviewer Guide. The callback parser required a JPG filename, positive size, and all
credential fields, but the send gate did not compare the callback's filename, `fileMd5`,
or `fileSize` with the approved `generated_image`. A correctly correlated but misrouted
or inconsistent callback could therefore provide credentials for a different image.

The root cause was treating `requestId` correlation as sufficient file identity. It
proves which asynchronous request the callback claims to answer, but it does not prove
that the returned file credentials describe the immutable JPEG reviewed by AgentOS.

The repair computes a canonical MD5 from the exact final JPEG bytes after deterministic
PNG-to-JPEG conversion and same-byte media readback. Huabaosi stores that MD5 with the
existing SHA-256 and byte size in both artifact metadata and the creation event. Human
approval now requires those facts to match. Upload acceptance snapshots the approved MD5
and byte size in `qiwe_image_send_attempts`; callback processing compares the callback
filename, canonical MD5, and byte size with the unchanged artifact and attempt before
committing `sending`. A mismatch rolls back the callback transaction and makes no
external send call. Callback AES keys, file ids, raw payloads, and filenames remain out
of Postgres, events, logs, and reports.

Validation covers the deterministic MD5 metadata, approval denial for malformed MD5,
state migration constraints, a disposable PostgreSQL callback mismatch that leaves the
send gate closed, the matching callback/idempotency path, the guarded apply smoke
fixture, default and staging-feature Clippy, and the full Rust suite. Real callback
field format still requires owner-approved isolated staging evidence; an uppercase or
otherwise noncanonical provider MD5 fails closed rather than being normalized silently.

This change does not enable the staging Cargo feature, install a listener/service/timer,
contact QiWe, write Feishu, or send a message. Production artifacts still exclude the
live adapter.

## PR #119 Dry-run Allowlist Drift

The Reviewer Guide for commit `bc3fe1d` found that `preview_ready_work_item` validated
the HTTPS JPEG shape and SHA-256 but did not apply the current exact target-group and
media-host allowlists. A dry-run could therefore report a request as previewable even
though apply would reject it.

The repair makes preview load only the two non-secret allowlists needed for this policy
check and pass them into the state layer. Preview now selects the artifact MD5, byte
size, and target group and reuses the same `validate_claim_boundary` function as apply.
It still performs no claim, Postgres write, or network request. The disposable
PostgreSQL test proves an allowlisted request previews successfully while a different
group or media host fails before claim.

The production boundary is unchanged: this improves read-only diagnostic fidelity and
does not compile or enable the live adapter, contact QiWe, or send a message.

## PR #119 Explicit Staging Owner Gate

The Reviewer Guide for commit `da53a7a` correctly treated the new executable upload and
callback commands as a production-adjacent external-send boundary. The default binary
already excluded live helpers at compile time, artifact/deploy checks rejected the
staging feature, and staging code required enablement, Webhook readiness, credentials,
and exact allowlists. However, a manually built staging-feature binary had no separate
command-entry proof that the one-shot run had received the owner decision documented by
the plan.

Both staging apply commands now require the exact
`QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL=approved-staging-qiwe-image-send` phrase
after the enable check and before adapter configuration, Postgres access, claim
mutation, or network access. The phrase is intentionally absent from production
configuration and is provided only with a reviewed one-shot staging command. Unit tests
reject missing or different values. This gate reduces accidental execution; it does not
replace the compile gate, OS access control, secret handling, allowlists, staging
evidence, or owner review.

No listener, service, timer, production feature build, or real external call is added.

## PR #119 Upload Attempt Lifecycle Gap

### Observed Evidence

The Reviewer Guide for commit `fec272e` found that an upload accepted by QiWe could be
repeated if the worker stopped before persisting the returned request-id hash, because
stale claims without an attempt were requeued.

### Root Cause

The attempt lifecycle began after external upload acceptance, so Postgres had no durable
fact proving that a claimed worker might already have opened the socket.

### Resolution

The claim transaction now creates an `uploading` attempt containing the immutable
artifact, target, claim, and final JPEG identity before external I/O. Acceptance updates
that same row with the request-id hash. Known rejection becomes `failed`; transport,
persistence, crash, and stale-claim uncertainty become terminal `ambiguous` with
`automatic_retry_allowed=false`. Legacy unrecorded claims are also terminalized rather
than requeued. Invalid preview policy also emits only a fixed sanitized action status
before failing, preserving the separately repaired preview boundary.

### Validation

The disposable PostgreSQL CI job runs the existing state tests plus these exact recovery
tests:

```text
cargo test --manifest-path runtime/sidecar/Cargo.toml --features postgres-integration-tests qiwe_image_send_state::tests::postgres_qiwe_send_state_terminalizes_stale_upload_and_send -- --ignored --exact
cargo test --manifest-path runtime/sidecar/Cargo.toml --features postgres-integration-tests qiwe_image_send_state::tests::postgres_qiwe_send_state_terminalizes_legacy_unrecorded_claim -- --ignored --exact
cargo test --manifest-path runtime/sidecar/Cargo.toml --features postgres-integration-tests qiwe_image_send_state::tests::postgres_qiwe_send_state_rejects_stale_claim -- --ignored --exact
```

The idempotency integration also asserts that claim creates one `uploading` row with a
null request hash, acceptance preserves its attempt id, and preview rejects a disallowed
group or media host. Default and all-feature tests and warning-denied Clippy remain
required.

### Remaining Boundary

No real provider, media, QiWe, Feishu, production Postgres, service, listener, or timer
is used or enabled. Production artifacts still exclude `qiwe-staging-adapter`.

### Follow-Up Owner Action

Keep PR #119 draft until its replacement head passes the disposable PostgreSQL job, both
Rust feature-set checks, and a fresh Reviewer Guide. Staging still requires an
owner-approved final JPEG, callback evidence, isolated group, secrets, and rollback.

## PR #119 Command Entry Security Review

### Observed Evidence

The review identified both QiWe CLI variants as production-adjacent external-send
entrypoints and required proof that disablement, allowlists, credential handling, and
staging approval are enforced by code rather than timer absence or documentation.

### Resolution

Default production artifacts exclude the non-default `qiwe-staging-adapter` feature, and
deploy preflight rejects explicit or all-feature production builds. Upload apply returns
`staging_adapter_not_compiled` before configuration, Postgres, claim mutation, or
network access. Callback apply now returns the same fixed failure before even reading
stdin. In a staging-feature build, the enable flag is checked before callback intake;
the exact one-shot owner phrase is required next, then the reviewed API/media/group
allowlists and webhook-ready gate are validated before callback intake or upload
database access. Live execution then keeps callback credentials in memory and rechecks
immutable JPEG identity before the send gate.

### Validation And Boundary

Default-feature tests invoke both apply entry functions without database configuration;
upload fails before database/network access and callback fails before stdin/database/
network access. Default and all-feature warning-denied Clippy, production artifact
feature checks, deploy preflight, fake-server tests, and the disposable PostgreSQL state
tests remain required. No staging binary is built or deployed, no timer/listener is
installed, and no real QiWe, media, provider, Feishu, or production Postgres call
occurs.

Direct CLI entry checks also failed both staging-feature apply commands with the fixed
owner-approval error when enablement was `1` but the phrase was absent. With the exact
phrase present but adapter configuration absent, both emitted sanitized
`adapter_not_configured` reports with no callback received and no external request,
before stdin or database access.

### Follow-Up Owner Action

Do not build or run the staging feature until the owner approves callback evidence, an
isolated target group, staging secrets and database, the exact command, and rollback.
Production enablement remains a separate reviewed release decision.
