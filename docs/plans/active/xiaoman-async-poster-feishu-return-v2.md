# Xiaoman Async Poster And Feishu Return V2

## Outcome

One explicit poster-generation request in a trusted Xiaoman Feishu direct conversation
authorizes image generation for that workflow. It does not authorize publication.

The user-visible sequence is:

1. Xiaoman accepts the request within five seconds and returns a workflow id.
2. AgentOS persists the authorization, immutable brief, and image-generation work.
3. A complete, source-grounded brief proceeds without a second approval prompt.
4. The exact generated JPEG returns to the originating direct conversation as pending
   review.
5. The originating user may approve, request a modification, or abandon the workflow.
6. No group-send request is created without a separate explicit publication instruction
   that names the target group.

## Scope

- Trusted Hermes per-session Feishu direct-message intake over a local Unix socket.
- Postgres-backed idempotency, workflow state, origin correlation, delivery attempts,
  review actions, and revision requests.
- A feature-gated Feishu image/card delivery adapter with app, host, chat, and user
  allowlists.
- A signature-verifying Feishu card callback ingress that emits only a normalized,
  bounded internal callback.
- Restart-safe workers, systemd templates, observation, activation, and rollback gates.

## Out Of Scope

- Increasing the synchronous MCP timeout.
- Starting Huabaosi through a Hermes one-shot process.
- Allowing model payloads to choose chat ids, reviewers, or authorization state.
- Automatically publishing an approved image to an activity group.
- Enabling production delivery without a reviewed Xiaoman Feishu application identity,
  permissions, credentials, callback endpoint, and allowlists.

## Delivery Increments

### Increment 1: Trusted Intake

- The Xiaoman plugin reads platform, conversation type, chat, user, and message ids only
  from Hermes session context.
- It submits to the fixed AgentOS Unix socket with a four-second client timeout.
- Postgres creates one workflow per platform/source-message/capability identity.
- Missing trusted direct-message context fails closed without synchronous fallback.

Acceptance: duplicate submissions return the same workflow, and the collab MCP poster
capability returns a migration response without invoking Huabaosi.

### Increment 2: Brief Authorization And Durable Return Work

- The originating request records `poster_generation_authorized`.
- Only source-grounded direct-message briefs are auto-approved with
  `originating_generation_request` audit provenance.
- Direct-generation visual work is ineligible for preview or claim until the persisted
  fact gate is complete, source-grounded, and has no missing or conflicting fields; the
  worker validates the gate again before authorization.
- Automatic activity signals retain the existing human brief gate.
- Each pending generated-image artifact creates one durable
  `conversation_notification_request` and one notification record.

Acceptance: the manual path has no duplicate brief approval, automatic paths cannot
bypass review, and no notification or image approval creates group-send work.

### Increment 3: Feishu Delivery And Review

- The adapter persists a delivery attempt before opening an external connection.
- Expired in-flight attempts become `ambiguous` and are never automatically replayed.
- The adapter revalidates and uploads the exact reviewed JPEG, then sends one card to
  the mapped originating direct conversation.
- The callback ingress verifies Feishu request authenticity before converting card
  actions into an internal review command.
- Approve, modify, and abandon are actor-bound and idempotent. A modification plus the
  next trusted user instruction creates the next image request in the same workflow.
- One delivered notification can create only one bound review action. A second callback
  event id is deduped only when notification, artifact, actor, and decision all match.

Acceptance: fake Feishu tests cover token, upload, card send, duplicate delivery,
uncertain delivery, forged callback, duplicate callback, and wrong-user rejection.

## Release Gates

1. Intake, notification starter, and review state ship behind internal worker switches.
2. The Feishu adapter is compiled only with `xiaoman-feishu-poster-adapter` and remains
   disabled until the exact owner phrase and production configuration preflight pass.
3. Installation may place disabled services and timers. Only an explicit owner
   activation script may enable the external delivery timer.
4. Rollback disables the delivery timer first. It does not delete workflows, artifacts,
   targets, attempts, or audit events.

## Validation

Repository validation:

```bash
pnpm skills:qintopia-tools:check
pnpm mcp:collab:check
pnpm workflows:check
RUST_MIN_STACK=33554432 cargo test --manifest-path runtime/sidecar/Cargo.toml
pnpm check:pr:auto
```

Disposable Postgres acceptance must prove one workflow, one brief, one image request,
one notification, restart-safe delivery state, one review event per action, and zero
`group_message_request`, `send_executed`, or `external_published` rows.

Production acceptance remains incomplete until one real Xiaoman Feishu direct-message
request receives the acceptance reply, exact image review card, and persisted review
event while the activity group receives no message.

## Implementation Status

Local implementation and repository validation completed on 2026-07-31. The default
sidecar suite passed 451 tests, the all-features suite passed 457 tests with 13 guarded
PostgreSQL tests ignored, both sidecar smokes passed, and `pnpm check:pr:auto` completed
its quick and heavy Rust tiers. The PostgreSQL integration target compiles with
`postgres-integration-tests`, including fact-gate claim prevention, notification-bound
review idempotency, revision handling, and zero group-send assertions.

The disposable PostgreSQL test was not executed locally because
`127.0.0.1:5432/qintopia_test` is unavailable. Production activation and real Feishu
acceptance remain owner-run gates requiring the reviewed Xiaoman Feishu app identity,
permissions, credentials, callback endpoint, target allowlists, and server secret
configuration. No service, database, external adapter, or group-send path was activated
during implementation.
