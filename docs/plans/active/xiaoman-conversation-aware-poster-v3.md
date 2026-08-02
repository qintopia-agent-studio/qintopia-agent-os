# Xiaoman Conversation-Aware Poster V3

## Outcome

An explicit poster request in a trusted Xiaoman direct conversation is accepted within
five seconds and returns to that direct conversation for review. An explicit Xiaoman
mention in an authorized internal collaboration group follows the same durable AgentOS
workflow and returns to the original thread. External, community, unclassified, bot, and
unmentioned group traffic cannot enter the workflow or receive a draft.

Generation authorization is not publication authorization. Xiaoman remains the sole
visible coordinator; AgentOS assigns Huabaosi and other providers through registered
capabilities.

Architecture decision:
[xiaoman-conversation-ingress-v3.md](../../architecture/xiaoman-conversation-ingress-v3.md).
Data design:
[2026-08-01-xiaoman-conversation-ingress-v3.md](../../../runtime/postgres/docs/data-design/2026-08-01-xiaoman-conversation-ingress-v3.md).

## Implementation Status

PR 1 was merged to `master` as `f1e0347b0b983f628f42c8db65b9017c8f251c3c` on 2026-08-01.
It includes the authenticated message-first ingress, additive policy and receipt
migration, protected policy apply command, strict Hermes session identity,
message-scoped V3 return-target identity, V2 direct compatibility, plugin manifest
checks, and guarded PostgreSQL tests wired into CI. Internal-group ingress remains
disabled by default and the V3 poster workflow still accepts direct conversations only.

PR 1's merge checks passed the plugin, default and all-features Rust, both
warning-denied Clippy, Markdown, CI-contract, MCP, secret-scan, package, and disposable
PostgreSQL 16 tiers.

PR 2 was merged to `master` as `05d648ebb328b573ae7ef860c2520bf9c6119f1a` on 2026-08-02.
It adds the unified direct/internal-group intake, participant snapshots,
conversation-scoped status, first-valid revision and review-decision rules,
capability-registry routing, and durable group notification work. Its boundary remains
persistence and authorization only: it makes no Feishu call, activates no service,
deploys nothing, and writes no production database.

PR 2 passed 479 default Rust tests, 485 all-features Rust tests with 20 protected
PostgreSQL tests ignored by design, both warning-denied Clippy configurations,
plugin/MCP checks, runtime and deploy contracts, `pnpm check:pr:auto`, and the
repository-supported PostgreSQL 16 CI tier. PR 3 now owns only the separately gated
Feishu delivery, callback runtime ceiling, and deploy/rollback boundary.

PR 3 was merged through the scoped delivery changes completed by `6d4c283` on
2026-08-02. It adds exact direct-chat and internal-group thread delivery, callback
runtime ceilings, separate scope-pinned preflight/service/timer units, fake Feishu and
fake systemd acceptance, and guarded production observation, activation, and rollback.
It did not mutate a production database, call real Feishu, or enable either delivery
timer when merged.

The PR 3 local suite passed 482 default Rust tests, 488 all-features Rust tests with 21
protected PostgreSQL tests ignored by design, both warning-denied Clippy configurations,
deploy and release-installer contracts, and `pnpm check:pr:auto` quick and heavy tiers.
The protected local PostgreSQL tier was unavailable because `qintopia_test` was not
running on `127.0.0.1:5432`; the required PostgreSQL 16 CI job includes the new scoped
claim and stale-recovery integration test.

Successive PostgreSQL 16 runs exposed three distinct stable-identity boundaries:
presentation-only brief fields, canonical activity-signal source provenance, and the
artifact-scoped first-valid poster revision race. The validator now keeps capability,
type, parent, requester/provider, source, payload, and policy bindings strict, while
allowing only presentation replay drift and the narrowly validated first-writer revision
contenders documented below. Replacement PostgreSQL 16 CI remains required.

The PR Reviewer Guide also identified that rejected or duplicate card-callback dry-runs
could write mutation audit rows before the apply gate. Callback preview now performs the
same validation without any audit, review-action, artifact, or workflow mutation; the
real callback ingress continues to audit rejected and duplicate apply requests.

PR 2 locks these implementation rules:

- V3 session classification, return mode, thread anchor, and policy identity come only
  from the persisted ingress receipt and message. V2 remains direct-only.
- Each V3 workflow snapshots exactly one requester and the policy-version reviewers.
  Status visibility uses the conversation policy, while every mutation uses the
  immutable participant snapshot.
- A source message remains the workflow idempotency boundary. A generated-image artifact
  is the revision boundary, so only the first valid revision instruction can create its
  next image work item.
