# Xiaoman activity contract

These are retained operating constraints, not a new permission grant. Read the sections
relevant to the change before editing. Original conditions and exceptions remain
binding; summaries in AGENTS.md do not replace them.

Backtick paths in root-source rules are repository-relative; paths in Sidecar-source
rules retain their original `runtime/sidecar/` base. New navigation links are relative
to this document.

[Migration inventory](../../../docs/plans/active/agents-guidance/README.md) records the
baseline and source locations.

<a id="root-071"></a>

## Commands — root-071

<!-- preserved-rule: root-071 -->

- Xiaoman activity signal timer observation smoke:
  `QINTOPIA_XIAOMAN_ACTIVITY_SIGNAL_TIMER_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/xiaoman-activity-signal-timer-observation-smoke.sh`

<!-- /preserved-rule: root-071 -->

<a id="root-072"></a>

## Commands — root-072

<!-- preserved-rule: root-072 -->

- Xiaoman activity promotion starter timer observation smoke:
  `QINTOPIA_XIAOMAN_ACTIVITY_PROMOTION_STARTER_TIMER_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/xiaoman-activity-promotion-starter-timer-observation-smoke.sh`

<!-- /preserved-rule: root-072 -->

<a id="root-073"></a>

## Commands — root-073

<!-- preserved-rule: root-073 -->

- Xiaoman activity downstream observation smoke:
  `QINTOPIA_XIAOMAN_ACTIVITY_DOWNSTREAM_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/xiaoman-activity-downstream-observation-smoke.sh`

<!-- /preserved-rule: root-073 -->

<a id="root-074"></a>

## Commands — root-074

<!-- preserved-rule: root-074 -->

- Xiaoman activity send request starter observation smoke:
  `QINTOPIA_XIAOMAN_ACTIVITY_SEND_REQUEST_STARTER_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/xiaoman-activity-send-request-starter-observation-smoke.sh`

<!-- /preserved-rule: root-074 -->

<a id="root-075"></a>

## Commands — root-075

<!-- preserved-rule: root-075 -->

- Xiaoman activity image generation starter observation smoke:
  `QINTOPIA_XIAOMAN_ACTIVITY_IMAGE_GENERATION_STARTER_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/xiaoman-activity-image-generation-starter-observation-smoke.sh`

<!-- /preserved-rule: root-075 -->

<a id="root-077"></a>

## Commands — root-077

<!-- preserved-rule: root-077 -->

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

<!-- /preserved-rule: root-077 -->

<a id="root-078"></a>

## Commands — root-078

<!-- preserved-rule: root-078 -->

- The official Feishu OpenAPI MCP (`@larksuiteoapi/lark-mcp`) exists but is not mounted
  on group-facing profiles: it is beta, unmaintained since 2025-08, takes app secrets as
  process arguments, and exposes generic Base access without the read-through output
  sanitization. There is no official Feishu CLI; the audited read-through worker remains
  the Xiaoman read channel.

<!-- /preserved-rule: root-078 -->

<a id="root-079"></a>

## Commands — root-079

<!-- preserved-rule: root-079 -->

- The production activity plan table column `下周排期确认` is a single-select with
  options `待确认（居民提交表单后默认）`, `已确认-排入下周`, `暂缓` (verified against
  the Feishu fields API on 2026-08-10). Consumers must substring-match these values, not
  exact-match short forms like `已确认`.

<!-- /preserved-rule: root-079 -->

<a id="root-125"></a>

## Commands — root-125

<!-- preserved-rule: root-125 -->

- Xiaoman activity production preflight smoke:
  `QINTOPIA_XIAOMAN_ACTIVITY_PRODUCTION_PREFLIGHT_ENABLE=1 deploy/sidecar/scripts/xiaoman-activity-production-preflight-smoke.sh`

<!-- /preserved-rule: root-125 -->

<a id="root-133"></a>

## Core Rules — root-133

<!-- preserved-rule: root-133 -->

- On macOS, run the complete sidecar unit suite with
  `RUST_MIN_STACK=33554432 cargo test --manifest-path runtime/sidecar/Cargo.toml`. The
  default test-thread stack can overflow in an existing Xiaoman async test; see
  `docs/reports/2026-07-13-rust-test-stack-limit.md`.

<!-- /preserved-rule: root-133 -->

<a id="root-171"></a>

## Core Rules — root-171

<!-- preserved-rule: root-171 -->

- Postgres/AgentOS is the system fact source. Feishu is a human workbench and mirror,
  not the source of truth.

<!-- /preserved-rule: root-171 -->

<a id="root-186"></a>

## Core Rules — root-186

<!-- preserved-rule: root-186 -->

