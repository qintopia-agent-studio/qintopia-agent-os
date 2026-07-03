# M5 Runtime Sidecar Adoption Closure

Date: 2026-07-03

## Scope

M5 adopted the reviewed local `qintopia-message-sidecar` source snapshot into the Agent
OS monorepo and split it by Agent OS domain:

- `runtime/sidecar`: Rust service, workers, config templates, fixtures, and runtime
  package rules.
- `runtime/postgres`: migrations and data-design notes for the Agent OS fact source.
- `mcp/context-server`: context, knowledge, evidence, and live-ops routing MCP surface.
- `mcp/message-store`: controlled message and discussion-evidence lookup.
- `workflows/activity-promotion`: operations control-plane workflow.
- `deploy/sidecar`: legacy deploy snapshot, no-credential smokes, rollout notes, and
  cutover planning.

## Source

- Adopted source: `../qintopia-message-sidecar`
- Adopted commit: `eda2652f21999e4f32699463413372accbd3b76e`
- Source snapshot note: `runtime/sidecar/docs/source-snapshot.md`

The server branch
`codex/huabaosi-localization-shadow@b16c247a19ec751c08de75ae2d312f35b765f317` remains
review-pool material. M5 did not adopt that branch as product direction.

## Completion Evidence

- M5 packages have registry entries in `registry/runtime.yaml`, `registry/mcp.yaml`,
  `registry/workflows.yaml`, and `registry/deploy.yaml`.
- The M5 package entries are active monorepo contracts. This does not mean production
  systemd has cut over to the monorepo; production cutover remains M9.
- M5 packages have README and manifest or workflow metadata.
- Source reference `eda2652f21999e4f32699463413372accbd3b76e` is recorded in the package
  manifests.
- Sidecar source tests are wired through `pnpm test:sidecar`.
- Sidecar formatting and compile checks are available through `pnpm fmt:sidecar` and
  `pnpm check:sidecar`.
- No-credential workflow smokes are wired through `pnpm smoke:sidecar`.
- Postgres migration and data-design consistency is enforced by `pnpm policy:check`.
- Secret and runtime-state exclusions are enforced by `pnpm secrets:check`.
- Deployment cutover remains non-mutating and gated by `pnpm deploy:preflight`.

## Out Of Scope

- M9 server cutover from `/home/ubuntu/qintopia-msg-sidecar` to this monorepo.
- Production systemd changes.
- Copying or adopting the server Huabaosi shadow branch.
- Running guarded Postgres apply smokes without owner approval.
- Converting the legacy `deploy/sidecar/scripts/server-deploy.sh` snapshot into the
  final monorepo-native deployment script.

## Follow-Up

- M7 should finish WorkTool/OpenClaw cleanup decisions.
- M9 should perform the reviewed server cutover using an approved commit SHA,
  `pnpm deploy:preflight`, server build checks, service health checks, smokes, and
  rollback notes.
