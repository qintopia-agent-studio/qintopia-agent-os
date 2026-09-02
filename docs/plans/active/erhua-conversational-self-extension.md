# Erhua Conversational Self-Extension

Updated: 2026-08-15

## Goal

Let a trusted administrator describe a reusable business rule or schedule to Erhua in
the current group, review one bounded proposal, and activate it without adding a
group-specific workflow or cron file to the repository.

This delivery implements the reusable control plane, bounded event interpreter, generic
dispatcher, deterministic executor, official-document research handoff, and guarded
release path. Every production-affecting capability remains disabled by default. The
initial schema, ingress authentication, runtime, and release-policy rollout requires one
manual owner-reviewed Release before any Space automation can be activated.

## Scope

- Treat the existing `qintopia_messages.conversations.id` as `space_id`; one QiWe room
  is one Space in v1.
- Add versioned Space policy, business, automation, and provider event-mapping
  definitions in Postgres.
- Resolve Space and actor identity from trusted gateway session values, never tool
  arguments.
- Reuse `work_items`, `artifacts`, and `work_item_events` for proposal, confirmation,
  evidence, status, and audit.
- Keep `qintopia_space_change_prepare`, `qintopia_space_change_confirm`, and
  `qintopia_space_change_status` independently available through their configuration
  review boundary. Existing QiWe and Erhua `qintopia-tools` capabilities remain
  registered; ordinary group invocations are governed by the active current-Space policy
  when enforcement is enabled.
- Keep all new definitions inactive until an authorized actor confirms the exact
  proposal digest with a short-lived code.

Event interpretation, generic schedule dispatch, official-document research, and
external delivery build on these contracts in separate modules. They add no
business-specific columns and cannot bypass Space authorization.

## Trusted Boundary

The tool payload never accepts a platform, conversation id, room id, target id, actor
id, or person id. The Erhua wrapper reads `gateway.session_context` and forwards the
trusted platform, conversation, requester, and source-message values over the existing
mode-`0600`, bounded Unix socket.

The sidecar resolves or creates the current `conversations` row and resolves the actor
through `qintopia_identity.channel_identities.person_id` using the same platform,
conversation, and channel user id. Unlinked identities fail closed.

Confirmation requires an active membership with either:

- `community_key = 'space:<space uuid>'` and role `space_admin` or `business_admin`; or
- for the first Space administrator only, `community_key = 'qintopia'` and role `owner`
  or `admin`.

The confirmation code is bound to actor, Space, proposal digest, and expiry. The current
authenticated message receipt must also contain exactly `确认 <8位确认码>` after QiWe
mention stripping; an ordinary message, a negation, or a code found only in chat history
cannot authorize confirmation. The intake service reloads that text from Postgres by the
trusted message id instead of accepting it as a tool argument. Only a random salt and
password hash are retained, expiry is ten minutes, and failed attempts are bounded.
Status reads are scoped by trusted Space before returning sanitized proposal state.

## Declarative Change Set

`prepare(intent)` accepts one bounded object containing a summary and one or more
declarative changes. Allowed resources are `space_policy`, `business_definition`,
`automation_definition`, `definition_operation`, and `channel_event_mapping`. The
`definition_operation` shorthand is limited to activating the current shadow automation,
pausing one active automation, or restoring one historical automation version in the
current Space. The sidecar, not the model, resolves every operation. Activation
digest-binds the current automation stream head and its exact business and event-mapping
rows, then revalidates those bindings at confirmation. Pause and rollback are expanded
to a complete version-bound automation before proposal hashing. None of the operations
asks the model to reconstruct stored business inputs, schedules, timezones, or event
bindings. The service rejects destination fields, credentials, arbitrary URLs, SQL,
executable code, and unknown top-level fields.

Business definitions support `deterministic` and `agent_turn` execution modes.
Automations bind a business definition to either an `event` or `schedule` trigger.
Provider event mappings are reusable across Spaces, but their selector/extractor output
is limited to canonical event facts and cannot contain a destination room. Runtime
routing always derives the event room, resolves it to `conversations.id`, and selects
only active automations from that Space.

Raw QiWe events retain a separate `ingress_auth_verified` fact, but publisher JSON can
never set it. After the internal ingress header passes constant-time verification, the
adapter durably publishes only to the distinct authenticated raw subject using its
producer auth file. The sidecar uses separate consumer credentials and derives trust
only from that received subject while its explicit trusted-subject gate is enabled.
Legacy-subject payloads claiming `true` remain unauthenticated. Production activation
must first prove anonymous publish denial and the distinct producer/consumer subject
ACL. Mapping promotion joins the observation back to the exact raw-event row and
requires the same Space, provider, and authenticated ingress.

