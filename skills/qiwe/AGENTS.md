# QiWe integration instructions

<!-- guidance-scope: skills/qiwe -->

These instructions supplement the [root contract](../../AGENTS.md) for QiWe protocol
parsing, webhook ingress, reply handling and callback integration.

## Map and required reading

- [Package guide](README.md).
- [Complete channel constraints](docs/channel-agent-contract.md).
- [Complete media constraints](docs/media-agent-contract.md).
- [Sidecar rules](../../runtime/sidecar/AGENTS.md).
- [Hermes rules](../../runtime/hermes/AGENTS.md).
- [Deploy rules](../../deploy/AGENTS.md).

Read the relevant detailed contract before editing a parser, send path, callback,
capability gate or production observation. This summary grants no new permissions.

Cross-component rules are explicit dependencies. Sidecar persistence, Hermes routing and
deployment changes require their own scoped instructions as well.

## Validation commands

Run from the repository root:

- `pnpm test:qiwe`
- `pnpm runtime:hermes:check`
- `pnpm deploy:contracts:check`
- `pnpm check:pr:auto`

Use bounded fixture payloads and synthetic transports. Do not validate a parser change
by sending to a real contact, replaying a production callback or invoking an installed
worker's apply mode.

## Authentication and ingress

Trust authenticated ingress only at its actual transport boundary. Publisher JSON cannot
assert trust. Keep authenticated raw subjects distinct from legacy subjects.

Keep producer and consumer identities separate. Never place NATS credentials in URLs;
use their distinct fixed auth files and preserve publication/consumption ACLs.

Normal replies must remain independent of NATS, Sidecar and Postgres availability. The
explicit authenticated system-event durable-capture exception retains its
complete-envelope 1.5-second bound and bounded 503 behavior when PubAck is missing.

Parse bounded JSON and preserve exact event/identity semantics. Unknown callback shapes
must not become a path for retaining raw secrets or private message text.

Sanitize asynchronous callback credentials before persistence or dead-letter handling.
Preserve the approved callback id/hash and credential-shape rules; do not log unknown
field names, values, raw identifiers, filenames or response bodies.

## Reply and send boundaries

Preserve group versus direct-message routing. Outbound suppression must match narrow
complete internal-message templates, not broad keywords in user content.

Never send raw provider retry traces, runtime diagnostics or internal tool/process
messages to conversations. Keep error handling aligned with the owning contract.

A timeout after external I/O may mean the provider accepted the operation. Preserve
ambiguous outcomes; do not turn them into definite no-send results or automatic retries.
Status, claim and attempt identity must remain transaction-bound.

Keep direct-recipient and group/media allowlists, consent and capability checks. Do not
bypass them because an account token or device id is present.

Configuration identity, API transport, plugin discovery and real user reply acceptance
are separate checks. Do not claim a send succeeded from HTTP status alone when a
provider also returns a business result code.

## Media and callbacks

Upload, callback and send have separate gates. Read the full media contract for approved
artifact identity, canonical hashes, byte size, claim ownership and expiry.

Persist attempts before external I/O. Do not reclaim ambiguous uploads or sends as safe
retries. Preserve the distinct expiry rules for awaiting callbacks and already-sending
attempts.

Callback input is bounded stdin, not CLI or environment payload. Default builds fail
apply before secret-bearing input, database or network access as specified.

Feature flags, reviewed artifact profiles, explicit owner approval and persistent
enablement are independent requirements. Mixed staging/production builds cannot
substitute one approval boundary for another.

The bounded text worker remains specific to its approved morning-brief capability; do
not reuse it as a general sender. Production enablement is not implied by an adapter
import, installed unit or successful dry-run.

## Evidence and completion

Keep credential values, raw identities, chat text, media URLs and provider message ids
out of retained reports. Use fixed schemas, counts and allowed metadata.

Record fixture and real-process compatibility results separately from live user
acceptance. User-triggered conversation checks do not authorize proactive tests or
historical task replay.
