# Runtime: Sidecar

`runtime/sidecar` is the Agent OS data and worker service package adopted from the
existing `qintopia-message-sidecar` Rust service.

## Current Source

- Local source: `../qintopia-message-sidecar`
- Adopted reference: `eda2652f21999e4f32699463413372accbd3b76e`
- Server deployment source observed on 2026-07-03: `/home/ubuntu/qintopia-msg-sidecar`
- Server branch observed on 2026-07-03:
  `codex/huabaosi-localization-shadow@b16c247a19ec751c08de75ae2d312f35b765f317`

The local `main` branch is the source for this package contract. The server Huabaosi
shadow branch is a review-pool input until the owner explicitly approves those files as
roadmap.

## Space Automation Execution

`run-space-automation-execution-worker` consumes only `space_automation_run` work items.
It locks the exact Space policy, automation version, business version, and enabled
registered capability before committing a non-retryable attempt boundary. The target is
always loaded from `work_items.space_id -> conversations.chat_id`; no definition,
work-item metadata, or command argument can supply a destination.

The first registered deterministic recipe is `qiwe_text_template_v1`, selected through
`metadata.space_execution_recipe` on `erhua.qiwe_text_template`. For event subjects it
loads `/room/batchGetRoomDetail`, requires the exact current room, intersects canonical
string user ids with its current roster, renders all current names into one message,
updates the validated `roomName` (or `name`) in `conversations.display_name`, and sends
once. The display name never authorizes routing. Any uncertain send result is terminal
`ambiguous`. `agent_turn` does not invoke a model in this worker; it creates one
`space_agent_turn` child with exact definition digests, the enabled Space capability
intersection, and the business-owned output contract only after the separate production
runtime is verifiably provisioned. This Release has no supervised broker or runner, so
the binary rejects readiness `1` even with the reserved owner approval phrase; an
environment declaration alone is not readiness evidence. Schedule and event dispatch
therefore skip `agent_turn`, Space activation rejects it, and the execution worker fails
before creating a child. Deterministic execution is unaffected. Event-triggered children
also snapshot the exact registered event-mapping ID and digest that produced the parent
run.

`run-space-agent-turn-broker` exposes bounded claim, capability invoke, and finish
operations on a separate mode-`0660` Unix socket for the fixed
`erhua-space-agent-runner-v1` identity. It defaults off and fails before socket bind or
Postgres access unless the exact owner phrase, database URL hash, runner bearer-token
hash, dedicated runner uid, and private shared gid are present. The parent directory
must be owner-provisioned as exact mode `0750`, owned by a user other than the runner,
and assigned to that private gid. After bind, the broker verifies the socket owner, gid,
and mode; every connection must also carry the configured OS peer uid/gid before its
bearer token is read. The shared operations-intake socket does not expose these
operations.

Every claim, invoke, and finish revalidates the exact active automation, business,
default Space policy, optional event mapping, parent work item, conversation, capability
grants, and definition digests. The claim contains no Space UUID, room id, destination,
actor id, credential, or endpoint. It returns only the bounded goal and trigger, the
exact business output contract, and capabilities currently present in all business,
Space, and global registry ceilings. Each invoke accepts one catalog key and
contract-bound input, derives Space and subjects from the locked work item, and writes a
durable idempotent child receipt. Finish derives the true capability usage from those
receipts and rejects a runner report that omits or alters it.

The first broker recipe, `trigger_subject_identity_lookup_v1`, is read-only. It can
resolve only IDs already present in the current event trigger, only in the exact Space
chat, and only from a current-member roster sync no older than 24 hours. It cannot
accept arbitrary user IDs or a target. Any later external-send consumer must still run
the deterministic executor's live `/room/batchGetRoomDetail` verification.

Claim leases are issued from the PostgreSQL clock. The broker reconciles expired claims
at startup and every 30 seconds even when no runner requests another claim. A finish
that arrives after expiry must first prove the exact stored claim token, then records
the same terminal `runner_claim_expired_unknown` outcome; its late output is ignored and
no result artifact is created.

Completed agent output is stored only as inert, untrusted artifact data. It is marked
`execution_eligible=false` and `routing_authority=none`; no downstream action may use a
field name or string value from that output as a room, recipient, endpoint, credential,
or authorization source. Those values must always be rebuilt from trusted Space context.
The v1 contract subset supports precise JSON integers but intentionally excludes
floating-point `number` values.

