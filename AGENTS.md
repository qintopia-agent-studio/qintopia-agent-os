# Project Instructions

## Map

- Human entrypoint: `README.md`
- Agent-facing rules: `AGENTS.md`
- Claude Code rules: `CLAUDE.md`
- Documentation hub: `docs/README.md`
- Architecture overview: `docs/architecture/agent-os-overview.md`
- Product scope: `docs/product/agent-os-prd.md`
- Agent OS design: `docs/agent-os/README.md`
- Runtime baseline: `docs/operations/runtime-baseline.md`
- Collaboration model: `docs/engineering/collaboration-model.md`
- Migration policy: `docs/engineering/migration-policy.md`
- Server change policy: `docs/engineering/server-change-policy.md`
- Programming agent guardrails: `docs/engineering/programming-agent-guardrails.md`
- Change routing index: `docs/engineering/change-routing-index.md`
- Current roadmap: `docs/plans/active/current-roadmap.md`
- Source document inventory: `docs/operations/source-document-inventory.md`
- Registry indexes: `registry/`
- Agent packages: `agents/`
- Skill packages: `skills/`
- Workflow packages: `workflows/`
- MCP adapters: `mcp/`
- Runtime templates: `runtime/`
- Deployment scripts and manifests: `deploy/`
- Engineering docs: `docs/engineering/`
- Operations docs: `docs/operations/`
- Fixtures and replay data: `fixtures/`
- Historical POC material: `deprecated/`

## Commands

- Install dependencies: `pnpm install`
- Format: `pnpm format`
- Pre-commit quick checks: `.husky/pre-commit`
- Repository check: `pnpm check`
- Markdown lint: `pnpm lint:md`
- PR readiness: `pnpm pr:doctor`
- PR body validation: `pnpm pr:check-body`
- PR creation: `pnpm pr:create -- --body-file <completed-pr-body.md>`
- Release Please PR manual CI validation:
  `gh workflow run ci.yml --ref <release-please-head-branch> -f release_please_pr_number=<pr-number>`
- If the local pnpm version shim cannot verify a registry signature, do not set
  `pmOnFail=ignore`. Confirm the exact `package.json` script first; when it is a fixed
  repository-local Node entrypoint, run that entrypoint directly and record the failed
  pnpm validation attempt.
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
- Huabaosi image generation disabled-state production observation smoke:
  `QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/huabaosi-image-generation-production-observation-smoke.sh`
- Xiaoman activity production preflight smoke:
  `QINTOPIA_XIAOMAN_ACTIVITY_PRODUCTION_PREFLIGHT_ENABLE=1 deploy/sidecar/scripts/xiaoman-activity-production-preflight-smoke.sh`
- AgentOS downstream evidence/visual timers observation smoke:
  `QINTOPIA_OPERATIONS_DOWNSTREAM_TIMERS_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/operations-downstream-timers-observation-smoke.sh`

Use `rg` and `rg --files` for search.

## Core Rules

- Organize by Agent OS capability, not by programming language.
- Rust, Python, TypeScript, shell, and SQL are implementation details inside a package.
- Do not create top-level `python/`, `rust/`, `typescript/`, or similar language
  buckets.
- On macOS, run the complete sidecar unit suite with
  `RUST_MIN_STACK=33554432 cargo test --manifest-path runtime/sidecar/Cargo.toml`. The
  default test-thread stack can overflow in an existing Xiaoman async test; see
  `docs/reports/2026-07-13-rust-test-stack-limit.md`.
- The ignored `group_message_send` PostgreSQL integration test may run only with
  `QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE=1` against a database named exactly
  `qintopia_test` on loopback and with the explicit Cargo feature
  `postgres-integration-tests`. It validates internal send-ready state and must never
  call QiWe or an external adapter.
- Do not develop directly on `master`; create a feature branch first.
- Document first for new features, behavior changes, migrations, runtime changes, or
  production-adjacent work.
- Use Conventional Commits for commit messages. Allowed types are `build`, `chore`,
  `ci`, `docs`, `feat`, `fix`, `perf`, `refactor`, `revert`, `style`, and `test`.
