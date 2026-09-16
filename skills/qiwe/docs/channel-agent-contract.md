# QiWe channel contract

These are retained operating constraints, not a new permission grant. Read the sections
relevant to the change before editing. Original conditions and exceptions remain
binding; summaries in AGENTS.md do not replace them.

Backtick paths in root-source rules are repository-relative; paths in Sidecar-source
rules retain their original `runtime/sidecar/` base. New navigation links are relative
to this document.

[Migration inventory](../../../docs/plans/active/agents-guidance/README.md) records the
baseline and source locations.

<a id="root-067"></a>

## Commands — root-067

<!-- preserved-rule: root-067 -->

- Staging-only sidecar artifact for Huabaosi/QiWe evidence smokes:
  `pnpm artifact:sidecar:staging`

<!-- /preserved-rule: root-067 -->

<a id="root-068"></a>

## Commands — root-068

<!-- preserved-rule: root-068 -->

- Independent QiWe production sidecar artifact: `pnpm artifact:sidecar:qiwe-production`

<!-- /preserved-rule: root-068 -->

<a id="root-069"></a>

## Commands — root-069

<!-- preserved-rule: root-069 -->

- Independent QiWe production sidecar artifact prune:
  `pnpm artifact:prune:sidecar:qiwe-production`

<!-- /preserved-rule: root-069 -->

<a id="root-102"></a>

## Commands — root-102

<!-- preserved-rule: root-102 -->

- Real Xiaoman activity production evidence export after owner-confirmed completion:

  ```bash
  QINTOPIA_XIAOMAN_REAL_ACTIVITY_PRODUCTION_SIDECAR_SHA256=<approved-qiwe-production-sidecar-sha256> \
  QINTOPIA_XIAOMAN_REAL_ACTIVITY_PRODUCTION_DATABASE_URL_SHA256=<approved-production-database-url-sha256> \
  qintopia-message-sidecar xiaoman-real-activity-production-evidence \
    --workflow-root-id <completed-xiaoman-activity-root-uuid> > production-evidence-output.txt
  ```

<!-- /preserved-rule: root-102 -->

<a id="root-116"></a>

## Commands — root-116

<!-- preserved-rule: root-116 -->

- Xiaoman QiWe group-arrival human confirmation evidence validation after a real
  activity send:
  `node tools/deploy/check-xiaoman-qiwe-group-arrival-confirmation-evidence.mjs <production-evidence-output.txt> <qiwe-group-arrival-confirmation-output.txt>`

<!-- /preserved-rule: root-116 -->

<a id="root-117"></a>

## Commands — root-117

<!-- preserved-rule: root-117 -->

- Xiaoman completion manifest inputs must keep the exact PR head SHA values. For a
  squash-merged or otherwise non-linear QiWe production enablement PR, the manifest
  builder verifies inclusion through the PR merge commit; do not substitute the merge
  commit for `--qiwe-production-enablement-head-sha`. If GitHub's PR status rollup omits
  the manual CI `changes` or `check` job, the builder verifies those jobs through the
  successful `Release Please validation` workflow run URL.

<!-- /preserved-rule: root-117 -->

<a id="root-121"></a>

## Commands — root-121

<!-- preserved-rule: root-121 -->

- Combined Huabaosi/QiWe staging runtime readiness evidence:

  ```bash
  QINTOPIA_STAGING_RUNTIME_READINESS_EVIDENCE_ENABLE=1 \
  QINTOPIA_STAGING_RUNTIME_RELEASE_SHA=<approved-staging-release-sha> \
  QINTOPIA_STAGING_RUNTIME_SIDECAR_SHA256=<approved-staging-sidecar-sha256> \
  QINTOPIA_STAGING_RUNTIME_DATABASE_URL_SHA256=<approved-staging-database-url-sha256> \
    deploy/sidecar/scripts/staging-runtime-readiness-evidence-smoke.sh
  ```

<!-- /preserved-rule: root-121 -->

<a id="root-134"></a>

## Core Rules — root-134