Ordinary chat messages keep the existing fast-response, best-effort NATS capture path.
The default-off `QIWE_SYSTEM_EVENT_DURABLE_CAPTURE_ENABLED=1` exception applies only to
authenticated system events. Every system event in one `data[]` envelope must receive a
valid authenticated-raw-subject JetStream PubAck within one shared 1.5-second budget;
any missing, negative, malformed, or timed-out acknowledgement returns a fixed HTTP 503
so Qiwe can retry. The adapter refuses to start this mode unless webhook authentication
and NATS capture and file-based producer authentication are enabled. `Nats-Msg-Id` and
the persisted provider event reference keep the retry idempotent.

When Space turn-policy enforcement is enabled, missing or malformed trusted-session
fields fail closed instead of falling back to the legacy unscoped tool path. Existing
Hermes Kanban complaint task ids do not carry server-verifiable Space ownership, so
Space-governed update and follow-up tools reject those ids. Complaint creation is bound
to the trusted current room, actor, and message. New conversational businesses use
Space-bound `work_items`; no second ownership store is introduced for legacy Kanban.

## Version And Activation Rules

Each definition stream has monotonically increasing versions and a canonical SHA-256
digest. Status is one of `draft`, `shadow`, `active`, `paused`, or `retired`; a partial
unique index permits at most one active version per stream.

Prepare creates an `awaiting_review` `space_change_request` and a pending
`space_change_proposal` artifact. Confirm locks the proposal and work item, validates
authorization and the code binding, retires the previous active version, inserts the
next version, and records bounded audit events in one transaction. No external action
runs inside confirmation.

Automation dependencies are materialized before the proposal digest is calculated. A
definition in the same proposal is pinned by its digest; an existing business or event
mapping is pinned by id, digest, and stream-head version. Confirmation rematerializes
those bindings and rejects drift. A pause or rollback shorthand additionally pins the
automation stream head and holds its stream lock while confirming, so an older proposal
cannot overwrite an intervening automation version. Replacing an active business or
provider mapping is rejected while an active or shadow automation still references it
unless the same proposal migrates or pauses that automation; a mapping referenced from
another Space cannot be retired from the current Space.

New Spaces start with no active Space policy, business, automation, or event mapping.
The generic capability is discoverable but cannot send externally. Pausing or rolling
back creates another version; history is immutable.

The schema retains all four approval-policy values so draft and shadow definitions can
be forward-compatible. An automation may enter `active` only when the current executor
can honor its policy: `qiwe_text_template_v1` requires `space_admin_confirmation`, while
`agent_turn` permits only `none` or `space_admin_confirmation`. The same
runtime-contract check applies to direct active proposals, exact shadow activation, and
rollback.

An explicit `activate` operation applies only to the latest shadow automation in the
current Space and must be the proposal's sole change. It promotes an exact shadow
business or event mapping only when that dependency has no conflicting active version;
already-active exact dependencies are reused. Event activation requires the completed
authenticated real-event observation scoped to that exact shadow automation and Space.
That evidence must come from an immutable raw event created strictly after both the
exact mapping and automation shadow versions; replaying a callback persisted before
those versions cannot satisfy activation. The confirmation transaction locks and
rechecks every bound stream head and rejects dependency drift or a provider-mapping
transition that could replace another Space's live dependency. Schedule activation
copies the stored cron, timezone, and misfire policy and lets the generic dispatcher
calculate the first run cursor. This Release rejects conversational activation of
`agent_turn`: no supervised broker/runner or dedicated production identity is installed,
and the runtime rejects readiness `1` even with the reserved owner phrase. A later
owner-reviewed provisioning Release must add observable broker availability, runner
liveness, OS-identity and credential isolation before activation can accept it.

## Scheduling Contract

`automation_definition_versions` owns schedule state consumed by the generic dispatcher:

- `trigger_kind`: `event` or `schedule`.
- `trigger_config`: bounded declarative selector or cron configuration.
- `timezone`: defaults to `Asia/Shanghai`.
- `misfire_policy`: only `run_once` is accepted in v1; `skip` and `catch_up` remain
  reserved for a future reviewed version.
- `next_run_at` and `last_dispatched_at`: dispatcher cursors.

The dispatcher creates a work item with idempotency key
`automation:<automation-id>:<scheduled-for-utc>`. No per-business timer or `jobs.json`
entry is added. Invalid active schedule configuration aborts apply without rewriting the
version; pause or replacement remains an explicit reviewed Space operation.

## Execution Contract

The generic dispatcher selects only schedules whose exact business and default Space
policy are still active and whose selected capability remains granted. One generic
worker claims only `space_automation_run`. It requires the exact active or shadow
automation, bound business version, active default Space policy, globally enabled
execution gate, and a selected capability present in both business and Space ceilings.
Every selected capability must also opt in through registry metadata with
`space_invocable=true` and `space_scope_binding=work_item_space_id`; configuration and
execution enforce this independently. Before queueing, both dispatcher paths also
require that selected row to remain enabled, owned by Erhua, callable by `system`,
registered for the mode-specific work-item type, and present in both the business and
Space ceilings. Shadow definitions never execute externally.