- Do not manually edit root `CHANGELOG.md` in ordinary feature or fix PRs. Release
  Please owns routine release changelog updates from merged Conventional Commits.
- Merging a Release Please PR prepares a version and draft GitHub Release. Production
  deployment still requires the owner to manually publish that draft Release.
- Do not merge a Release Please PR unless the draft GitHub Release will be published or
  intentionally deleted in the same release decision. The repository release manifest
  must track the latest published Release tag; deleted draft-only releases must not
  remain as the Release Please baseline.
- A Release Please PR created or updated with `GITHUB_TOKEN` may have no automatic PR
  checks because GitHub suppresses recursive workflow triggers. Before merging such a
  PR, run the manual CI validation command on its exact head branch and require the
  workflow `changes` and `check` jobs plus the PR-attached `Release Please validation`
  commit status to pass. The dispatch must fail if the PR is not open, does not target
  `master`, is not bot-authored, or the checked-out SHA differs from the PR head.
- Do not hand humans a prefilled GitHub compare URL as the normal PR flow. Use
  `pnpm pr:doctor`, then `pnpm pr:create` with a completed PR body. If GitHub CLI is
  missing, run `pnpm pr:bootstrap` and follow `gh auth login`.
- PR-Agent must not automatically edit PR descriptions. The completed repository PR
  template is author-owned because CI validates its required sections.
- Do not introduce Java, Gradle, Maven, Kotlin, Go, Swift, C#, PHP, Ruby, Elixir, or a
  new language/toolchain stack without an explicit owner-approved architecture decision.
- Do not hot-edit production servers.
- Do not copy secrets, live `.env` files, tokens, table ids, private chat logs, raw
  member profiles, or server-only runtime state into git.
- WorkTool is not a Qintopia Agent OS channel for new work. Treat WorkTool and the
  WorkTool Hermes plugin as deprecated or audit-only material.
- Hermes Kanban is not the future task/orchestration backbone. Do not build new
  workflows on Hermes Kanban.
- Postgres/AgentOS is the system fact source. Feishu is a human workbench and mirror,
  not the source of truth.
- Hermes remains the Agent runtime. It should not become the business database.
- Xiaoman activity signal intake uses `xiaoman-activity signal-ingest` to create
  `xiaoman.create_activity_request` through the operations control plane with
  `requester_agent=default` and `target_agent=xiaoman`; do not bypass capability policy
  by making Xiaoman call its own provider capability directly.
- Xiaoman signal apply smokes should use sanitized non-UUID event signal ids unless a
  matching `qintopia_agent_os.event_signals` row is created first; UUID
  `event_signal_id` values are stored as `source_event_signal_id` and must satisfy the
  Postgres foreign key.
- Xiaoman `status-update` and `gap-update` may only mutate Xiaoman-owned Postgres
  `event_signals` by internal `event_signal_id` with an explicit UUID `mutation_id`.
  Each apply must update one allowlisted field and append one `event_signal_mutations`
  audit row transactionally. Do not accept Feishu record ids, write Feishu, send QiWe,
  or reuse these commands for arbitrary metadata updates.
- `run-xiaoman-activity-signal-worker` only scans eligible Xiaoman `event_signals` and
  submits the existing `xiaoman-activity signal-ingest` work item contract. It must not
  write Feishu, send QiWe messages, create visual assets, or be added to production
  scheduling without owner-reviewed runtime changes.
- `qintopia-agentos-xiaoman-activity-signal-worker.timer` may only run
  `run-xiaoman-activity-signal-worker --once --apply` for AgentOS work item intake. Do
  not repurpose it for Feishu writeback, QiWe sends, visual asset creation, or external
  adapters.
- `run-xiaoman-activity-promotion-starter-worker` may only create missing AgentOS
  evidence/visual child `work_items` under existing Xiaoman activity request parents. It
  must not execute evidence retrieval, visual generation, Feishu writeback, QiWe sends,
  group-send readiness, or external adapters.