<!-- preserved-rule: root-134 -->

- The ignored `group_message_send` PostgreSQL integration test may run only with
  `QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE=1` against a database named exactly
  `qintopia_test` on loopback and with the explicit Cargo feature
  `postgres-integration-tests`. It validates internal send-ready state and must never
  call QiWe or an external adapter.

<!-- /preserved-rule: root-134 -->

<a id="root-144"></a>

## Core Rules — root-144

<!-- preserved-rule: root-144 -->

- Authenticated QiWe event provenance is transport-owned. The sidecar may set
  `ingress_auth_verified=true` only because it actually received the event on the exact
  configured authenticated NATS subject while the trusted-subject gate is enabled;
  publisher JSON, headers, or a legacy raw subject can never assert that trust.

<!-- /preserved-rule: root-144 -->

<a id="root-147"></a>

## Core Rules — root-147

<!-- preserved-rule: root-147 -->

- Do not describe a Xiaoman-adjacent Release as production-complete unless
  `docs/plans/active/xiaoman-production-completion-gate.md` is satisfied. Infrastructure
  or activation-ready Releases may ship staging/provisioning/deploy tooling, but they
  must not be treated as the usable activity-to-QiWe group-send workflow.

<!-- /preserved-rule: root-147 -->

<a id="root-165"></a>

## Core Rules — root-165

<!-- preserved-rule: root-165 -->

- A production same-SHA follow-up may repair owner and mode metadata or install the
  complete missing QiWe companion only after the existing manifest identity matches the
  request, the complete release tree matches freshly fetched verified artifacts, and
  both packaged checksum files pass. The only missing-content exception is the complete
  `sidecar-profiles/qiwe-production` tree for a legacy Huabaosi-only release; partial
  companion trees fail. The primary Huabaosi payload must never be replaced. Fail before
  metadata mutation on any other content or path drift; do not hot-fix release ownership
  with server-side `chown` or `chmod` outside this reviewed runner path.

<!-- /preserved-rule: root-165 -->

<a id="root-167"></a>

## Core Rules — root-167

<!-- preserved-rule: root-167 -->

- The staging-only sidecar artifact must package the exact Huabaosi and QiWe staging
  smoke runners with manifest and checksum identities. Provision them under the same
  immutable staging release at `deploy/sidecar/scripts/`; a real staging smoke must not
  fall back to a mutable checkout or use test mode because the runner is absent.

<!-- /preserved-rule: root-167 -->

<a id="root-173"></a>

## Core Rules — root-173

<!-- preserved-rule: root-173 -->

- Erhua public local recommendations such as performances, restaurants, cafes, or
  exhibitions must use current public-source checks before claiming "best", consensus,
  availability, price, or firsthand experience. Without verified venue/organizer,
  ticketing, map/review, or local audience evidence, route them as
  `public_source_recommendation` and answer with the lookup path plus uncertainty.
  Xiaohongshu may be used as local audience evidence and search-discovery input, but
  must be cross-checked against ticketing schedules or official venue/organizer accounts
  before claiming availability, lineup, price, or "best". The QiWe adapter must promote
  `public_source_check_required` and its lookup plan into explicit reply directives;
  embedding the raw answer-context JSON alone is not sufficient to keep Xiaohongshu in
  the final reply.

<!-- /preserved-rule: root-173 -->

<a id="root-191"></a>

## Core Rules — root-191

<!-- preserved-rule: root-191 -->

- `qintopia_xiaoman_activity_promotion_review_draft` may only transform already-read
  sanitized Xiaoman activity records into a human-reviewable activity summary, promotion
  assessment, copy draft, poster brief, and dry-run controlled record-path payload. It
  must not read Feishu, write Postgres, call Huabaosi, queue or send QiWe messages,
  publish, or skip human confirmation. Hermes remains the runtime caller, not the
  business fact source.

<!-- /preserved-rule: root-191 -->

<a id="root-195"></a>

## Core Rules — root-195

<!-- preserved-rule: root-195 -->

