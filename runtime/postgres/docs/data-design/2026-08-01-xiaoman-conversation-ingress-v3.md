# Xiaoman Conversation Ingress V3

Date: 2026-08-01

## Decision

AgentOS authenticates and persists a minimal Feishu message before a poster tool call
can use it. Conversation authorization is resolved from Postgres policy and exact server
allowlists; it is never accepted from model arguments.

## Schema

- `conversation_policies` stores a versioned opaque conversation reference, `private`,
  `internal_collaboration`, or `external_community` audience class, allowed
  capabilities, initiation rule, return mode, status visibility, policy digest, and
  enabled state. At most one version is active per platform and conversation.
- `conversation_policy_actors` stores hashed actor references for reviewer policy. Raw
  user identifiers remain in the server allowlist used by the protected apply command.
- `poster_workflow_participants` snapshots requester and reviewer actor references for
  an accepted workflow and policy version.
- `feishu_message_ingress_nonces` stores nonce and payload hashes plus expiry for replay
  defense. It stores no raw message, chat, or user identifier.
- `feishu_message_ingress_receipts` binds an opaque source-message reference to the
  persisted message and records bounded duplicate counts.
- `qintopia_messages.messages` gains `sender_type`, `thread_root_message_id`, and
  `parent_message_id`. Existing rows are retained and backfilled compatibly.
- `poster_return_targets` gains group-aware audience, conversation reference, policy
  version, delivery mode, and thread anchor fields. Existing direct rows retain their
  primary keys and are backfilled as legacy private-chat targets.

All policy, participant, target, nonce, and receipt tables are revoked from `PUBLIC`.

## Invariants

- The ingress signature is HMAC-SHA256 over `qintopia-feishu-message-ingress-v3\n`, the
  decimal timestamp, `\n`, the lowercase hexadecimal nonce, `\n`, and the exact decoded
  body bytes, in that order. Base64 is transport encoding only and is not itself signed.
- A nonce is accepted once within the fixed clock window. A repeated Feishu message with
  a fresh envelope is a durable message-level duplicate, not a new message.
- Direct candidates require a `private` policy. Group candidates require an
  `internal_collaboration` policy, an explicit Bot mention, human sender type, and the
  independently enabled group feature.
- `external_community` and unclassified conversations cannot authorize poster work or
  draft delivery.
- Database policies cannot exceed exact server chat/user allowlists.
- One source platform, message id, and capability produce one poster workflow. The
  return target is bound to that source message, so concurrent requests in one chat do
  not overwrite each other's thread anchors.
- Workflow metadata and ordinary audit output contain only opaque conversation, actor,
  and message references.
- Migration does not delete or rewrite existing poster notification, review, revision,
  attempt, artifact, or work-item identities.
- Conversation notification never implies public or group-send authorization.

## Protected Policy Apply

`conversation-policy-apply --stdin` accepts bounded JSON only after exact owner approval
and database URL hash checks pass. It validates every raw chat and reviewer against
deployment allowlists, writes policy versions idempotently, and prints only counts,
versions, and hashes.

## Rollback

Disable the ingress and group feature switches first. Additive schema may remain for
audit and forward recovery. Do not drop pending or ambiguous ingress, notification, or
delivery state during runtime rollback.