- `qintopia-agentos-xiaoman-activity-promotion-starter-worker.timer` may only run
  `run-xiaoman-activity-promotion-starter-worker --once --apply` for AgentOS child work
  item intake. Do not repurpose it for evidence execution, visual generation, Feishu
  writeback, QiWe sends, group-send readiness, or external adapters.
- `xiaoman-activity-downstream-observation-smoke.sh` is a read-only production
  observation check for existing evidence and visual workers. It may only run
  `run-evidence-worker --once --dry-run` and
  `run-collaboration-worker --work-item-type visual_asset_request --once --dry-run`; do
  not turn it into an apply smoke, Feishu write, QiWe send, poster generation, or
  external adapter trigger.
- Evidence and visual worker reports must derive `dry_run` from `apply_requested` so a
  `--dry-run` observation cannot report `dry_run=false`; preflight must fail closed on
  any mismatch rather than weakening that assertion.
- `qintopia-agentos-operations-evidence-worker.timer` may only run
  `run-evidence-worker --once --apply` for internal `evidence_summary` artifact writes.
  Xiaoman activity evidence with `source_type=event_signal` must resolve
  `source_event_signal_id` to explicitly linked Postgres messages, with a same-chat
  bounded-window local keyword fallback. It must fail closed when no source evidence
  exists and must not export platform message ids, raw chat ids, sender ids, or
  unbounded raw chat. Do not repurpose it for Feishu writeback, QiWe sends, external
  Wenyuange or embedding search, raw message export, or external adapters.
- `qintopia-agentos-operations-visual-worker.timer` may only run
  `run-collaboration-worker --work-item-type visual_asset_request --once --apply` for
  internal pending `poster_brief` artifact writes. For `activity_promotion`, it must
  wait for the sibling completed `evidence_summary`; do not repurpose it for Huabaosi
  production generation, Feishu writeback, QiWe sends, group-send readiness, or external
  adapters.
- `run-xiaoman-activity-send-request-starter-worker` may only create an
  `awaiting_publish` AgentOS `erhua.send_group_message` / `group_message_request` child
  from an approved Xiaoman `generated_image` whose image-generation request is
  completed. It must not record final confirmation, queue the group message, run
  send-ready, publish, call QiWe, write Feishu, or call external adapters.
- `xiaoman-activity-send-request-starter-observation-smoke.sh` is read-only unless a
  reviewed timer exists and may run the starter in `--check-only` mode only. Do not turn
  it into an apply smoke, final confirmation, send-ready worker, Feishu write, QiWe
  send, or external adapter trigger.
- `qintopia-agentos-xiaoman-activity-send-request-starter-worker.timer` may only run
  `run-xiaoman-activity-send-request-starter-worker --once --apply` for AgentOS
  awaiting-publish group message request intake. Do not repurpose it for final
  confirmation, queueing, send-ready, Feishu writeback, QiWe sends, or external
  adapters.
- `run-xiaoman-activity-image-generation-starter-worker` may only create an
  `image_generation_request` from an approved Xiaoman `poster_brief`; it must not call
  an image provider, upload media, write Feishu, send QiWe, or publish.
- `qintopia-agentos-xiaoman-activity-image-generation-starter-worker.timer` may only run
  `run-xiaoman-activity-image-generation-starter-worker --once --apply` for AgentOS
  image-generation request intake. Do not repurpose it for provider calls, media upload,
  generated-image creation, Feishu writeback, QiWe sends, or publishing.
- `run-huabaosi-image-generation-worker` defaults to
  `QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED=0`. Until a provider, isolated media
  storage, host allowlist, staged smoke, rollback owner, and owner-reviewed runtime
  configuration exist, it may only validate and preview requests. It must not create a
  `generated_image` artifact, contact an external service, or be attached to a timer.
- `operations-artifact-review-decision` may approve a `generated_image` only after its
  Huabaosi worker provenance, HTTPS URI, sha256, PNG metadata, source brief/prompt refs,
  and `generated_image_created` audit match its image-generation request. Integrity
  denial must leave the artifact pending and must not complete the work item or unlock
  downstream send intake.
