# Person foundation consumers — local batch 1

Owner: Agent OS. Baseline: `a2999e2`. Status: local implementation, no production
activation. This implements the existing
[combined handoff](../../../../docs/plans/active/person-agent-foundation-parallel-handoff.md#8-合并交接共同底座二花接入与欢迎闭环).

## Shared authority and version ownership

Password sessions and authenticated Gateway adapters resolve the existing Person.
Consumers lock the collaboration tenant and evaluate the existing appointment, duty,
grant chain, scope, decision mode and designated reviewer inside the final transaction.
The same lock serializes appointment changes and deferred execution. Management permits
delegation, not execution on behalf of an unprivileged person. Missing authority fails
closed; neither trainer allowlists nor welcome-specific grants are fallback authority.

Knowledge items bind an existing collaboration scope to an existing Space. Their
immutable bodies and version numbers live in `business_definition_versions`; additive
item/revision mappings hold ownership, applicability, effective intervals, withdrawal
and the granting authority. These inert definitions stay `shadow`, with no capabilities
or automation registration, so they cannot accidentally enter the Space scheduler.
Reading selects the latest applicable committed version at database time. Future
versions leave the current version in use; expiring case overrides fall back to a valid
default. Community sharing permits reading, not mutation or overriding principles.

Rule maintenance and cross-Agent requests use existing `work_items` and events. A
request retains authenticated initiator, exact rule input and source, expected version,
delegating entrypoint and target. Execution rechecks identity, authority and versions.
Idempotency conflicts cannot replace an earlier request. Acceptance, confirmation,
execution and failure remain distinct receipts. No new scheduler is introduced.

The synthetic conversation adapter freezes its first typed mutation input before
execution, including the expected version. Migration
`202609220005_foundation_turn_input.sql` adds nullable command/hash columns to existing
turn sources. An acknowledgement loss therefore reuses the original service command,
while a changed instruction under the same operation fails; only a hash of free-form
text is retained. General knowledge withdrawal targets one immutable revision, checks
current authority and the item version, increments that version, and retains an
idempotent receipt. Reads then select the latest remaining effective revision; cancelled
future changes leave the old rule in force.

## Isolation, interfaces and validation

Only the existing synthetic loopback Store is exposed in this batch. UI and tool APIs
share services; HTTP uses the existing password session and CSRF checks. The optional
local Unix broker requires a fixed configured Gateway, local peer and private bearer;
model arguments cannot supply Person, tenant, namespace or an arbitrary destination.
Every tool turn also matches a persisted authenticated ingress message by tenant,
platform, conversation type, conversation, sender and message reference. Memory uses
that source timestamp rather than broker arrival time; late unseen messages cannot
restore a stopped preference. Strict bounded JSON rejects duplicate keys at every depth.
Personal stay history is private-turn only. Group context provides a reply-style hint,
not permission to disclose personal memories. Adopted scopes and people permanently
refuse the legacy trainer path, including after a Gateway or identity link is revoked.
Production profile, credentials, sending, PMS and Feishu writes remain disabled.

The identity/memory and welcome migrations have separate design notes and file owners.
The shared migration is `202609220001_person_foundation_consumers.sql`. It is additive
and does not migrate or expand existing live authorizations. Stop the local listener to
roll back; retain records and audit evidence rather than deleting history.

Validation drives the password/tool consumers and real Postgres transactions: successful
local update, shared community reading, cross-building denial, scheduled version, replay
conflict, deferred execution after revocation, target unavailable/recovery and
designated confirmation. Welcome consumes this authority in its own transaction. Run the
targeted Rust/Postgres suites, tool contract tests, browser interactions, then the
applicable `pnpm check:pr:auto` tier. Synthetic model completions and external adapters
prove plumbing only; real model understanding and channels need later evidence.