- `agents/xiaoman/profile-bundle/migrate_values.py --apply` is a one-time manual
  observation prerequisite. It must require root and the exact owner approval before
  reading the fixed live files, lock both reviewed source hashes, validate exactly four
  values, prove complete rendered parity, and no-clobber create only the root-owned mode
  `0600` `/etc/qintopia/xiaoman-profile-bundle-values.json`. It must not accept path
  overrides, print values, edit the live profile, create symlinks, restart Hermes, use
  the network, write Postgres/Feishu, call external adapters, publish, or send. The
  deploy runner must never invoke it automatically.

<!-- /preserved-rule: root-186 -->

<a id="root-187"></a>

## Core Rules — root-187

<!-- preserved-rule: root-187 -->

- Xiaoman activity signal intake uses `xiaoman-activity signal-ingest` to create
  `xiaoman.create_activity_request` through the operations control plane with
  `requester_agent=default` and `target_agent=xiaoman`; do not bypass capability policy
  by making Xiaoman call its own provider capability directly.

<!-- /preserved-rule: root-187 -->

<a id="root-188"></a>

## Core Rules — root-188

<!-- preserved-rule: root-188 -->

- `qintopia_xiaoman_activity_list_by_date` may execute read-through only when
  `QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE=1` and Feishu Base mode is explicitly
  selected with `QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE=1`. In that mode it may run
  the configured sidecar for read-only, non-dry-run queries and return sanitized
  `record_count`, `records`, and `summaries`; write wrappers must continue to return
  bounded worker commands. The read-through worker path validator must follow the
  release integrity model: root-owned `0755` release roots and sidecar binaries are
  acceptable because the `ubuntu` runtime user cannot modify them; runtime-user-owned
  writable paths and group/world-writable paths must still be rejected.

<!-- /preserved-rule: root-188 -->

<a id="root-189"></a>

## Core Rules — root-189

<!-- preserved-rule: root-189 -->

- Xiaoman prompt rules for immediate Feishu/Base queries must require explicit requester
  authorization and current conversation visibility checks before inlining table names,
  counts, or record summaries. If either boundary is unclear, the reply must ask for
  authorization or move to a controlled private/review channel instead of exposing row
  summaries in the current conversation.

<!-- /preserved-rule: root-189 -->

<a id="root-194"></a>

## Core Rules — root-194

<!-- preserved-rule: root-194 -->

- Xiaoman signal apply smokes should use sanitized non-UUID event signal ids unless a
  matching `qintopia_agent_os.event_signals` row is created first; UUID
  `event_signal_id` values are stored as `source_event_signal_id` and must satisfy the
  Postgres foreign key.

<!-- /preserved-rule: root-194 -->

<a id="root-269"></a>

## Core Rules — root-269

<!-- preserved-rule: root-269 -->

- The `feishu_task_dry_run` workbench mirror must preserve immediate `child_status_refs`
  and expose nested work only through sanitized `descendant_status_refs` with direct
  parent and depth. It must not copy raw payloads, call Feishu, or make the workbench a
  fact source. Keep the description bounded and report truncation explicitly.

<!-- /preserved-rule: root-269 -->

<a id="sidecar-025"></a>

## Rules — sidecar-025

<!-- preserved-rule: sidecar-025 -->

- New built-in capabilities must be registered in both `builtin_capability()` and
  `BUILTIN_CAPABILITY_KEYS`, with matching capability-list smoke expectations. If a new
  root capability should participate in Xiaoman downstream starters, update every
  candidate selector for child creation, image generation, and send-request staging.

<!-- /preserved-rule: sidecar-025 -->

<a id="sidecar-033"></a>

## Rules — sidecar-033

<!-- preserved-rule: sidecar-033 -->

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

<!-- /preserved-rule: sidecar-033 -->

<a id="sidecar-034"></a>

## Rules — sidecar-034

<!-- preserved-rule: sidecar-034 -->

- When `QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE=1` and
  `QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY` are configured, legacy V2 operations intake
  must fail closed before session trust, Postgres, or workflow mutation. Unset or `0`
  hook enablement must keep authenticated ingress disabled even when other ingress env
  exists; other enablement values must fail closed. Do not allow a caller to downgrade
  from authenticated Feishu ingress to the local V2 direct workflow path.

<!-- /preserved-rule: sidecar-034 -->

<a id="sidecar-037"></a>

## Rules — sidecar-037

<!-- preserved-rule: sidecar-037 -->

- A canary review apply must provide expected artifact type and review status
  preconditions. The sidecar must enforce them again under the artifact row lock before
  changing review state, and before authenticated Feishu revalidation, so a mistaken
  generated-image UUID cannot be approved through a poster-brief workflow.

<!-- /preserved-rule: sidecar-037 -->

<a id="sidecar-055"></a>

## Rules — sidecar-055

<!-- preserved-rule: sidecar-055 -->

- `xiaoman-profile-bundle-observation-smoke.sh` may only verify reviewed source hashes
  and byte parity after rendering into a temporary directory. It must not print
  server-local identity values, create symlinks, edit live profile files, restart
  Hermes, write Postgres/Feishu, use external adapters, or send.

<!-- /preserved-rule: sidecar-055 -->
