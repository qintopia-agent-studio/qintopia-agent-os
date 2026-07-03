# Runtime: Sidecar

`runtime/sidecar` is the Agent OS data and worker service contract for the existing
`qintopia-message-sidecar` Rust service.

## Current Source

- Local source: `../qintopia-message-sidecar`
- Adopted reference: `eda2652f21999e4f32699463413372accbd3b76e`
- Server deployment source observed on 2026-07-03: `/home/ubuntu/qintopia-msg-sidecar`
- Server branch observed on 2026-07-03:
  `codex/huabaosi-localization-shadow@b16c247a19ec751c08de75ae2d312f35b765f317`

The local `main` branch is the source for this package contract. The server Huabaosi
shadow branch is a review-pool input until the owner explicitly approves those files as
roadmap.

## Responsibility

The sidecar receives QiWe/Hermes message events from NATS JetStream, persists raw and
normalized records into Postgres, and runs Agent OS background workers. It must stay
independent from the Hermes reply path: sidecar, NATS, Postgres, or embedding failures
must not block webhook ACKs or group replies.

## Package Split

This package owns the service runtime and workers. Related contracts are split out so
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

## Validation Before Source Import

Run in `../qintopia-message-sidecar`:

```bash
cargo fmt --check
cargo test
cargo check
scripts/operations-control-plane-smoke.sh
```

Use guarded apply smokes only with explicit owner approval and configured local or
server database credentials.

## Next Migration Step

Import the sidecar source in a narrow follow-up commit after the package split is
reviewed. Preserve the Rust toolchain boundary, tests, fixtures, docs, and deployment
smoke scripts.