The repository does not create a second model provider. The default-disabled QiWe
completion socket reuses Hermes-owned `ctx.llm`, returns only a final object or one
catalog capability request, and never executes the capability itself. The
standard-library `tools/agents/run-space-agent-turn.py --once` process connects that
completion boundary to the broker and exposes no other tool path. Production rollout
must still provision its dedicated OS identity, private socket group, bearer, exact
owner gates, credential isolation, and supervised liveness evidence. No service or timer
for that process is installed or enabled by default. A separately reviewed manual
rehearsal uses the broker and runner's own gates while production runtime readiness
stays `0`.

Capability grants are not enough on their own. A capability must opt in with
`metadata.space_invocable=true` and `metadata.space_scope_binding=work_item_space_id`;
proposal confirmation and runtime claiming both reject capabilities without that
contract.

QiWe raw events can drive mappings only when they arrive on the distinct
`qintopia.qiwe.raw.authenticated` NATS subject while the consumer's trusted-subject gate
and file-based consumer authentication are enabled. Publisher JSON cannot assert this
fact: `RawQiweEvent` ignores `ingress_auth_verified`, and the consumer overwrites it
from the actual subject. The NATS URL cannot contain credentials. Production must deny
anonymous access and use separate publish-only adapter and consume/API sidecar
principals before enabling trusted capture. Unauthenticated events remain
archive-compatible but cannot create shadow observations or automation runs.

Ordinary messages remain asynchronous best-effort capture. When the separate,
default-off durable-system-event gate is enabled, the QiWe adapter requires an
authenticated-raw-subject JetStream PubAck for every authenticated system event in the
complete `data[]` envelope under one 1.5-second budget; a partial or uncertain capture
returns HTTP 503 and relies on the existing event identities for retry deduplication.

Trusted event replay is data-driven. `build.rs` embeds complete append-only bundles from
`fixtures/qiwe/event-mappings/**/*.mapping.json`,
`fixtures/qiwe/system/**/*.fixture.json`, and matching `*.expected.json` files. Runtime
activation requires an exact registered selector/extractor and exact canonical replay
output. A new bounded mapping bundle therefore becomes available after an ordinary
sidecar build without adding a Rust event-type branch; incomplete, ambiguous, or
drifting bundles fail closed.

When the direct mapping transforms are insufficient, a mapping may reference one
release-embedded `*.primitive.json` recipe per extractor field. Recipes can only compose
the fixed parser kernel and cannot invoke another recipe. Adding a kernel operation or
any executable parser source remains an owner-reviewed change. The complete registry is
replayed in tests so a schema-valid but behaviorally invalid recipe cannot ship through
the low-risk lane.

All three execution capabilities are disabled in Postgres by default. Apply also
requires the runtime enable flag, exact owner phrase, database URL hash, and, for QiWe,
the standalone production artifact compiled with `qiwe-production-adapter` and without
`qiwe-staging-adapter`, plus the exact HTTPS host allowlist. Default, staging-only, and
mixed adapter builds fail before Postgres access. A dry run performs no external access:

```bash
qintopia-message-sidecar run-space-automation-execution-worker --once --dry-run
```

## Space Schedule Dispatcher

`run-automation-dispatcher` is the single database-backed scheduler for every Space
business. It selects due active schedule definitions, creates `space_automation_run`
work items with idempotency key `automation:<automation-id>:<scheduled-for-utc>`, and
advances only the selected definition's cursor. It adds no per-business timer, script,
or Hermes `jobs.json` entry.

Version 1 accepts only `misfire_policy=run_once`; the database constraint, proposal
validator, activation path, and dispatcher all enforce the same rule. `skip` and
`catch_up` are reserved for a future reviewed version. If an active definition has an
invalid cron, timezone, or policy, apply aborts without rewriting that immutable version
or silently pausing it. An administrator must pause or replace it through a reviewed
Space definition operation.

## Ordinary QiWe Group-Turn Policy

The operations-intake socket exposes two internal, schema-v1 policy operations for the
trusted QiWe adapter:

- `space_turn_policy_context` resolves the exact current active group and returns only
  `policy_found`, `identity`, `knowledge_scope`, and `effective_capabilities`. It is
  read-only but still requires the authenticated persisted-message receipt.
- `space_turn_capability_authorize` checks one fixed-catalog capability immediately
  before invocation. It uses the same receipt resolver and returns only `authorized`
  plus `capability_key`.