- Direct and internal-group workflows may create durable conversation notifications, but
  the existing direct adapter remains direct-only. Group notifications cannot be
  delivered until PR 3 adds the separately gated thread adapter.
- Capability-provider bindings are resolved from the AgentOS capability registry on
  apply. Neither the model nor the poster intake payload supplies a provider.
- Poster callback `--dry-run` is read-only even for target mismatch, unauthorized actor,
  runtime-allowlist denial, duplicate callback, or conflicting decision outcomes.

PR 3 locks these implementation rules:

- The existing delivery worker selects both direct and internal-group notifications from
  the persisted target. It never infers a target from a callback or model payload.
- A disabled internal-group switch leaves group notifications pending and invisible to
  the delivery claim scan. It does not fail them, reroute them, or block direct work.
- Direct and internal-group delivery retain one queue and worker implementation but use
  separate scope-pinned systemd services and timers. The direct timer can claim only
  direct rows; the default-disabled group timer can claim only group rows after separate
  owner activation. Each service has a scope-specific preflight, so group-only
  configuration failure cannot block direct delivery. Group activation and rollback
  never stop or restart direct delivery.
- Direct delivery keeps the reviewed chat-send endpoint. Group delivery uses only the
  persisted thread root with Feishu reply, `reply_in_thread=true`, and a stable
  notification-derived `uuid`; there is no main-timeline or direct-message fallback.
- The upload attempt is persisted before image I/O and upload acceptance is persisted
  before the reply send gate. Any uncertain external outcome is terminal `ambiguous`
  with automatic retry disabled.
- Group callbacks must pass the immutable participant and policy-version checks from PR
  2 plus the current group feature switch and exact deployment chat/user allowlists.
- Group activation requires ingress and delivery to share the same exact chat/user
  ceiling, with every allowed user also covered by the operations reviewer ceiling, so
  no accepted requester can receive an unusable review card.
- Any group timer enable, restart, or final-observation failure immediately disables the
  group timer and stops its worker without mutating the direct timer.
- The existing V3 tables already carry the required target, snapshot, attempt, and
  idempotency facts. PR 3 adds no schema migration and does not create another queue.

The remaining production closeout is intentionally one reviewed PR and one Release. It
owns the missing production configuration transaction, the direct-only ingress
configuration correction, and the operator runbook needed to finish both the direct
acceptance and one internal-group canary on that same immutable Release. It must not be
split into separate HMAC, Bot identity, allowlist, database-hash, or group-switch
releases.

The closeout configuration entrypoint reads bounded JSON from stdin, writes only the
fixed sidecar and Xiaoman Hermes environment files, preserves their existing ownership
and modes, updates both through one locked transaction, and emits only counts and
boolean evidence. It may accept a newly rotated database URL through stdin and update
the fixed production database hash bindings in the same transaction. It must never print
the URL, secrets, chat ids, user ids, file contents, or backup contents. It does not
restart services, call Postgres or Feishu, or enable a delivery timer.

Direct ingress does not use a Bot open id and therefore must not require one while the
internal-group switch is `0`. Group configuration continues to require the exact Bot
open id, chat and user ceilings, and reviewer ceiling before the separate group
activation can succeed. This is a scope correction, not a weaker group boundary.

## Delivery Plan

### PR 1: Authenticated Ingress And Policy Foundation

- Add the signed `feishu_message_ingest` V3 operation with timestamp, nonce, bounded
  payload, exact allowlists, and Postgres policy revalidation.
- Persist the minimal normalized message, replay receipt, policy data, message thread
  fields, and additive poster target fields.
- Add guarded `conversation-policy-apply --stdin` with exact owner approval and database
  hash binding.
- Extend Xiaoman `pre_gateway_dispatch` without changing Hermes core. Review cards keep
  their current deterministic path; ordinary authentic messages are persisted before
  model dispatch and never block normal replies.
- Make poster identity reads use `gateway.session_context` only. The V2 direct request
  remains a one-release compatibility path while authenticated ingress is disabled and
  cannot acquire group access. Once V3 ingress is enabled, failure never downgrades to
  V2.
- Keep internal-group intake and every Feishu delivery path disabled.

Acceptance: signed direct ingest and durable dedupe pass unit and guarded Postgres
tests; forged signatures, stale/replayed envelopes, process-environment identity
forgery, bot messages, malformed SDK events, unmentioned groups, and disabled groups
fail closed. No Feishu or external call occurs.

### PR 2: Unified Poster Intake And Participants

- Resolve direct/group conversation type and policy from the persisted V3 message.
- Derive idempotency from platform, source message, and capability; bind a unique
  message-scoped return target.