- When the Huabaosi adapter is explicitly enabled in an approved staging boundary, it
  may retry only provider transport failures and HTTP 408, 429, or 5xx responses. It
  must stop after three total attempts, use delayed requeueing, and record only
  sanitized attempt/stage/outcome metadata. Authentication, payload, PNG, media upload,
  readback, persistence, and claim failures are terminal and must not be retried. When
  explicitly enabled in a reviewed staging configuration, every provider, upload, and
  readback response must be size-capped before parsing, and an already reviewed
  `generated_image` must never be overwritten or returned to `pending` by a retry. Every
  outbound HTTP header name/value must reject control characters before socket
  connection. Each work-item claim must use a unique token; artifact or failure writes
  must lock and match that unexpired token, with exactly one affected work-item row.
- `huabaosi-image-generation-production-observation-smoke.sh` may only verify that the
  provider worker remains disabled and unscheduled, run configuration preflight, and run
  `run-huabaosi-image-generation-worker --once --dry-run` for a read-only queue preview.
  It must not use `--apply`, contact provider/media endpoints, write Postgres or Feishu,
  call QiWe, create a generated image, or publish.
- `huabaosi-image-generation-preflight` may only validate and emit a sanitized summary
  of local image-adapter configuration. It must not open network or database
  connections, reveal configuration values, enable generation, write Feishu, send QiWe,
  or publish.
- `huabaosi-image-generation-staging-smoke.sh` may only run one owner-approved staging
  image request after the fail-closed preflight, explicit smoke flag and approval
  phrase, staging-only env file, matching staging database URL hash, and an explicit
  UUID work item id. It must leave the image pending review and must not run in
  production, add a timer, write Feishu, send QiWe, or publish.
- `operations-group-send-ready-timer-observation-smoke.sh` may only inspect the group
  send-ready systemd timer, unit commands, and sanitized journal output. It must not run
  the worker, record final confirmation, write Postgres, call QiWe, or send externally.
- `qiwe-image-send-preflight` may only validate the disabled async URL-upload/send-image
  contract from local configuration. It must not open network or database connections,
  emit tokens, device/group ids, media URLs, file credentials, or message identifiers,
  write Feishu, or send externally. Do not add a QiWe send worker or timer until staging
  proves complete `cmd=20000` callback credentials and resolves the current PNG/JPG
  compatibility gap. Final request construction must recheck the target group allowlist,
  response parsing must fail closed unless both `code=0` and `isSendSuccess=1`, and this
  disabled-state preflight must fail when the send-enable flag is `1`. All future
  outbound header values must reject every control character before socket connection.
- `xiaoman-activity-production-preflight-smoke.sh` is a read-only composition of Xiaoman
  timer observation smokes, shared evidence/visual timer observation, Xiaoman downstream
  evidence/visual preview, and the group send-ready timer observation. It must not set
  apply-smoke flags, deploy units, publish releases, write Feishu, call QiWe, run the
  send-ready worker, or run external adapters.
- `operations-work-item-status` must resolve nested work items to the top-level workflow
  root and report every descendant while preserving each direct `parent_work_item_id`.
  `operations-workflow-sync` may persist that recursive AgentOS summary, but neither
  command may execute workers, schedule a general DAG, call external adapters, or send.
- The `feishu_task_dry_run` workbench mirror must preserve immediate `child_status_refs`
  and expose nested work only through sanitized `descendant_status_refs` with direct
  parent and depth. It must not copy raw payloads, call Feishu, or make the workbench a
  fact source. Keep the description bounded and report truncation explicitly.
- `install-release-systemd-units.sh` may only render units from the promoted immutable
  release, install its fixed allowlist, and enable AgentOS internal workflow timers. Do
  not extend it to execute arbitrary commands, enable Feishu/QiWe/external adapters, or
  source a writable server checkout.