Both responses also include `success` and `external_send_executed=false`; neither
returns a Space UUID, provider group id, or user id. With no active `default` Space
policy, context is explicitly empty and every ordinary capability is denied. Query or
policy-shape failures reject the request instead of disabling isolation.

The ordinary-turn catalog is globally disabled by default. A capability becomes
effective only when the current Space policy grants it and its global registry row is
enabled for provider/caller `erhua`, work-item type `qiwe_group_turn`,
`space_turn_invocable=true`, scope binding `trusted_session_space_id`, and invocation
boundary `erhua.space_turn`. Space configuration prepare/confirm/status tools retain
their separate administrator-confirmation boundary and do not pass through this ordinary
capability gate.

An active policy change may express `capability_revocations` as a delta against the
current active grant set. Confirmation rejects a missing grant or a simultaneous grant
and revocation, then stores the complete resulting `capability_grants` and removes the
delta field. `quota_declaration` is accepted only with
`enforcement=reserved_non_enforced`; version 1 records bounded limits for future use but
does not enforce them.

`erhua.knowledge.community` has one additional backend gate: its global registry row
must set `metadata.knowledge_scope_enforced=true`. The seeded row deliberately sets it
to false, so a Space grant cannot expose community knowledge until the backend proves it
enforces the current policy's knowledge scope. Public knowledge is always projected as
`Public` regardless of a model-supplied scope.

The trusted QiWe adapter enables this ordinary-turn boundary with
`QIWE_SPACE_TURN_POLICY_ENFORCEMENT_ENABLED=1`; it defaults to `0` for reviewed rollout.
Each context lookup and just-in-time authorization is bounded by
`QIWE_SPACE_TURN_POLICY_TIMEOUT_SECONDS`, which defaults to `0.4` seconds and fails the
governed operation closed on timeout.

## Xiaoman Feishu Poster Return

The poster path is split into durable commands:

```text
run-operations-intake
run-xiaoman-poster-notification-starter --once --apply
xiaoman-feishu-poster-preflight --conversation-scope direct
xiaoman-feishu-poster-preflight --conversation-scope group
run-xiaoman-feishu-poster-delivery --once --apply --conversation-scope direct
run-xiaoman-feishu-poster-delivery --once --apply --conversation-scope group
run-xiaoman-poster-review-callback-ingress
```

The intake and callback sockets default to
`/run/qintopia-agentos/operations-intake.sock` and
`/run/qintopia-agentos/poster-review-callback.sock`; both are mode `0600` under a `0700`
runtime directory. Delivery is compiled behind `xiaoman-feishu-poster-adapter`, disabled
by default, and requires the exact owner phrase, release/database bindings, official
Feishu API root, app credentials, and chat/user/media allowlists. Card callbacks use a
bounded signed envelope containing `timestamp`, `nonce`, `signature`, and `body_base64`;
the sidecar verifies the Feishu signature and five-minute clock window before any review
mutation. When authenticated Xiaoman Feishu ingress is explicitly enabled with
`QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE=1` and
`QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY`, legacy V2 operations intake is rejected
instead of downgrading around the signed ingress boundary. Unset or `0` hook enablement
keeps that ingress disabled even when non-secret config or keys are pre-provisioned;
other values fail closed.

Delivery attempts are persisted before upload. Expired `uploading` or `sending` attempts
become terminal `ambiguous` and are not automatically replayed. The path never creates
group-send authorization.

### Conversation Ingress V3

The existing operations-intake socket also accepts a signed `feishu_message_ingest` V3
envelope. Direct ingress requires the dedicated ingress HMAC key and exact chat/user
deployment allowlists. Internal-group ingress additionally requires the exact Bot
identity. The sidecar verifies the timestamp, nonce, HMAC, minimal message schema,
deployment ceilings, and active Postgres conversation policy before persisting the
message. The complete Feishu SDK payload is never stored by this path.

The explicit `QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE=1` flag plus a complete
authenticated-ingress configuration is also the protocol cutover boundary: the socket
then accepts only V3 poster and status requests. With the flag absent or set to `0`,
preprovisioned keys and allowlists remain inactive and the socket accepts only the
one-release V2 direct compatibility request. Invalid flags, partial enabled
configuration, and mismatched plugin/sidecar cutovers fail closed instead of downgrading
around the signed receipt.

Apply versioned policies only through bounded stdin:

```bash
qintopia-message-sidecar conversation-policy-apply --stdin < policies.json
```

