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

The workflow coordinates Xiaoman activity requests, source-grounded evidence retrieval,
Huabaosi visual asset work, human review, optional image-generation request intake, and
Erhua group-message readiness. It is a control-plane workflow, not an autonomous publish
path.

`operations-work-item-status` resolves any nested work item back to the top-level
activity request and returns every descendant with its direct `parent_work_item_id`.
This keeps the image-generation request, which is nested under the visual request,
visible in the same status report and workflow summary.

The `feishu_task_dry_run` workbench mirror keeps its immediate `child_status_refs` and
adds a complete `descendant_status_refs` summary with direct parent and depth. This
makes the same nested image-generation stage visible to human operators without calling
a Feishu API or copying raw payloads into the mirror. The dry-run description is bounded
to depth 8 and 32 refs and reports explicitly if that bound truncates an abnormal tree.

## Required Human Gates

- Visual artifacts need review before use.
- An approved `poster_brief` may create an `image_generation_request`. The guarded
  adapter accepts only OpenAI-compatible `gpt-image-2` `b64_json` PNG output, fully
  decodes it, composites alpha over white, and encodes a quality-92 JPEG. Only that
  final JPEG is uploaded to the isolated allowlisted media boundary, read back
  byte-for-byte, and recorded as the pending `generated_image`.
- The adapter remains disabled until separately owner-reviewed provider, storage,
  staged-smoke, budget, and rollback decisions exist.
- `huabaosi-image-generation-preflight` can validate the local adapter configuration
  without opening network or database connections. Its sanitized `adapter_config_ready`
  result is only a staging prerequisite, not approval to generate or publish.
- A `generated_image` must remain pending human review before any downstream use.
- Approving a `generated_image` also requires its recorded worker provenance, HTTPS URI,
  final JPEG sha256, source PNG hash, fixed transform metadata, source brief/prompt
  refs, and creation audit to match the image request. Human review cannot approve a
  manually inserted, stale, or incomplete image record.
- The send-request starter creates `group_message_request` only from an approved
  `generated_image` whose image-generation request is completed. An approved
  `poster_brief` alone is insufficient.
- Group message requests need final human confirmation before send readiness.
- Allowlists control group targets, reviewers, confirmers, owners, and attachment hosts
  when configured.

## Boundaries

- Writes Postgres work items, events, artifacts, and workflow summaries.
- Event-signal evidence may read explicitly linked Postgres messages, with a same-chat
  bounded local keyword fallback. It does not call external Wenyuange, embeddings,
  Feishu, QiWe, Huabaosi image providers, or send adapters.
- Must not use Hermes Kanban as the future orchestration backbone.
- Recursive status reporting is read-only. Workflow sync may persist the derived AgentOS
  summary, but neither command executes a worker or acts as a general DAG scheduler.

## Production Boundary

This workflow is already active as a control-plane package, but group-send execution and
external publication remain outside the current approved boundary. Any change that
enables real external sends needs owner review, allowlist evidence, smoke output, and
rollback notes.

The image request starter and preview worker are merged on `master`, but they are not in
the observed production `v0.2.6` release. Source-grounded Postgres evidence retrieval is
also newer than that observed release. The reviewed deployment path installs internal
timers for evidence artifacts and `image_generation_request` intake. The separate image
provider worker remains disabled and has no systemd timer.

The final QiWe image-send contract is documented in
`../../docs/plans/active/xiaoman-qiwe-image-send.md`. It uses the official async URL
upload, correlates the `cmd=20000` Webhook by `requestId`, and only then permits a
`/msg/sendImage` request with complete file credentials. The Rust contract and local
preflight are implemented. An additive Postgres state machine now records hashed upload
correlation, callback idempotency, claim tokens, and sanitized terminal audit without
persisting callback file credentials. No network worker, dedicated callback listener, or
timer exists. The current Huabaosi path converts provider PNG into the exact final JPEG
artifact reviewed by humans. Staging must still verify that JPEG through the isolated
media and QiWe callback boundaries before any external-send implementation can be
enabled.

The no-network configuration check is:

```bash
qiwe-image-send-preflight
```

It fails closed unless the HTTPS API host, generated-image media hosts, target groups,
credentials, and reviewed Webhook readiness flag are configured. A successful preflight
still does not authorize upload or sending. The command fails when the send-enable flag
is `1`; staging enablement needs a separate owner-reviewed gate.

Before any staging adapter smoke, run:

```bash
huabaosi-image-generation-preflight
```

The command neither contacts the provider/media service nor mutates AgentOS. A missing
or invalid configuration is reported as `success=false` / `adapter_not_configured`
without exposing the failed value, then exits non-zero. Staging automation must treat
that exit status as a hard stop.

After an owner-approved internal-runtime deploy, the production disabled-state
observation verifies that image generation remains off and that no provider worker
service or timer is installed. It runs configuration preflight and the image worker only
as `--once --dry-run` to inspect the AgentOS queue. This is not a staging adapter smoke
and cannot create an image.

After the Required Human Gates have owner approval, one controlled staging generation
may run through `deploy/sidecar/scripts/huabaosi-image-generation-staging-smoke.sh`. It
requires an explicit flag, approval phrase, isolated staging env file, matching staging
database URL hash, and one existing image request UUID. It accepts only a newly created
pending `generated_image`; it never sends, writes Feishu, publishes, or installs a
timer. The smoke must assert `image/jpeg`, 1024x1024, a positive bounded byte size, and
a canonical hash for the exact reviewed bytes.

## Acceptance Scenarios

- Activity signal creates a governed work item without sending an external message.
- Event-signal evidence lookup records sanitized Postgres message sources and fails
  closed when no authorized source exists.
- Visual asset work records artifact evidence and review state.
- An approved `poster_brief` can create one idempotent image-generation request on
  `master`, including through the internal starter timer; that request does not call a
  provider or create an image.
- Querying the status of a nested image-generation request resolves the activity root
  and reports immediate and nested descendants without losing the parent relation.
- A workbench mirror description reports both immediate children and all descendants,
  including the nested image-generation request, while keeping sensitive payload fields
  excluded.
- An incomplete or forged `generated_image` approval is denied and audited without
  completing its image request or unlocking the send-request starter.
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

The Rust send-ready PostgreSQL integration test is ignored by default and may run only
against the disposable `qintopia_test` database with
`QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE=1` and the explicit `postgres-integration-tests`
Cargo feature. It proves an approved generated image records exactly one internal
`group_message_send_ready_recorded` event, duplicate apply is a no-op, and a pending
artifact fails closed without any `send_executed` or `external_published` event. It does
not call QiWe.
