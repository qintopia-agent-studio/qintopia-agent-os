# Xiaoman Conversation Ingress V3

Date: 2026-08-01 Status: accepted for staged implementation

## Decision

Xiaoman conversation-aware poster work uses the existing Hermes plugin SDK boundary.
Hermes remains the conversation runtime and is not forked. The versioned Xiaoman plugin
observes authentic Feishu SDK message events in `pre_gateway_dispatch`, submits a
bounded signed message to the local AgentOS intake socket, and then allows ordinary
Hermes dispatch to continue.

Postgres and AgentOS remain the fact and authorization source. A model tool call can
refer only to the current Hermes platform, chat, user, and message identity. It cannot
choose a conversation class, delivery target, reviewer, generation authorization, or
provider. AgentOS resolves those facts from the previously authenticated message,
conversation policy, deployment allowlists, and capability registry.

## Trust Flow

```text
Feishu SDK event
  -> Xiaoman pre_gateway_dispatch validation
  -> HMAC-SHA256 envelope over a fixed Unix socket
  -> message persistence
  -> Postgres conversation policy plus server allowlist
  -> AgentOS poster workflow and participant snapshot
  -> capability-registry work-item routing
```

The ingress accepts only these candidates:

- a human message in a policy-enabled Xiaoman direct conversation;
- a human message with an exact Xiaoman Bot mention in a policy-enabled internal
  collaboration group.

Bot messages, unmentioned group messages, external communities, unclassified chats, and
malformed SDK bindings are rejected. An ingress failure is bounded and never blocks the
normal Hermes response path.

## Identity And Policy Boundaries

- The plugin and sidecar use a dedicated ingress HMAC key. The key is not a model
  parameter and is distinct from the Feishu card-callback key.
- The signed envelope contains a timestamp and one-use nonce. AgentOS enforces a short
  clock window and persists only nonce and payload hashes needed for replay defense.
- The plugin normalizes only the reviewed message fields. It never forwards or stores
  the complete Feishu SDK payload.
- `conversation_policies` is the auditable business gate. Exact server chat and user
  allowlists are a deployment ceiling that a database policy cannot broaden.
- Raw chat and user identifiers remain only in restricted target mappings, message
  storage, and server configuration. Normal workflow metadata uses opaque hashes.
- Policy participants are snapshotted when a workflow is accepted. Later policy edits do
  not change the review authority of existing work.

## Conversation And Delivery Semantics

Direct results return to the originating direct chat. Internal-group results return to
the originating thread root. A thread reply is an internal task notification, not a
publication. It must never create `group_message_request`, `send_executed`, or
`external_published` facts.

The original explicit generation instruction authorizes image generation only. The
requester and the workflow's snapshotted reviewers may review internal-group work;
direct work remains requester-only. Public or community delivery always requires a
separate instruction, target, and publication authorization.

## Multi-Agent Boundary

Xiaoman is the only Feishu-facing coordinator. AgentOS routes image work through the
capability registry to Huabaosi or another reviewed provider. Models cannot select the
provider, and agents do not coordinate by recursively sending Bot messages in Feishu.

## Compatibility And Rollout

The existing V2 direct poster request remains accepted for one release cycle. It never
gains group access. V3 group intake and group delivery have separate feature switches
and remain disabled through the first release. The rollout is split into:

1. authenticated ingress, policy data, protected policy apply, and strict session
   identity;
2. unified direct/group poster intake, participant authorization, and group workflow
   rules;
3. thread delivery, group review callbacks, deployment contracts, activation,
   observation, and rollback.

No stage may silently fall back to a synchronous Huabaosi Hermes one-shot, a main-chat
message, a direct-message resend, or a public send.

## Rejected Alternatives

- Increasing the MCP timeout keeps a slow task on the conversational request path and
  does not provide durable recovery or idempotency.
- A Hermes fork creates a second runtime ownership boundary for behavior already exposed
  by the plugin SDK.
- Letting the model provide chat type, target, reviewer, or provider makes prompt data
  an authorization source.
- Independent Agent Bot identities in the group create message loops and bypass the
  AgentOS work-item audit path.

## Rollback

Disable group ingress and delivery before stopping related workers. Retain messages,
policies, participants, targets, receipts, attempts, and ambiguous outcomes for audit.
Never reroute a failed group result to a direct chat or main group timeline during
rollback.