- Xiaoman `status-update`, `gap-update`, and `phase-update` may only mutate
  Xiaoman-owned Postgres `event_signals` by internal `event_signal_id` with an explicit
  UUID `mutation_id`. Each apply must update one allowlisted field and append one
  `event_signal_mutations` audit row transactionally. Do not accept Feishu record ids,
  write Feishu, send QiWe, or reuse these commands for arbitrary metadata updates.

<!-- /preserved-rule: root-195 -->

<a id="root-196"></a>

## Core Rules — root-196

<!-- preserved-rule: root-196 -->

- `run-xiaoman-activity-signal-worker` only scans eligible Xiaoman `event_signals` and
  submits the existing `xiaoman-activity signal-ingest` work item contract. It must not
  write Feishu, send QiWe messages, create visual assets, or be added to production
  scheduling without owner-reviewed runtime changes.

<!-- /preserved-rule: root-196 -->

<a id="root-197"></a>

## Core Rules — root-197

<!-- preserved-rule: root-197 -->

- `qintopia-agentos-xiaoman-activity-signal-worker.timer` may only run
  `run-xiaoman-activity-signal-worker --once --apply` for AgentOS work item intake. Do
  not repurpose it for Feishu writeback, QiWe sends, visual asset creation, or external
  adapters.

<!-- /preserved-rule: root-197 -->

<a id="root-198"></a>

## Core Rules — root-198

<!-- preserved-rule: root-198 -->

- `run-xiaoman-activity-promotion-starter-worker` may only create missing AgentOS
  evidence/visual child `work_items` under existing Xiaoman activity request parents. It
  must not execute evidence retrieval, visual generation, Feishu writeback, QiWe sends,
  group-send readiness, or external adapters.

<!-- /preserved-rule: root-198 -->

<a id="root-199"></a>

## Core Rules — root-199

<!-- preserved-rule: root-199 -->

- `qintopia-agentos-xiaoman-activity-promotion-starter-worker.timer` may only run
  `run-xiaoman-activity-promotion-starter-worker --once --apply` for AgentOS child work
  item intake. Do not repurpose it for evidence execution, visual generation, Feishu
  writeback, QiWe sends, group-send readiness, or external adapters.

<!-- /preserved-rule: root-199 -->

<a id="root-200"></a>

## Core Rules — root-200

<!-- preserved-rule: root-200 -->

- `xiaoman-activity-downstream-observation-smoke.sh` is a read-only production
  observation check for existing evidence and visual workers. It may only run
  `run-evidence-worker --once --dry-run` and
  `run-collaboration-worker --work-item-type visual_asset_request --once --dry-run`; do
  not turn it into an apply smoke, Feishu write, QiWe send, poster generation, or
  external adapter trigger.

<!-- /preserved-rule: root-200 -->

<a id="root-202"></a>

## Core Rules — root-202

<!-- preserved-rule: root-202 -->

- `qintopia-agentos-operations-evidence-worker.timer` may only run
  `run-evidence-worker --once --apply` for internal `evidence_summary` artifact writes.
  Xiaoman activity evidence with `source_type=event_signal` must resolve
  `source_event_signal_id` to explicitly linked Postgres messages, with a same-chat
  bounded-window local keyword fallback. It must fail closed when no source evidence
  exists and must not export platform message ids, raw chat ids, sender ids, or
  unbounded raw chat. Do not repurpose it for Feishu writeback, QiWe sends, external
  Wenyuange or embedding search, raw message export, or external adapters.

<!-- /preserved-rule: root-202 -->

<a id="root-203"></a>

## Core Rules — root-203

<!-- preserved-rule: root-203 -->

- `qintopia-agentos-operations-visual-worker.timer` may only run
  `run-collaboration-worker --work-item-type visual_asset_request --once --apply` for
  internal pending `poster_brief` artifact writes. For `activity_promotion`, it must
  wait for the sibling completed `evidence_summary`; do not repurpose it for Huabaosi
  production generation, Feishu writeback, QiWe sends, group-send readiness, or external
  adapters.