- The first release containing a deploy-runner behavior change is processed by the
  previous runner. Use a reviewed follow-up `workflow_dispatch` request for the same
  published SHA to activate the new runner behavior; do not bootstrap it with server
  edits.
- `xiaoman-postgres-integration` in GitHub Actions may enable the guarded apply smoke
  only against its disposable `qintopia_test` PostgreSQL service. It must not use a
  production database URL, secrets, Feishu, QiWe, or external adapters.

## Package Placement

- Agent profile, prompt, allowed skills, memory policy, and forbidden actions:
  `agents/<agent>/`.
- Reusable channel or business capability: `skills/<capability>/`.
- Cross-Agent business process: `workflows/<workflow>/`.
- MCP server or adapter: `mcp/<adapter>/`.
- Runtime template or render/check logic: `runtime/<runtime-area>/`.
- Release, smoke, rollback, or server install logic: `deploy/<area>/`.
- Historical POC or removed direction: `deprecated/<topic>/`.

## Package Contract

Every adopted package should eventually include:

- `README.md`
- `manifest.yaml`, `agent.yaml`, or `workflow.yaml`
- `tests/` or `fixtures/`
- owner and risk level
- validation command
- production boundary
- rollback or decommission notes when relevant

Do not migrate a package as production-ready until these are present or there is a
documented exception.

## Migration Rules

Migration is inventory-first:

1. Identify the current source path.
2. Record whether it is `adopt`, `template`, `runtime-only`, `deprecated`, or `remove`.
3. Preserve source hash or commit reference.
4. Add package metadata.
5. Add focused tests or fixtures.
6. Only then wire it into registry and deployment.

Server runtime directories under `.hermes/profiles/*` must be treated as live runtime
state. They can produce inventory records, templates, or diffs; they must not be copied
wholesale into this repository.

## Server Change Policy

The server is a deployment target, not an editing workspace.

Allowed server activity:

- read-only inventory
- service status checks
- log inspection
- smoke checks
- deploying an approved commit SHA through a runbook
- emergency rollback with a follow-up patch and owner record

Disallowed server activity:

- editing docs directly
- editing code directly
- editing `.hermes` runtime files directly
- scp overwrites of single source files
- committing unreviewed experiments on the server and treating them as product direction

## Validation Expectations

Before a PR:

- Run package-level tests.
- Run fixture replay when available.
- Run registry/manifest checks when available.
- Validate the completed PR body with `pnpm pr:check-body` or `pnpm pr:doctor`.
- For runtime/deploy changes, include dry-run output and rollback notes.
- For user-facing HTML reports, run HTML parse and browser overflow checks.
- For production-adjacent changes, state whether the change touches external sends,
  database writes, profile runtime, secrets, Feishu, QiWe, or systemd.

## Documentation Rules

- Keep decisions in git, not only in chat.
- For every production, deploy, preflight, or CI integration failure, add or update a
  dated, indexed record under `docs/reports/` in the same PR. Include the observed
  evidence, root cause, resolution, validation, remaining boundary, and follow-up owner
  action. Update affected runbooks, package READMEs, manifests, or checks in that same
  PR; do not leave the repair documented only in a report or chat.
- Prefer short, focused docs over one large manual.
- Mark server-side exploration as unapproved until owner review confirms it.
- Avoid formalistic phrasing when writing internal engineering docs.
- Keep technical reports concrete: current state, evidence, risk, next action.

## First Read For New Agents

1. `README.md`
2. `AGENTS.md`
3. `docs/README.md`
4. `docs/architecture/agent-os-overview.md`
5. `docs/plans/active/current-roadmap.md`
6. `docs/engineering/programming-agent-guardrails.md`
7. `docs/engineering/change-routing-index.md`
8. `docs/product/agent-os-prd.md` for product scope changes
9. `docs/agent-os/README.md` for Agent OS design changes
10. `docs/plans/completed/monorepo-migration.md` for historical migration evidence
11. Target package README or manifest
12. Relevant docs under `docs/engineering/` or `docs/operations/`

Report what you read, what you plan to touch, validation commands, and production
boundaries before making broad changes.
