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

PR 1 implementation is complete on the feature branch as of 2026-08-01. It includes the
authenticated message-first ingress, additive policy and receipt migration, protected
policy apply command, strict Hermes session identity, message-scoped V3 return-target
identity, V2 direct compatibility, plugin manifest checks, and guarded PostgreSQL tests
wired into CI. Internal-group ingress remains disabled by default and the V3 poster
workflow still accepts direct conversations only.

Local plugin tests, default and all-features Rust suites, both warning-denied Clippy
configurations, Markdown, CI contracts, MCP regression, secret scanning, and package
checks pass. The guarded PostgreSQL tests compile locally, but execution requires the
CI-equivalent `pgvector/pgvector:pg16` service. The local image pull failed before
repository SQL ran, so the PR's disposable `qintopia_test` CI job remains the required
database execution proof.

PR 2 and PR 3 have not started. No Feishu call, image generation, group notification,
service activation, deployment, or production database write is part of PR 1.

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

Production acceptance requires one real direct request and one real internal-group
thread request. It must retain sanitized evidence for acceptance, exact-image return,
review persistence, and zero messages in external activity or member groups.

## Rollback

Disable group ingress and delivery before workers. Preserve policies, messages,
participants, workflows, notifications, attempts, and ambiguous outcomes. Never reroute
group results during rollback. Direct intake and unrelated short-running collaboration
capabilities remain independent.