The command fails before reading stdin or connecting to Postgres unless the exact owner
approval and database URL hash are present. It emits only policy counts, versions, and
opaque hashes. Internal-group behavior remains disabled by default through
`QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED=0`. When separately enabled for
repository, staging, or an owner-approved production canary, AgentOS resolves group
intake from the persisted receipt, snapshots requester/reviewer authority, and persists
the conversation notification. The same delivery worker selects eligible direct and
group targets independently. Group delivery uses only the persisted thread root and is
also gated by authenticated ingress, the deployment chat/user ceilings, and the separate
production activation contract. A disabled group row remains pending and cannot block
direct work.

Production scheduling uses separate direct and internal-group systemd services and
timers. Both invoke this worker and queue, but each command pins one
`--conversation-scope`; their preflights and the SQL claim predicate enforce the same
split. Release installation does not enable either external-delivery timer, direct
activation keeps the group timer stopped, and the separate group activation never
mutates the direct timer. A broken group-only ceiling therefore fails the group service
closed without blocking direct delivery.

When group support is enabled, the ingress and delivery chat/user ceilings must match,
and every permitted requester/reviewer must also be present in the operations reviewer
ceiling. A mismatched deployment fails before Postgres or Feishu so an accepted group
request cannot become an undeliverable or unreviewable card.

Production observation, activation, and rollback are separate from the existing direct
poster activation. Their release-local entrypoints are documented in
`../../deploy/sidecar/README.md`. They do not create a second queue, publication
request, main-timeline fallback, or direct-message fallback.

## Responsibility

The sidecar receives QiWe/Hermes message events from NATS JetStream, persists raw and
normalized records into Postgres, and runs Agent OS background workers. It must stay
independent from the Hermes reply path: sidecar, NATS, Postgres, or embedding failures
must not block webhook ACKs or group replies.

## Package Split

This package owns the service runtime and workers. Related packages are split out so
reviewers can reason about risk:

- `runtime/postgres`: migrations, schema notes, and database runbooks.
- `mcp/context-server`: context and answer-basis MCP surface.
- `mcp/message-store`: message search and evidence lookup MCP surface.
- `workflows/activity-promotion`: Xiaoman, Wenyuange, Huabaosi, and Erhua operations
  control-plane workflow.
- `deploy/sidecar`: systemd, smoke, rollout, and rollback procedures.

## Boundaries

- External sends: only dedicated, default-disabled workers own sends; Space execution
  additionally requires a version-bound policy/capability grant and Space-derived
  target.
- Database writes: yes. Migrations and workers write Agent OS state.
- Runtime profile: no direct Hermes profile mutation.
- Secrets: uses runtime-only env vars and database URLs; never commit real env files.

## Huabaosi WeCom Migration Entrypoints

The 阿亮画报师 WeCom migration is layered and does not replace the production Hermes Bot
route from this package yet:

- `huabaosi-wecom-shadow-capture`: read one bounded stdin event and emit sanitized
  shadow metadata only.
- `huabaosi-wecom-policy-preview`: read one bounded stdin event and emit sanitized
  policy decisions only.
- `huabaosi-wecom-canary-preflight`: validate canary configuration without stdin,
  network, database, or sends.
- `huabaosi-wecom-canary-gateway`: dry-run one allowlisted payload by default; real
  apply requires the non-default `huabaosi-wecom-canary-gateway` Cargo feature plus
  owner-reviewed staging configuration and exact allowlists.

These commands must not change the production Bot route, install timers, write Feishu,
call image providers, upload media, or send outside an approved canary allowlist.

## Imported Contents

- Rust crate: `Cargo.toml`, `Cargo.lock`, and `src/`.
- Runtime config templates: `config/agentos/`.
- Replay fixtures: `fixtures/`.
- Safe env template: `.env.example`.
- Source-specific agent rules: `AGENTS.md`.

Migrations are intentionally owned by `runtime/postgres`. The sidecar loads
`../postgres/migrations` by default inside this monorepo. Set
`QINTOPIA_SIDECAR_MIGRATIONS_DIR` to override the path for legacy deployments or local
experiments.

## Validation

Run from the monorepo root:

```bash
pnpm test:sidecar
```

For source-level checks during M5:

```bash
pnpm fmt:sidecar
pnpm check:sidecar
```

Use smoke scripts under `deploy/sidecar/scripts/` only with the documented environment
and owner approval. Guarded apply smokes can write Postgres state when explicitly
enabled.
