# Workflow: Activity Promotion

`workflows/activity-promotion` is the governed multi-Agent workflow for turning activity
signals into reviewed operating assets and controlled group-send readiness.

## Current Source

- Current implementation: `../../runtime/sidecar/src/operations.rs`
- Supporting modules: `../../runtime/sidecar/src/collaboration.rs`,
  `../../runtime/sidecar/src/evidence.rs`,
  `../../runtime/sidecar/src/group_message_send.rs`,
  `../../runtime/sidecar/src/workbench.rs`, and
  `../../runtime/sidecar/src/xiaoman_activity.rs`
- Current production observation:
  `../../deploy/smoke/docs/xiaoman-production-preflight-record.md`
- Historical 2026-06-30 control-plane baseline:
  `docs/agentos-operations-control-plane.md`

## Responsibility

The workflow coordinates Xiaoman activity requests, Wenyuange evidence retrieval,
Huabaosi visual asset work, human review, optional image-generation request intake, and
Erhua group-message readiness. It is a control-plane workflow, not an autonomous publish
path.

## Required Human Gates

- Visual artifacts need review before use.
- An approved `poster_brief` may create an `image_generation_request`. The guarded
  adapter accepts only OpenAI-compatible `gpt-image-2` `b64_json` PNG output, uploads it
  to an isolated allowlisted media boundary, and validates a same-byte readback before
  recording a pending `generated_image`.
- The adapter remains disabled until separately owner-reviewed provider, storage,
  staged-smoke, budget, and rollback decisions exist.
- `huabaosi-image-generation-preflight` can validate the local adapter configuration
  without opening network or database connections. Its sanitized `adapter_config_ready`
  result is only a staging prerequisite, not approval to generate or publish.
- A `generated_image` must remain pending human review before any downstream use.
- The send-request starter creates `group_message_request` only from an approved
  `generated_image` whose image-generation request is completed. An approved
  `poster_brief` alone is insufficient.
- Group message requests need final human confirmation before send readiness.
- Allowlists control group targets, reviewers, confirmers, owners, and attachment hosts
  when configured.

## Boundaries

- Writes Postgres work items, events, artifacts, and workflow summaries.
- Does not directly call Feishu, QiWe, Huabaosi image providers, Wenyuange, or external
  send adapters in dry-run and current apply smoke paths.
- Must not use Hermes Kanban as the future orchestration backbone.

## Production Boundary

This workflow is already active as a control-plane package, but group-send execution and
external publication remain outside the current approved boundary. Any change that
enables real external sends needs owner review, allowlist evidence, smoke output, and
rollback notes.

The image request starter and preview worker are merged on `master`, but they are not in
the observed production `v0.2.6` release. The reviewed deployment path installs an
internal timer that only creates `image_generation_request` work items from approved
briefs. The separate provider worker remains disabled and has no systemd timer.

Before any staging adapter smoke, run:

```bash
huabaosi-image-generation-preflight
```

The command neither contacts the provider/media service nor mutates AgentOS. A missing
or invalid configuration is reported as `success=false` / `adapter_not_configured`
without exposing the failed value, then exits non-zero. Staging automation must treat
that exit status as a hard stop.

After the Required Human Gates have owner approval, one controlled staging generation
may run through `deploy/sidecar/scripts/huabaosi-image-generation-staging-smoke.sh`. It
requires an explicit flag, approval phrase, isolated staging env file, matching staging
database URL hash, and one existing image request UUID. It accepts only a newly created
pending `generated_image`; it never sends, writes Feishu, publishes, or installs a
timer.

## Acceptance Scenarios

- Activity signal creates a governed work item without sending an external message.
- Evidence lookup records source basis and risk notes.
- Visual asset work records artifact evidence and review state.
- An approved `poster_brief` can create one idempotent image-generation request on
  `master`, including through the internal starter timer; that request does not call a
  provider or create an image.
- Group-send readiness requires final human confirmation before any external send path
  is considered.

## Validation

Use the local no-credential smoke first:

```bash
deploy/sidecar/scripts/operations-control-plane-smoke.sh
pnpm workflows:check
```

The Postgres apply smoke is guarded and must only run with explicit owner approval and
configured database credentials.