The deterministic registry initially contains the generic `qiwe_text_template_v1` recipe
on `erhua.qiwe_text_template`. Recipe selection comes from registered capability
metadata rather than a capability-key branch. Its text is definition data, not a
welcome-specific code path. Canonical event subjects are rechecked against the exact
current QiWe room and all remaining names render into one message. `agent_turn` creates
a constrained internal work item with a business-owned output contract; it does not run
an unrestricted model in the executor. In this Release, active schedule/event selection
skips `agent_turn` and the executor rechecks the same disabled readiness boundary before
handoff, so it cannot create a stranded queued child. Shadow observation and all
deterministic automations remain available. Every external attempt is durable before
network access, and uncertain send outcomes are terminal without automatic retry.

`space_agent_turn` deliberately does not impersonate a Hermes turn. A dedicated,
default-disabled Unix-socket broker exposes bounded claim, capability invoke, and finish
operations only to the fixed `erhua-space-agent-runner-v1` identity after exact owner,
database-hash, bearer-token-hash, and Unix peer gates. It revalidates the parent, Space
policy, business, automation, optional event mapping, capability intersection, and
contracts before every operation. Lease deadlines come from PostgreSQL; a supervised
broker loop terminalizes expired claims without waiting for another request.

The repository does not invent a model provider. A second default-disabled local socket
inside the QiWe plugin reuses Hermes-owned `ctx.llm` and produces only a final object or
one catalog capability request; it does not execute tools. The isolated standard-library
runner connects the two sockets for one claim with a bounded 16-round loop. Every
capability request returns to the broker for live reauthorization, contract validation,
and a durable idempotent receipt. Finish derives actual usage from those receipts and
rejects altered runner telemetry.

The first catalog primitive is a read-only current-trigger-subject identity lookup. It
accepts no Space, target, arbitrary user ID, URL, or destination; the broker derives the
exact conversation and subject IDs from trusted work-item state and requires a recent
current-member roster sync. External-send flows still require the deterministic
executor's live room-detail verification. The manual `--once` path is now end-to-end
executable, while every production gate remains disabled until the owner provisions the
dedicated OS identity, private socket group, bearer, model bridge environment, and
credential isolation. The manual rehearsal leaves production readiness at `0`; setting
it to `1` is rejected even with the reserved approval phrase. No runner service or timer
is installed by this change.

## Production Boundary

This plan touches Postgres schema, the local operations-intake socket, Erhua tool
registration, QiWe adapter authentication and asynchronous event capture, a
default-disabled nginx ingress slot, disabled systemd units, deployment bundles,
rollback checks, and Release policy. The route can be rendered only by fixed signed
owner-gated apply/rollback requests from `release/current`; callback paths and tokens
remain server-local and never enter request evidence. No live callback URL, secret,
capability, timer, automation, or external send is enabled by this change.

The first deployment must keep event mappings in shadow and all automations inactive.
The new execution gate and both registered sub-capabilities are seeded disabled. Real
execution additionally requires the production-only QiWe adapter artifact, an explicit
owner phrase, runtime enable flag, database hash, and official-host allowlist. Default,
staging-only, or mixed QiWe builds cannot enter Space automation apply.

## Programming Extension Handoff

The programming Agent does not connect to Postgres. For programming extensions, the
existing operations-intake process remains the sole database-owning boundary and exposes
two bounded operations on its mode-`0600` Unix socket: claim one exact
`space_programming_extension_request`, then finish that same unexpired claim. Both
operations are disabled unless
`QINTOPIA_SPACE_PROGRAMMING_EXTENSION_DISPATCH_ENABLED=1`; any other non-empty value
fails intake startup.

The claim response contains only the work-item id, an opaque one-use lease, the
sanitized intent, provider, research query, and bounded official-document evidence. Each
evidence item carries its normalized registered QiWe URL and sanitized untrusted
excerpt; the broker recomputes the cross-runtime digest before handoff. It never
includes a Space, room, actor, person, source-message identifier, credential, routing
field, or caller-supplied URL. A claimed attempt is never automatically retried. An
expired claim becomes a terminal sanitized failure because a lost runner may already
have pushed a branch or opened a PR.

`tools/agents/run-space-programming-extension.mjs --once` performs the external half in
an isolated temporary git worktree based on the locally cached exact `origin/master`.
Codex receives a minimal child environment and may add only the append-only mapping,
synthetic fixture, canonical expectation, optionally one fixed-format mapping summary,
and, when necessary, one fixed-kernel restricted-parser recipe accepted by the low-risk
classifier, for at most five files. The runner uses fixed validation commands, creates a
`qintopia-programming-agent/` branch with a `feat(qiwe):` commit and PR title, records
an `awaiting_publish` PR handoff through the lease, and only then applies the fixed
`qintopia-low-risk-auto` label. It cannot merge, publish, deploy, send, run a migration
or alter runtime authentication.

