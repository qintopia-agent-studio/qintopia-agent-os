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

PR 2 is implementation-complete on an independent branch and awaiting replacement CI and
review. It adds the unified direct/internal-group intake, participant snapshots,
conversation-scoped status, first-valid revision and review-decision rules,
capability-registry routing, and durable group notification work. Its boundary remains
persistence and authorization only: it makes no Feishu call, activates no service,
deploys nothing, and writes no production database. PR 3 has not started.

The PR 2 local suite passes 479 default Rust tests, 485 all-features Rust tests with 20
protected PostgreSQL tests ignored by design, both warning-denied Clippy configurations,
plugin/MCP checks, runtime and deploy contracts, and `pnpm check:pr:auto`. Before the
later idempotency hardening, a fresh disposable PostgreSQL 18.4 database with pgvector
0.8.1 passed all three poster intake integration tests. The current head must still pass
the repository-supported `pgvector/pgvector:pg16` service job; the earlier PG18 result
is additional migration evidence, not a substitute for current CI.

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

Acceptance: the exact image reaches the original direct chat or internal-group thread
within 60 seconds; restart and uncertainty do not duplicate generation or delivery;
review authority and zero-publication invariants remain enforced.

## Release Sequence

1. Ship PR 1 and PR 2 with internal-group features disabled.
2. Install the PR 3 release with `QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED=0`.
3. Revalidate the existing direct path.
4. Rotate previously exposed database credentials and update the approved database hash
   before any production reactivation.
5. Apply one private and one internal-group policy through the protected command.
6. Run no-external-call preflight against the exact chat/user deployment ceilings.
7. Obtain a separate owner approval, enable one internal group, and run one canary.
8. Expand only after the direct and group evidence chains both pass.

## Validation

```bash
pnpm skills:qintopia-tools:check
pnpm mcp:collab:check
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

Disable group ingress and delivery before workers. Preserve policies, messages,
participants, workflows, notifications, attempts, and ambiguous outcomes. Never reroute
group results during rollback. Direct intake and unrelated short-running collaboration
capabilities remain independent.