<!-- /preserved-rule: root-203 -->

<a id="root-205"></a>

## Core Rules — root-205

<!-- preserved-rule: root-205 -->

- `xiaoman-activity-send-request-starter-observation-smoke.sh` is read-only unless a
  reviewed timer exists and may run the starter in `--check-only` mode only. Do not turn
  it into an apply smoke, final confirmation, send-ready worker, Feishu write, QiWe
  send, or external adapter trigger.

<!-- /preserved-rule: root-205 -->

<a id="root-206"></a>

## Core Rules — root-206

<!-- preserved-rule: root-206 -->

- `qintopia-agentos-xiaoman-activity-send-request-starter-worker.timer` may only run
  `run-xiaoman-activity-send-request-starter-worker --once --apply` for AgentOS
  awaiting-publish group message request intake. Do not repurpose it for final
  confirmation, queueing, send-ready, Feishu writeback, QiWe sends, or external
  adapters.

<!-- /preserved-rule: root-206 -->

<a id="root-211"></a>

## Core Rules — root-211

<!-- preserved-rule: root-211 -->

- QiWe outbound text filtering may suppress only complete, narrowly recognized Hermes
  internal-process templates. Every added template needs positive and negative tests;
  never block ordinary answers through broad standalone terms such as `plain text` or
  `纯文本`.

<!-- /preserved-rule: root-211 -->

<a id="root-212"></a>

## Core Rules — root-212

<!-- preserved-rule: root-212 -->

- Hermes/WeCom and QiWe outbound paths must never send raw provider/runtime retry
  diagnostics such as `Retrying in ...`, `API call failed after ...`, HTTP status codes,
  stack traces, paths, record ids, or command text to user chats. Classify them as
  internal process state, keep details in logs/audit, and use a short user-safe Chinese
  fallback only where the reviewed path intentionally sends one.

<!-- /preserved-rule: root-212 -->

<a id="root-219"></a>

## Core Rules — root-219

<!-- preserved-rule: root-219 -->

- Production release requests use `runtime_artifact_profile=huabaosi-production` for the
  primary artifact and install `qiwe-production` as a companion. Do not use the request
  profile as a global runtime switch. A rollback target is valid only when its complete
  primary and companion artifact set has been reviewed for that commit.

<!-- /preserved-rule: root-219 -->

<a id="root-239"></a>

## Core Rules — root-239

<!-- preserved-rule: root-239 -->

- Huabaosi and QiWe external HTTP calls must use the shared bounded Rust client. It must
  reject invalid methods/headers before connect, require HTTPS outside tests, enforce
  header/body/chunk limits while reading, set socket timeouts, zeroize sensitive request
  and response buffers, and classify whether an error occurred after a request may have
  been sent.

<!-- /preserved-rule: root-239 -->

<a id="root-245"></a>

## Core Rules — root-245

<!-- preserved-rule: root-245 -->

- Huabaosi live provider/media helpers may compile only with one reviewed Huabaosi live
  feature: `huabaosi-staging-adapter` for guarded staging or
  `huabaosi-production-adapter` for production. A build containing neither or both must
  reject apply before Postgres or network access. Staging keeps its one-shot owner
  phrase and reviewed staging database hash gate. Production must bind explicit
  enablement to the deployed release SHA and production database URL hash before
  connecting to Postgres. Production artifacts must not contain QiWe live adapter
  features until a separate owner-approved production send boundary exists.

<!-- /preserved-rule: root-245 -->

<a id="root-264"></a>

## Core Rules — root-264

<!-- preserved-rule: root-264 -->

- `operations-group-send-ready-timer-observation-smoke.sh` may only inspect the group
  send-ready systemd timer, unit commands, and sanitized journal output. It must not run
  the worker, record final confirmation, write Postgres, call QiWe, or send externally.

<!-- /preserved-rule: root-264 -->

<a id="root-266"></a>

## Core Rules — root-266

<!-- preserved-rule: root-266 -->

