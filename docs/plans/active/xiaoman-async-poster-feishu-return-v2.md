# Xiaoman Async Poster And Feishu Return V2

> Superseded for new implementation by
> [Xiaoman Conversation-Aware Poster V3](xiaoman-conversation-aware-poster-v3.md). V2
> remains a direct-conversation compatibility contract for one release cycle and must
> not be extended to group intake or delivery.

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
- A permanently invalid target or artifact is terminalized as a sanitized failed
  notification before any attempt or external connection is created, so it cannot block
  later eligible notifications.
- Expired in-flight attempts become `ambiguous` and are never automatically replayed.
- The adapter revalidates and uploads the exact reviewed JPEG, then sends one card to
  the mapped originating direct conversation.
- The callback ingress verifies Feishu request authenticity before converting card
  actions into an internal review command.
- Hermes currently labels generic Feishu card actions as `group` even when the SDK
  callback originated in a P2P chat. The plugin therefore does not trust that derived
  label. It cross-checks the SDK callback chat and operator against the synthetic event,
  then the sidecar requires the same raw chat and actor to match the restricted
  `poster_return_targets` direct-conversation row before idempotency or review mutation.
- The deterministic `pre_gateway_dispatch` hook handles matching review cards before
  model or authorization dispatch and forwards only a bounded, locally signed envelope
  to the fixed Unix socket. Typed `/card` text cannot enter this path.
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

Local implementation and repository validation completed on 2026-07-31. The Xiaoman
plugin suite passed 68 tests, the default sidecar suite passed 453 tests, and the
all-features suite passed 459 tests with 16 guarded PostgreSQL tests ignored. Both
sidecar smokes passed, and `pnpm check:pr:auto` completed its quick and heavy Rust
tiers. The PostgreSQL integration target compiles with `postgres-integration-tests`,
including fact-gate claim prevention, notification-bound review idempotency, revision
handling, and zero group-send assertions.

PR #334 follow-up review found that the delivery claim query attempted to lock the
nullable side of a left join, permanent policy rejection rolled back to `pending`, and
the integration fixture referenced a non-existent `work_items.completed_at` column. The
follow-up keeps only notification and work-item rows in the claim lock, records a
sanitized terminal failure for pre-I/O policy or identity rejection, and aligns
completion updates with the actual work-item schema.

The follow-up also persists a hashed Feishu image-upload identity and an explicit
upload-accepted event before opening the card-send gate. Any later ambiguous failure
retains that non-sensitive identity without retaining the raw image key. The primary
`huabaosi-production` artifact name remains backward compatible; the compiled Xiaoman
adapter stays disabled until its independent owner approval, preflight, persistent
enablement, and timer activation gates pass.

Authorized poster-brief upserts are monotonic and fail closed: an existing pending
artifact with the same work item and content hash is promoted in place with complete
review provenance, while any incompatible persisted review state aborts the transaction
before authorization events or work-item completion.

The disposable PostgreSQL test was not executed locally because
`127.0.0.1:5432/qintopia_test` is unavailable; the guarded CI job remains the execution
gate. Production activation and real Feishu acceptance remain owner-run gates requiring
the reviewed Xiaoman Feishu app identity, permissions, credentials, callback endpoint,
target allowlists, and server secret configuration. No service, database, external
adapter, or group-send path was activated during implementation.

The callback bridge follow-up lives in the versioned Xiaoman `qintopia-tools` plugin
because the deployed Hermes Feishu adapter already exposes authentic SDK card objects to
`pre_gateway_dispatch`. It does not require a public callback route or model mediation.
Production activation must additionally prove the plugin resolves to the immutable
Release, the Xiaoman Hermes environment enables the hook, the callback key matches the
sidecar environment, and `hermes-gateway-xiaoman.service` is restarted and healthy
before external delivery is enabled.

The approved v0.2.61 live legacy-runner Bootstrap completed through Action `30625542550`
and request `deploy-20260731T110354Z-045bd2114114`. It installed the reviewed deploy
bundle while retaining runtime and commit SHA `045bd21`; the poster units remained
absent, both persistent poster flags remained absent, and Xiaoman was not restarted.
That evidence proves only the deploy-runner prerequisite. A normal production Release
deployment, persistent secret and allowlist configuration, explicit poster activation,
and real Feishu direct-message acceptance still require separate owner decisions.