The runner rejects GitHub tokens in its startup environment. Codex execution, allowed
path review, complete committed-diff review, fixed validation, low-risk classification,
and the final clean-worktree check all finish before the parent invokes a fixed
short-lived token helper. The first authenticated fetch then proves fetched
`origin/master` still equals the audited local base; drift fails closed before any push
or PR creation. The token helper receives only the fixed repository identity and must
return a token with a bounded expiration, never a long-lived token inherited from the
runner environment.

The handoff binds the original Space/request digest to the generated mapping key, exact
mapping source digest, candidate commit, and PR number. Same-Space status exposes only
the PR number and short fingerprints. It advances from `pr_created` to
`released/ready_to_replan` only when the active sidecar embeds the exact mapping digest
and carries a valid deploy-injected commit SHA. The trusted status wrapper then reloads
the original intent only through a same-Space internal operation, invokes the bounded
planner, and creates the exact mapping proposal in `shadow`. Continuation is idempotent
on the original programming-extension request id, the original intent is never returned
to the model-facing status response, and administrator confirmation remains mandatory.

Environment and HOME filtering do not prevent same-UID filesystem or process inspection.
Dispatch must therefore remain disabled until Codex executes under a dedicated OS
identity or equivalent container that cannot read production env, Hermes, COS, database,
server, or GitHub credentials. The separate PR orchestration boundary retains the GitHub
token outside that sandbox.

A separate `Low-Risk Auto Release` workflow is the sole pre-authorized consumer of that
label. It is default-disabled, authenticates a fixed dedicated actor/token, and may
advance only the append-only mapping bundle, its optional restricted recipe, and its
optional fixed-format summary. It revalidates exact required checks and the complete
unpublished range before candidate merge, exact Release Please merge, and draft
publication. Any additional code, dependency, schema, authentication, permission,
deployment, or production configuration change stops for owner review. The workflow
cannot activate the published runtime or issue a production deploy request itself.

The first rollout is a manual, default-disabled `--once` command. Scheduling the runner
or installing a service is a later owner-reviewed operation after the broker and
credential isolation have been observed locally.

## Implementation Status

The deterministic execution path and reusable control plane are implemented and remain
default-disabled. Local fixtures cover the declarative control plane, Space
authorization and isolation, Qiwe v1/v2 member events, restricted event research,
generic schedule dispatch, deterministic execution, the constrained `agent_turn`
claim/result broker, programming-extension handoff, release classification,
authenticated ingress controls, activation, observation, and rollback.

The `agent_turn` model completion bridge, bounded capability invoke path, durable usage
receipts, first current-Space read-only primitive, and manual isolated runner are
implemented and default-disabled. Agent output remains an inert artifact and cannot
route or execute follow-up work. Production activation, schedule/event dispatch, and
execution handoff all fail closed for `agent_turn`; deterministic automation remains
available. Production scheduling of the runner remains deferred to the owner-reviewed
OS-identity, liveness, and credential-isolation rollout; the repository does not
silently enable it during Release installation.

Production acceptance is intentionally still open. It requires the initial
owner-reviewed schema/ingress/runtime Release, a deploy-runner hardening Release adding
and observing `ProtectSystem=strict` or an equivalent fixed-path boundary before the
low-risk lane is enabled, an isolated agent-runner OS identity, one sanitized real
authenticated member-add callback, exactly-once welcome delivery evidence, a second
group configured only through conversation with cross-Space isolation evidence, and a
recorded smoke-failure rollback. None of those live actions is performed by this
repository change.

## Validation

- `pnpm test:qiwe`
- `pnpm fmt:sidecar`
- `pnpm check:sidecar`
- `pnpm test:sidecar`
- `pnpm check:pr:auto`
- `pnpm check`
- warning-denied Clippy with no default features and all features

The disposable PostgreSQL tier must cover Space isolation, actor authorization,
idempotent prepare, code expiry and attempt limits, one-active-version constraints,
pause/rollback, and status lookup from a different Space.

After the initial manual infrastructure Release: enable only shadow capture in the
current group, retain one sanitized real member-add callback, activate one test welcome
automation, prove exactly one send for duplicate callbacks, then configure a second
group entirely through conversation and prove no definitions or effects leak into the
first group. Only after that evidence may the owner enable the low-risk mapping release
lane.

## Rollback

Rollback is additive: stop exposing the three tools and pause the active definition
versions. The new nullable `space_id` columns and version history remain readable. Do
not drop definition or audit data during an application rollback.