- `xiaoman-activity-production-preflight-smoke.sh` is a read-only composition of Xiaoman
  timer observation smokes, shared evidence/visual timer observation, Xiaoman downstream
  evidence/visual preview, and the group send-ready timer observation. It must not set
  apply-smoke flags, deploy units, publish releases, write Feishu, call QiWe, run the
  send-ready worker, or run external adapters. It must invoke child observations through
  `env -i` with only a fixed PATH, the child enable flag, and the release-local sidecar
  path when present; do not pass caller-provided test overrides, systemctl/journalctl
  overrides, env-file overrides, or ambient deployment secrets into child observations.

<!-- /preserved-rule: root-266 -->

<a id="root-270"></a>

## Core Rules — root-270

<!-- preserved-rule: root-270 -->

- `install-release-systemd-units.sh` may only render units from the promoted immutable
  release, install its fixed allowlist plus the release-owned deploy-runner service and
  timer, and enable AgentOS internal workflow timers. Do not extend it to execute
  arbitrary commands, enable Feishu/QiWe/external adapters, or source a writable server
  checkout.

<!-- /preserved-rule: root-270 -->

<a id="root-278"></a>

## Core Rules — root-278

<!-- preserved-rule: root-278 -->

- `postgres-integration` in GitHub Actions may enable the guarded apply smoke only
  against its disposable `qintopia_test` PostgreSQL service. It must not use a
  production database URL, secrets, Feishu, QiWe, or external adapters.

<!-- /preserved-rule: root-278 -->

<a id="sidecar-015"></a>

## Rules — sidecar-015

<!-- preserved-rule: sidecar-015 -->

- Keep the sidecar independent from 二花's ordinary reply path; NATS, sidecar, or
  Postgres failures must not delay ordinary Hermes webhook ACKs or replies. The sole
  exception is the default-disabled authenticated system-event boundary: when
  `QIWE_SYSTEM_EVENT_DURABLE_CAPTURE_ENABLED=1`, the adapter may spend at most 1.5
  seconds for the complete envelope waiting for every raw JetStream PubAck and must
  return a bounded 503 on any missing or failed acknowledgement so QiWe can retry.

<!-- /preserved-rule: sidecar-015 -->

<a id="sidecar-016"></a>

## Rules — sidecar-016

<!-- preserved-rule: sidecar-016 -->

- Derive authenticated ingress only from the actual NATS subject received by the
  consumer while `QINTOPIA_SIDECAR_TRUST_AUTHENTICATED_RAW_SUBJECT=true`; never trust an
  `ingress_auth_verified` value carried in publisher JSON. Keep the authenticated raw
  subject distinct from legacy raw and normalized-message subjects.

<!-- /preserved-rule: sidecar-016 -->

<a id="sidecar-035"></a>

## Rules — sidecar-035

<!-- preserved-rule: sidecar-035 -->

- The dedicated QiWe production sidecar artifact is separate and compile-reviewed. Its
  manifest profile is `qiwe-production`, its artifact name is
  `qintopia-message-sidecar-qiwe-production-linux-x86_64-gnu`, and it must compile
  exactly `qiwe-production-adapter`. Production deploy requests must record
  `runtime_artifact_profile`, and QiWe enabled-state observations must accept only this
  reviewed artifact profile, never a mixed Huabaosi/QiWe binary.

<!-- /preserved-rule: sidecar-035 -->

<a id="sidecar-036"></a>

## Rules — sidecar-036

<!-- preserved-rule: sidecar-036 -->

- `xiaoman-real-activity-production-evidence` must read the adjacent
  `artifact-manifest.json`, require `commit_sha` to match
  `QINTOPIA_DEPLOYED_COMMIT_SHA`, require `validation.artifact_profile=qiwe-production`,
  and require exactly `validation.cargo_features=["qiwe-production-adapter"]` before
  exporting sanitized evidence. Final completion evidence must also keep the Huabaosi
  canary profile as `huabaosi-production`.

<!-- /preserved-rule: sidecar-036 -->