- Snapshot requester and policy reviewers into `poster_workflow_participants`.
- Implement internal-group status visibility, actor-bound modification, first valid
  revision instruction, and first final review decision semantics.
- Route provider work only through the AgentOS capability registry.

Acceptance: concurrent chats, users, groups, threads, and workflows do not cross; one
source message creates one workflow, brief, image request, and notification; every
unauthorized mutation is audited and rejected; zero publication facts exist.

### PR 3: Thread Return And Group Review

- Add the feature-gated Feishu thread-reply adapter, same-byte image upload, stable
  delivery idempotency, and authenticated group review callbacks.
- Add fake Feishu coverage, preflight, systemd units, release installation, observation,
  activation, and rollback contracts.
- Fail or mark ambiguous without falling back to the group main timeline or a direct
  chat.

The release installer continues to install all poster units without enabling them. The
existing poster activation brings up the direct path and explicitly leaves the group
timer stopped. A separate internal-group activation requires both the sidecar and
Xiaoman plugin group switches, authenticated V3 ingress, matching local ingress/callback
keys, immutable plugin binding, a successful no-network preflight, and proof that direct
delivery is active while group delivery is stopped. It enables only the group timer. Its
rollback disables only group intake/delivery while the direct timer remains active.

Acceptance: the exact image reaches the original direct chat or internal-group thread
within 60 seconds; restart and uncertainty do not duplicate generation or delivery;
review authority and zero-publication invariants remain enforced.

## Release Sequence

1. PR 1, PR 2, PR 3, and the trusted-session binding fix are already merged. Do not
   create another feature slice for production configuration.
2. Merge and publish one final closeout Release containing the protected configuration
   transaction and direct-only Bot identity correction.
3. Have the database owner create the replacement production credential, then stream the
   replacement URL and approved hash into the release-local configuration entrypoint.
   Keep the previous credential valid until the same-Release service reload succeeds;
   revoke it immediately after that proof.
4. Apply the `direct` desired state with the group switch fixed at `0`. The entrypoint
   reuses the reviewed direct chat/user ceiling, creates or rotates the dedicated
   ingress HMAC without printing it, and binds all Xiaoman poster settings to the exact
   Release and database hash.
5. Use one same-SHA deploy request to reload the fixed system service family after the
   database URL rotation. This is not another PR or Release.
6. Run the release-local no-external-call direct preflight, obtain the existing direct
   activation approval, and complete one real private-message request and review.
7. Stream one reviewed internal-group configuration into the same entrypoint, apply one
   private and one internal-group policy through `conversation-policy-apply --stdin`,
   and run the disabled-state group observation. No new build or Release is required.
8. Obtain the separate owner approval, activate one group on the same Release, and run
   one real thread canary. Expand only after the direct and group evidence chains both
   pass.

## Validation

```bash
pnpm skills:qintopia-tools:check
pnpm mcp:collab:check
cargo test --manifest-path runtime/sidecar/Cargo.toml poster_delivery::tests
cargo test --manifest-path runtime/sidecar/Cargo.toml poster_notification::tests
node tools/deploy/test-xiaoman-feishu-internal-group-production.mjs
RUST_MIN_STACK=33554432 cargo test --manifest-path runtime/sidecar/Cargo.toml
cargo test --manifest-path runtime/sidecar/Cargo.toml --all-features
pnpm check:pr:auto
```

The guarded PostgreSQL smoke must use a literal loopback `qintopia_test` database and
must prove migration backfill, policy double gates, message/workflow idempotency,
participant snapshots, review authority, restart safety, and zero
`group_message_request`, `send_executed`, and `external_published` facts.

For PR 2, the focused local database command is:

```bash
QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE=1 \
QINTOPIA_SIDECAR_DATABASE_URL=postgres://postgres:qintopia_test@127.0.0.1:55432/qintopia_test \
RUST_MIN_STACK=33554432 \
cargo test --manifest-path runtime/sidecar/Cargo.toml \
  --features postgres-integration-tests \
  operations_intake::tests::postgres_ -- --ignored --test-threads=1
```

Production acceptance requires one real direct request and one real internal-group
thread request. It must retain sanitized evidence for acceptance, exact-image return,
review persistence, and zero messages in external activity or member groups.

## Rollback

Set the persistent group switches to `0`, run the guarded group rollback, and verify the
group-disabled observation. The rollback stops only the scope-pinned group timer,
reloads Xiaoman without group intake, and proves the direct timer remained active.
Preserve policies, messages, participants, workflows, notifications, attempts, and
ambiguous outcomes. Never reroute group results during rollback. Direct intake and
unrelated short-running collaboration capabilities remain independent.
