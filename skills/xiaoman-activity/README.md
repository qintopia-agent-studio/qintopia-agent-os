# Xiaoman Activity Skill

This package owns the Agent-facing Xiaoman activity tools that were previously buried
inside `skills/qintopia-tools`.

Hermes still loads stable tool names through the Xiaoman `qintopia-tools` profile
variant until profile bundle repoint is reviewed. This package provides the dedicated
capability boundary first and uses a legacy bridge to preserve byte-for-byte behavior
while the implementation is moved out of the broad registration shell.

## Capability

- read sanitized Xiaoman activity records by date or record reference;
- prepare text announcements, weekly preview drafts, and weekly poster workflow intake;
- prepare reviewed text group-message requests without sending;
- draft promotion review material and material summaries;
- prepare bounded activity field-update, status, gap, phase, and handoff commands.

## Tools

- `qintopia_xiaoman_activity_record_get`
- `qintopia_xiaoman_activity_list_by_date`
- `qintopia_xiaoman_activity_plan_table_probe`
- `qintopia_xiaoman_activity_announcement_prepare`
- `qintopia_xiaoman_activity_text_group_message_request_prepare`
- `qintopia_xiaoman_weekly_poster_workflow_prepare`
- `qintopia_xiaoman_public_reply_rewrite`
- `qintopia_xiaoman_activity_status_update`
- `qintopia_xiaoman_activity_gap_update`
- `qintopia_xiaoman_activity_phase_update`
- `qintopia_xiaoman_activity_feishu_field_update`
- `qintopia_xiaoman_activity_handoff_create`
- `qintopia_xiaoman_activity_promotion_review_draft`
- `qintopia_xiaoman_activity_material_summary`

## Runtime Boundary

- The skill may call release-managed read-through and worker command boundaries.
- Feishu writes are limited to the reviewed field-update worker command and never expose
  a generic write primitive.
- Text/image sending is not performed here. The skill may prepare approved work-item
  payloads; QiWe delivery remains behind the separate send-ready and adapter chains.
- No `.env`, profile sessions, raw Feishu/QiWe payloads, raw private chats, or live
  Hermes runtime state belong in this package.

## Legacy Bridge

The current implementation is loaded from
`skills/qintopia-tools/variants/xiaoman/__init__.py` through a narrow bridge. This
avoids two active implementations while keeping a dedicated package target for future
edits.

Do not add new Xiaoman activity behavior to `qintopia-tools`; route it here, then move
the legacy implementation behind this package in small, tested slices.

## Validation

```bash
pnpm skills:xiaoman-activity:check
pnpm skills:qintopia-tools:check
pnpm registry:check
```

## Operating rules

These constraints supplement the scoped AGENTS.md summaries. Conditions and historical
exceptions remain binding. Backtick paths from root rules are repository-relative;
Sidecar rules retain their original `runtime/sidecar/` path base.

### Commands

- Xiaoman activity signal timer observation smoke:
  `QINTOPIA_XIAOMAN_ACTIVITY_SIGNAL_TIMER_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/xiaoman-activity-signal-timer-observation-smoke.sh`

- Xiaoman activity promotion starter timer observation smoke:
  `QINTOPIA_XIAOMAN_ACTIVITY_PROMOTION_STARTER_TIMER_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/xiaoman-activity-promotion-starter-timer-observation-smoke.sh`

- Xiaoman activity downstream observation smoke:
  `QINTOPIA_XIAOMAN_ACTIVITY_DOWNSTREAM_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/xiaoman-activity-downstream-observation-smoke.sh`

- Xiaoman activity send request starter observation smoke:
  `QINTOPIA_XIAOMAN_ACTIVITY_SEND_REQUEST_STARTER_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/xiaoman-activity-send-request-starter-observation-smoke.sh`

- Xiaoman activity image generation starter observation smoke:
  `QINTOPIA_XIAOMAN_ACTIVITY_IMAGE_GENERATION_STARTER_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/xiaoman-activity-image-generation-starter-observation-smoke.sh`

- Xiaoman activity Feishu writes exist only as the bounded `feishu-field-update` worker
  operation behind `qintopia_xiaoman_activity_feishu_field_update`. It writes only the
  four Xiaoman-owned plan-table columns (`status`→小满运营状态,
  `promotion_status`→宣发判断, `notes`→小满备注, `reminder_status`→活动前提醒状态) on
  the single record resolved server-side by exact 活动主题+date; SingleSelect values
  must exactly match a live option fetched at write time (production options verified
  2026-08-10). Every attempt writes a `qintopia_agent_os.tool_invocation_audit` row and
  the report carries only the hashed `record_ref`. Widening the writable column set
  means an owner-reviewed PR updating `FEISHU_WRITE_FIELD_COLUMNS` (Rust) and
  `XIAOMAN_ACTIVITY_FEISHU_WRITE_FIELDS` (Python) together; never expose a generic
  Feishu write primitive to chat tools.

- The official Feishu OpenAPI MCP (`@larksuiteoapi/lark-mcp`) exists but is not mounted
  on group-facing profiles: it is beta, unmaintained since 2025-08, takes app secrets as
  process arguments, and exposes generic Base access without the read-through output
  sanitization. There is no official Feishu CLI; the audited read-through worker remains
  the Xiaoman read channel.

