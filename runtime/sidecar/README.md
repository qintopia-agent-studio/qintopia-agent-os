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

## Xiaoman Feishu Poster Return

The poster path is split into durable commands:

```text
run-operations-intake
run-xiaoman-poster-notification-starter --once --apply
xiaoman-feishu-poster-preflight
run-xiaoman-feishu-poster-delivery --once --apply
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
mutation.

Delivery attempts are persisted before upload. Expired `uploading` or `sending` attempts
become terminal `ambiguous` and are not automatically replayed. The path never creates
group-send authorization.

### Conversation Ingress V3

The existing operations-intake socket also accepts a signed `feishu_message_ingest` V3
envelope. This operation is available only when the dedicated ingress HMAC key, Bot
identity, and exact chat/user deployment allowlists are configured. The sidecar verifies
the timestamp, nonce, HMAC, minimal message schema, deployment ceilings, and active
Postgres conversation policy before persisting the message. The complete Feishu SDK
payload is never stored by this path.

A complete authenticated-ingress configuration is also the protocol cutover boundary:
the socket then accepts only V3 poster and status requests. Without that configuration,
it accepts only the one-release V2 direct compatibility request. A mismatched plugin and
sidecar cutover therefore fails closed instead of downgrading around the signed receipt.

Apply versioned policies only through bounded stdin:

```bash
qintopia-message-sidecar conversation-policy-apply --stdin < policies.json
```

The command fails before reading stdin or connecting to Postgres unless the exact owner
approval and database URL hash are present. It emits only policy counts, versions, and
opaque hashes. PR 1 keeps `QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED=0`, creates no
thread reply, and calls no Feishu endpoint.

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

- External sends: no direct group send ownership in this package.
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