- The production activity plan table column `下周排期确认` is a single-select with
  options `待确认（居民提交表单后默认）`, `已确认-排入下周`, `暂缓` (verified against
  the Feishu fields API on 2026-08-10). Consumers must substring-match these values, not
  exact-match short forms like `已确认`.

- Xiaoman activity production preflight smoke:
  `QINTOPIA_XIAOMAN_ACTIVITY_PRODUCTION_PREFLIGHT_ENABLE=1 deploy/sidecar/scripts/xiaoman-activity-production-preflight-smoke.sh`

### Core Rules

- On macOS, run the complete sidecar unit suite with
  `RUST_MIN_STACK=33554432 cargo test --manifest-path runtime/sidecar/Cargo.toml`. The
  default test-thread stack can overflow in an existing Xiaoman async test; see
  `docs/reports/2026-07-13-rust-test-stack-limit.md`.

- Postgres/AgentOS is the system fact source. Feishu is a human workbench and mirror,
  not the source of truth.

- `agents/xiaoman/profile-bundle/migrate_values.py --apply` is a one-time manual
  observation prerequisite. It must require root and the exact owner approval before
  reading the fixed live files, lock both reviewed source hashes, validate exactly four
  values, prove complete rendered parity, and no-clobber create only the root-owned mode
  `0600` `/etc/qintopia/xiaoman-profile-bundle-values.json`. It must not accept path
  overrides, print values, edit the live profile, create symlinks, restart Hermes, use
  the network, write Postgres/Feishu, call external adapters, publish, or send. The
  deploy runner must never invoke it automatically.

- Xiaoman activity signal intake uses `xiaoman-activity signal-ingest` to create
  `xiaoman.create_activity_request` through the operations control plane with
  `requester_agent=default` and `target_agent=xiaoman`; do not bypass capability policy
  by making Xiaoman call its own provider capability directly.

- `qintopia_xiaoman_activity_list_by_date` may execute read-through only when
  `QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE=1` and Feishu Base mode is explicitly
  selected with `QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE=1`. In that mode it may run
  the configured sidecar for read-only, non-dry-run queries and return sanitized
  `record_count`, `records`, and `summaries`; write wrappers must continue to return
  bounded worker commands. The read-through worker path validator must follow the
  release integrity model: root-owned `0755` release roots and sidecar binaries are
  acceptable because the `ubuntu` runtime user cannot modify them; runtime-user-owned
  writable paths and group/world-writable paths must still be rejected.

- Xiaoman prompt rules for immediate Feishu/Base queries must require explicit requester
  authorization and current conversation visibility checks before inlining table names,
  counts, or record summaries. If either boundary is unclear, the reply must ask for
  authorization or move to a controlled private/review channel instead of exposing row
  summaries in the current conversation.

- Xiaoman signal apply smokes should use sanitized non-UUID event signal ids unless a
  matching `qintopia_agent_os.event_signals` row is created first; UUID
  `event_signal_id` values are stored as `source_event_signal_id` and must satisfy the
  Postgres foreign key.

- The `feishu_task_dry_run` workbench mirror must preserve immediate `child_status_refs`
  and expose nested work only through sanitized `descendant_status_refs` with direct
  parent and depth. It must not copy raw payloads, call Feishu, or make the workbench a
  fact source. Keep the description bounded and report truncation explicitly.

### Sidecar Rules

- New built-in capabilities must be registered in both `builtin_capability()` and
  `BUILTIN_CAPABILITY_KEYS`, with matching capability-list smoke expectations. If a new
  root capability should participate in Xiaoman downstream starters, update every
  candidate selector for child creation, image generation, and send-request staging.

- Xiaoman Feishu poster apply must fail before Postgres or external I/O unless the exact
  owner phrase, release SHA, database hash, official API host, app credentials, and
  direct-chat/user/media allowlists pass. Internal-group selection and callbacks
  additionally require authenticated ingress, the separate group switch, the persisted
  internal policy/thread target, matching ingress/delivery chat and user ceilings, and
  an operations reviewer ceiling covering every allowed user. Persist an attempt before
  upload, terminalize expired in-flight attempts as ambiguous, verify card callbacks
  inside the sidecar, never fall back from a group thread to a main timeline or direct
  chat, and never create group-send authorization. Keep direct and group production
  scheduling in separate scope-pinned services and timers even though they share the
  durable queue and worker binary; review callback dry-runs must enforce the same
  runtime delivery boundary as apply and may skip only persistence mutations; group
  activation and rollback must not mutate the direct timer.

- When `QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE=1` and
  `QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY` are configured, legacy V2 operations intake
  must fail closed before session trust, Postgres, or workflow mutation. Unset or `0`
  hook enablement must keep authenticated ingress disabled even when other ingress env
  exists; other enablement values must fail closed. Do not allow a caller to downgrade
  from authenticated Feishu ingress to the local V2 direct workflow path.

- A canary review apply must provide expected artifact type and review status
  preconditions. The sidecar must enforce them again under the artifact row lock before
  changing review state, and before authenticated Feishu revalidation, so a mistaken
  generated-image UUID cannot be approved through a poster-brief workflow.

- `xiaoman-profile-bundle-observation-smoke.sh` may only verify reviewed source hashes
  and byte parity after rendering into a temporary directory. It must not print
  server-local identity values, create symlinks, edit live profile files, restart
  Hermes, write Postgres/Feishu, use external adapters, or send.
