# QiWe Skill

Status: adopting source snapshot

This package is the future monorepo home for the QiWe / WeCom Hermes platform adapter.
M4B imports a clean source snapshot from `../qiwei-hermes-plugin@6f69794`. It does not
change production server files.

## Current Source

| Source           | Value                                                       |
| ---------------- | ----------------------------------------------------------- |
| Local repository | `../qiwei-hermes-plugin`                                    |
| Local branch     | `main`                                                      |
| Local reference  | `6f69794`                                                   |
| Local state      | clean                                                       |
| Server checkout  | `/home/ubuntu/.hermes/profiles/erhua/plugins/qiwe-platform` |
| Server branch    | `main`                                                      |
| Server reference | `6f69794`                                                   |
| Server state     | clean tracked files, one untracked historical backup        |

## Production Boundary

Current production route:

```text
https://qintopia.cn/qiwe/webhook
  -> nginx
  -> http://127.0.0.1:18661/qiwe/webhook
  -> hermes-gateway-erhua.service
  -> /home/ubuntu/.hermes/profiles/erhua/plugins/qiwe-platform
```

This skill can touch external QiWe sends, Hermes profile runtime behavior, and server
secrets. Production adoption requires review, smoke checks, and rollback notes.

## Space Configuration And Group Isolation

The three configuration tools, `qintopia_space_change_prepare`,
`qintopia_space_change_confirm`, and `qintopia_space_change_status`, remain
independently available through their trusted-session and administrator-confirmation
boundary. They do not replace or unregister the existing QiWe tools or Erhua
`qintopia-tools` capabilities.

For an ordinary QiWe group turn, setting `QIWE_SPACE_TURN_POLICY_ENFORCEMENT_ENABLED=1`
makes the adapter load identity, knowledge scope, and effective capabilities from the
active policy for the exact current group, then authorize every governed capability
again immediately before invocation. Both operations resolve the current Space and
speaker from the authenticated persisted message receipt; neither accepts model-supplied
room, actor, or destination ids. A missing policy, timeout, malformed response, or
unauthorized capability fails closed. `QIWE_SPACE_TURN_POLICY_TIMEOUT_SECONDS` defaults
to `0.4` seconds.

The switch governs ordinary group turns only. An explicitly authenticated QiWe direct
session keeps the existing direct-tool behavior without projecting a group policy, but
the gateway platform, conversation type, chat, speaker, and message fields must all be
present. Direct tools remain bound to the current direct conversation and speaker; a
direct turn cannot select a group target or another user.

The enforcement switch defaults to `0` for the one-time reviewed rollout. Ordinary
capabilities remain registered, but a governed call is effective only when the current
Space policy grants it, no active revocation subtracts that grant, and the matching
global capability registry row is enabled. The three configuration tools retain their
separate review boundary so an authorized Space administrator can prepare, inspect, and
confirm policy changes even when ordinary business capabilities are empty. Quota
declarations remain validated but explicitly non-enforced in v1.

## Official Event Research Boundary

`QINTOPIA_SPACE_EVENT_RESEARCH_ENABLED` accepts only the exact value `1` and defaults to
disabled. When enabled, the default researcher starts the release-owned
`official_qiwe_research_worker.py` with an isolated Python mode, a fixed minimal
environment, closed inherited file descriptors, no stdin, discarded stderr, a bounded
stdout protocol, and a hard deadline. It passes only bounded depth/page counts. The
worker accepts no URL, query, headers, credentials, proxy settings, or executable path
from the group turn or process environment.

The worker starts only from the two repository-registered Qiwe documentation pages,
follows only normalized `https://doc.qiweapi.com/doc-<number>` links without redirects,
and caps request count, page bytes, visible text, link fanout, crawl depth, page count,
runtime, and result bytes. Both worker and parent independently validate the output.
Retrieved text is always framed as untrusted reference data for the planner; it can
provide event facts but cannot provide instructions, destinations, credentials, tools,
or code.

Clearing the child environment materially reduces credential carriage, but a subprocess
under the same Unix UID is not a credential-isolation boundary: it may still be able to
read files available to that UID. Production research must remain disabled until the
worker runs as a dedicated OS identity or equivalent container with no access to Hermes,
Qiwe, NATS, database, deployment, or operator credential files. Enabling this switch on
the existing same-UID gateway is not production approval.

## Space Agent Completion Boundary

The default-disabled Space agent completion socket reuses the Hermes-owned `ctx.llm`
handle inside the QiWe adapter. It does not load provider configuration or credentials,
execute a capability, select a destination, or send a message. It starts and stops with
the adapter and accepts only a dedicated non-root runner whose Unix peer UID/GID and
bearer SHA-256 both match the reviewed configuration.

The newline-delimited, bounded JSON protocol accepts only
`operation=space_agent_turn_complete`, schema version 1, one work-item UUID, the bounded
goal/trigger/output contract, the broker-issued capability catalog, and at most 16
completed capability calls. Its response always has only `schema_version`, `accepted`,
and `decision`. An accepted decision is either a final output object or one capability
call containing a new UUID, an exact catalog key, and an input object. Authorization,
capability execution, receipts, and final output validation remain sidecar-owned.

Enablement requires `QIWE_SPACE_AGENT_COMPLETION_ENABLED=1`, the exact reviewed approval
phrase, an absolute socket path, the runner UID/GID, and only the runner bearer's
SHA-256. The plaintext bearer belongs solely in the isolated runner environment.

## Current Behavior Summary

- Uses inner QiWe raw event `data.fromRoomId` as the stable group id.
- Replies to group messages only when Erhua is mentioned or clearly cued.
- Keeps direct/private handling behind explicit configuration and contact guards.
- Exposes controlled QiWe channel tools for location cards, direct messages,
  rich/media/card sends, revocation, voice-to-text, direct-contact requests, and human
  handoff.
- Rebuilds asynchronous `cmd=20000` callback capture into hashed correlation and fixed
  field-presence metadata before publishing to NATS. Callback credentials, URLs,
  filenames, message content, identities, and unknown values are not published; the Rust
  sidecar independently enforces the same boundary before Postgres writes. Existing
  callback ids are preserved only when the suffix is a validated 64-hex SHA-256 digest;
  a `qiwe-callback:` prefix by itself is not trusted.
- Publishes ingress-authenticated durable system events only to the separate
  `qintopia.qiwe.raw.authenticated` subject. The producer uses a bounded auth file;
  credentials in the NATS URL are rejected. The sidecar ignores the envelope's
  `ingress_auth_verified` value and derives that fact only from the ACL-protected
  subject. Compatibility-mode or ordinary raw capture therefore cannot self-assert
  trust, drive Space event mappings, or count as real shadow evidence.
- Keeps ordinary message capture best-effort even when NATS is unavailable. The separate
  `QIWE_SYSTEM_EVENT_DURABLE_CAPTURE_ENABLED=1` gate applies only to authenticated
  system events: the whole `data[]` envelope has a fixed 1.5-second budget to receive a
  valid authenticated-raw-subject JetStream PubAck for every system event. Any timeout,
  NATS rejection, malformed acknowledgement, or partial batch returns a fixed HTTP 503
  without provider details so QiWe can retry. The gate is default-disabled and requires
  webhook authentication, a producer auth file, and NATS capture at adapter startup;
  `Nats-Msg-Id` and Postgres event ids retain replay idempotency. Production activation
  additionally requires anonymous publish denial and distinct producer/consumer subject
  ACL evidence; loopback binding alone is not an authentication boundary.
- Plans explicit requests such as "启用 welcome_new_members" as the bounded
  `definition_operation=activate` form. The public tool supplies only the stable
  automation key; the sidecar resolves and digest-binds the current group's latest
  shadow automation and exact dependencies, requires exact same-Space real-event
  evidence for event triggers, and rejects confirmation after any stream-head drift.
  Stored cron, timezone, event binding, and business input are never reconstructed by
  the model. `agent_turn` activation uses the dedicated authenticated broker contract;
  execution remains default-disabled until its isolated OS identity, socket group,
  bearer secret, model adapter, capability gates, and owner runtime approval are
  provisioned.
- Provides a disabled-by-default memory bridge that recognizes `cmd=20000` before
  ordinary Agent dispatch and streams the bounded callback only to
  `process-qiwe-image-send-callback --apply` over child stdin. It requires explicit
  `staging` or `production` processor mode, the matching owner phrase, canonical
  approved database URL hash, explicit image-send and webhook readiness flags, bounded
  sanitized stdout, discarded stderr, and a hard timeout. It never places callback
  credentials in arguments, environment variables, files, NATS, logs, audit records, or
  HTTP responses. An explicitly enabled but invalid bridge returns HTTP 503 so an
  unprocessed callback is not acknowledged and silently lost. Callback detection
  requires the reviewed top-level QiWe success envelope, bounded event list, request id,
  and complete `msgData` core credential fields (`fileAesKey`, `fileId`, `fileMd5`, and
  `fileSize`). `filename`/`fileName` is optional and never becomes a fallback for the
  transaction-locked approved artifact filename; arbitrary nested `cmd=20000` values do
  not bypass ordinary message parsing.
- In staging mode the child receives only the fixed staging database, QiWe adapter,
  owner gate, and host/group allowlist environment. Its processor must be the exact
  `<40-hex-sha>/sidecar/qintopia-message-sidecar` under the fixed owner-reviewed
  `/home/ubuntu/qintopia-agent-os-staging-releases` root.
- In production mode the child receives only the production database/QiWe apply gate and
  the reviewed Huabaosi Feishu primary-storage delivery configuration needed by the
  production sidecar. The processor must be exactly
  `/home/ubuntu/qintopia-agent-os-releases/current/sidecar/qintopia-message-sidecar`,
  with root exactly `/home/ubuntu/qintopia-agent-os-releases/current`; direct release
  directory paths, mutable checkout binaries, staging roots, missing `current` symlinks,
  or sidecar SHA drift fail closed. The release root, current target, sidecar directory,
  and executable may not be group/world-writable, their owners must be root or the
  gateway effective user, and the approved executable SHA-256 is checked during
  configuration and again immediately before spawn.
- Unrelated Hermes, NATS, proxy, and runtime variables are not inherited in either mode.
  The bridge does not enable production timers, publish a Release, approve artifacts, or
  bypass the Rust production apply gate; it only gives the already reviewed sidecar a
  memory-only callback ingress after production deployment and owner activation.
- Supports passive processors such as group-solitaire activity collection when enabled.
- Keeps Feishu activity writes and reminders behind explicit scoped configuration.
- Treats Erhua trainer memory as a controlled context-MCP path, not free-form prompt
  editing.
- Promotes `public_source_check_required` answer context into explicit reply directives,
  including the Xiaohongshu search path. The raw answer-context JSON alone is not a
  reliable final-reply constraint.
- Suppresses narrowly recognized Hermes approval, progress, interruption, formatting
  failure, and traceback messages before QiWe delivery. Ordinary answers that discuss
  plain-text formatting are not suppressed.

## Validation

Package validation:

```bash
pnpm test:qiwe
node tools/deploy/test-qiwe-image-staging-smoke.mjs
```

Focused callback bridge validation:

```bash
cd skills/qiwe
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest tests.test_image_callback_bridge -v
```

M4B validation result on 2026-07-03:

- `Ran 155 tests`
- `OK`

Repository-level validation:

```bash
pnpm check
```

## Server Backup Review

Server untracked file:

```text
/home/ubuntu/.hermes/profiles/erhua/plugins/qiwe-platform/adapter.py.bak.home-group-send-20260607-1050
```

Read-only comparison on 2026-07-03:

| File                                           | SHA-256                                                            |
| ---------------------------------------------- | ------------------------------------------------------------------ |
| `adapter.py`                                   | `01e847d7c1484856c5d86f55378dd0c612a431080318b2c3e8bfe678b6af80bb` |
| `adapter.py.bak.home-group-send-20260607-1050` | `3b6a9099e7d4cda31aa02fdbf1720cc67279bfdd8dce6774dc3e3f92d1e84349` |

Diff stat:

```text
1 file changed, 40 insertions(+), 1922 deletions(-)
```

Conclusion: the backup is an older rollback snapshot from 2026-06-07. It lacks later
tracked behavior such as passive pipeline, NATS capture, rich/revoke/voice/handoff
tools, activity handling, and context preparation. It should not be used as the adoption
source. Keep it as server-side audit evidence until owner approves cleanup.

## M4C Adoption Work

Before production wiring changes:

1. Add deploy smoke and rollback notes.
2. Decide server cutover from the old plugin checkout to this monorepo package.
3. Use reviewed commit SHA deployment only; do not hot-edit the server checkout.
4. Confirm server backup cleanup or archival with owner approval.

## Operating rules

Paths below are repository-relative; Sidecar subsections use `runtime/sidecar/`.

### Commands

- Staging-only sidecar artifact for Huabaosi/QiWe evidence smokes:
  `pnpm artifact:sidecar:staging`
- Independent QiWe production sidecar artifact: `pnpm artifact:sidecar:qiwe-production`
- Independent QiWe production sidecar artifact prune:
  `pnpm artifact:prune:sidecar:qiwe-production`
- Real Xiaoman activity production evidence export after owner-confirmed completion:

  ```bash
  QINTOPIA_XIAOMAN_REAL_ACTIVITY_PRODUCTION_SIDECAR_SHA256=<approved-qiwe-production-sidecar-sha256> \
  QINTOPIA_XIAOMAN_REAL_ACTIVITY_PRODUCTION_DATABASE_URL_SHA256=<approved-production-database-url-sha256> \
  qintopia-message-sidecar xiaoman-real-activity-production-evidence \
    --workflow-root-id <completed-xiaoman-activity-root-uuid> > production-evidence-output.txt
  ```

- Xiaoman QiWe group-arrival human confirmation evidence validation after a real
  activity send:
  `node tools/deploy/check-xiaoman-qiwe-group-arrival-confirmation-evidence.mjs <production-evidence-output.txt> <qiwe-group-arrival-confirmation-output.txt>`
- Xiaoman completion manifest inputs must keep the exact PR head SHA values. For a
  squash-merged or otherwise non-linear QiWe production enablement PR, the manifest
  builder verifies inclusion through the PR merge commit; do not substitute the merge
  commit for `--qiwe-production-enablement-head-sha`. If GitHub's PR status rollup omits
  the manual CI `changes` or `check` job, the builder verifies those jobs through the
  successful `Release Please validation` workflow run URL.
- Combined Huabaosi/QiWe staging runtime readiness evidence:

  ```bash
  QINTOPIA_STAGING_RUNTIME_READINESS_EVIDENCE_ENABLE=1 \
  QINTOPIA_STAGING_RUNTIME_RELEASE_SHA=<approved-staging-release-sha> \
  QINTOPIA_STAGING_RUNTIME_SIDECAR_SHA256=<approved-staging-sidecar-sha256> \
  QINTOPIA_STAGING_RUNTIME_DATABASE_URL_SHA256=<approved-staging-database-url-sha256> \
    deploy/sidecar/scripts/staging-runtime-readiness-evidence-smoke.sh
  ```

### Core Rules

- The ignored `group_message_send` PostgreSQL integration test may run only with
  `QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE=1` against a database named exactly
  `qintopia_test` on loopback and with the explicit Cargo feature
  `postgres-integration-tests`. It validates internal send-ready state and must never
  call QiWe or an external adapter.
- Authenticated QiWe event provenance is transport-owned. The sidecar may set
  `ingress_auth_verified=true` only because it actually received the event on the exact
  configured authenticated NATS subject while the trusted-subject gate is enabled;
  publisher JSON, headers, or a legacy raw subject can never assert that trust.
- Do not describe a Xiaoman-adjacent Release as production-complete unless
  `docs/plans/active/xiaoman-production-completion-gate.md` is satisfied. Infrastructure
  or activation-ready Releases may ship staging/provisioning/deploy tooling, but they
  must not be treated as the usable activity-to-QiWe group-send workflow.
- A production same-SHA follow-up may repair owner and mode metadata or install the
  complete missing QiWe companion only after the existing manifest identity matches the
  request, the complete release tree matches freshly fetched verified artifacts, and
  both packaged checksum files pass. The only missing-content exception is the complete
  `sidecar-profiles/qiwe-production` tree for a legacy Huabaosi-only release; partial
  companion trees fail. The primary Huabaosi payload must never be replaced. Fail before
  metadata mutation on any other content or path drift; do not hot-fix release ownership
  with server-side `chown` or `chmod` outside this reviewed runner path.
- The staging-only sidecar artifact must package the exact Huabaosi and QiWe staging
  smoke runners with manifest and checksum identities. Provision them under the same
  immutable staging release at `deploy/sidecar/scripts/`; a real staging smoke must not
  fall back to a mutable checkout or use test mode because the runner is absent.
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
- `qintopia_xiaoman_activity_promotion_review_draft` may only transform already-read
  sanitized Xiaoman activity records into a human-reviewable activity summary, promotion
  assessment, copy draft, poster brief, and dry-run controlled record-path payload. It
  must not read Feishu, write Postgres, call Huabaosi, queue or send QiWe messages,
  publish, or skip human confirmation. Hermes remains the runtime caller, not the
  business fact source.
- Xiaoman `status-update`, `gap-update`, and `phase-update` may only mutate
  Xiaoman-owned Postgres `event_signals` by internal `event_signal_id` with an explicit
  UUID `mutation_id`. Each apply must update one allowlisted field and append one
  `event_signal_mutations` audit row transactionally. Do not accept Feishu record ids,
  write Feishu, send QiWe, or reuse these commands for arbitrary metadata updates.
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
- `xiaoman-activity-send-request-starter-observation-smoke.sh` is read-only unless a
  reviewed timer exists and may run the starter in `--check-only` mode only. Do not turn
  it into an apply smoke, final confirmation, send-ready worker, Feishu write, QiWe
  send, or external adapter trigger.
- `qintopia-agentos-xiaoman-activity-send-request-starter-worker.timer` may only run
  `run-xiaoman-activity-send-request-starter-worker --once --apply` for AgentOS
  awaiting-publish group message request intake. Do not repurpose it for final
  confirmation, queueing, send-ready, Feishu writeback, QiWe sends, or external
  adapters.
- QiWe outbound text filtering may suppress only complete, narrowly recognized Hermes
  internal-process templates. Every added template needs positive and negative tests;
  never block ordinary answers through broad standalone terms such as `plain text` or
  `纯文本`.
- Hermes/WeCom and QiWe outbound paths must never send raw provider/runtime retry
  diagnostics such as `Retrying in ...`, `API call failed after ...`, HTTP status codes,
  stack traces, paths, record ids, or command text to user chats. Classify them as
  internal process state, keep details in logs/audit, and use a short user-safe Chinese
  fallback only where the reviewed path intentionally sends one.
- Production release requests use `runtime_artifact_profile=huabaosi-production` for the
  primary artifact and install `qiwe-production` as a companion. Do not use the request
  profile as a global runtime switch. A rollback target is valid only when its complete
  primary and companion artifact set has been reviewed for that commit.
- Huabaosi and QiWe external HTTP calls must use the shared bounded Rust client. It must
  reject invalid methods/headers before connect, require HTTPS outside tests, enforce
  header/body/chunk limits while reading, set socket timeouts, zeroize sensitive request
  and response buffers, and classify whether an error occurred after a request may have
  been sent.
- Huabaosi live provider/media helpers may compile only with one reviewed Huabaosi live
  feature: `huabaosi-staging-adapter` for guarded staging or
  `huabaosi-production-adapter` for production. A build containing neither or both must
  reject apply before Postgres or network access. Staging keeps its one-shot owner
  phrase and reviewed staging database hash gate. Production must bind explicit
  enablement to the deployed release SHA and production database URL hash before
  connecting to Postgres. Production artifacts must not contain QiWe live adapter
  features until a separate owner-approved production send boundary exists.
- `operations-group-send-ready-timer-observation-smoke.sh` may only inspect the group
  send-ready systemd timer, unit commands, and sanitized journal output. It must not run
  the worker, record final confirmation, write Postgres, call QiWe, or send externally.
- `xiaoman-activity-production-preflight-smoke.sh` is a read-only composition of Xiaoman
  timer observation smokes, shared evidence/visual timer observation, Xiaoman downstream
  evidence/visual preview, and the group send-ready timer observation. It must not set
  apply-smoke flags, deploy units, publish releases, write Feishu, call QiWe, run the
  send-ready worker, or run external adapters. It must invoke child observations through
  `env -i` with only a fixed PATH, the child enable flag, and the release-local sidecar
  path when present; do not pass caller-provided test overrides, systemctl/journalctl
  overrides, env-file overrides, or ambient deployment secrets into child observations.
- `install-release-systemd-units.sh` may only render units from the promoted immutable
  release, install its fixed allowlist plus the release-owned deploy-runner service and
  timer, and enable AgentOS internal workflow timers. Do not extend it to execute
  arbitrary commands, enable Feishu/QiWe/external adapters, or source a writable server
  checkout.
- `postgres-integration` in GitHub Actions may enable the guarded apply smoke only
  against its disposable `qintopia_test` PostgreSQL service. It must not use a
  production database URL, secrets, Feishu, QiWe, or external adapters.

### Sidecar Rules

- Keep the sidecar independent from 二花's ordinary reply path; NATS, sidecar, or
  Postgres failures must not delay ordinary Hermes webhook ACKs or replies. The sole
  exception is the default-disabled authenticated system-event boundary: when
  `QIWE_SYSTEM_EVENT_DURABLE_CAPTURE_ENABLED=1`, the adapter may spend at most 1.5
  seconds for the complete envelope waiting for every raw JetStream PubAck and must
  return a bounded 503 on any missing or failed acknowledgement so QiWe can retry.
- Derive authenticated ingress only from the actual NATS subject received by the
  consumer while `QINTOPIA_SIDECAR_TRUST_AUTHENTICATED_RAW_SUBJECT=true`; never trust an
  `ingress_auth_verified` value carried in publisher JSON. Keep the authenticated raw
  subject distinct from legacy raw and normalized-message subjects.
- The dedicated QiWe production sidecar artifact is separate and compile-reviewed. Its
  manifest profile is `qiwe-production`, its artifact name is
  `qintopia-message-sidecar-qiwe-production-linux-x86_64-gnu`, and it must compile
  exactly `qiwe-production-adapter` and `huabaosi-feishu-mirror-adapter`, in that
  manifest order. The mirror feature is required for Feishu-backed media delivery;
  `huabaosi-production-adapter`, staging features and other additional features are
  excluded from this companion. Production deploy requests must record
  `runtime_artifact_profile`, and QiWe enabled-state observations must accept only this
  reviewed profile and exact feature pair. See the existing
  [artifact contract](../../docs/operations/sidecar-ci-artifacts.md#artifact-retention)
  and [builder](../../tools/deploy/build-qiwe-production-sidecar-artifact.mjs).
- `xiaoman-real-activity-production-evidence` must read the adjacent
  `artifact-manifest.json`, require `commit_sha` to match
  `QINTOPIA_DEPLOYED_COMMIT_SHA`, require `validation.artifact_profile=qiwe-production`,
  and require `validation.cargo_features` to equal the same ordered two-feature list
  above before exporting sanitized evidence. Final completion evidence must also keep
  the Huabaosi canary profile as `huabaosi-production`.

### Media delivery

#### Commands

- QiWe image-send intro-text production enable is the only reviewed one-shot path for
  turning on the optional chat intro text (`小满日报` / `二花早报` caption before the
  poster image). Use `Run Production Runtime One-Shot` with
  `runtime_one_shot_target=qiwe-image-send-intro-text-enable` and
  `approval=approved-production-qiwe-image-send-intro-text-v1`; it may write only the
  fixed `QINTOPIA_QIWE_IMAGE_SEND_INTRO_TEXT_ENABLED=1` constant to
  `/etc/qintopia/message-sidecar.env`, must fail closed on duplicate/wrong values, and
  must never accept or expose chat ids, group ids, DB hashes, payload JSON, env values,
  or arbitrary config fields. The Rust sidecar reads this flag at `qiwe_image_send.rs`
  (`QINTOPIA_QIWE_IMAGE_SEND_INTRO_TEXT_ENABLED`); default off.

  The Erhua Hermes profile (the gateway that actually sends the `二花早报` card and the
  `小满日报` poster) gets `QINTOPIA_QIWE_IMAGE_SEND_INTRO_TEXT_ENABLED=1` by default
  from `runtime/hermes/migrate_erhua_livecool_env.py` during profile activation, so the
  flag survives re-renders and fresh provisioning without a manual one-shot. An explicit
  `=0` in `/home/ubuntu/.hermes/profiles/erhua/.env` still disables it for rollback
  (re-activate the profile to apply).

- QiWe image-send staging readiness smoke:

  ```bash
  QINTOPIA_QIWE_IMAGE_STAGING_READINESS_ENABLE=1 \
  QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL=approved-staging-qiwe-image-send \
  QINTOPIA_QIWE_IMAGE_STAGING_RELEASE_SHA=<approved-staging-release-sha> \
  QINTOPIA_QIWE_IMAGE_STAGING_SIDECAR_SHA256=<approved-staging-sidecar-sha256> \
    deploy/sidecar/scripts/qiwe-image-send-staging-readiness-smoke.sh
  ```

- QiWe image-send production observation smoke:
  `QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/qiwe-image-send-production-observation-smoke.sh`
- QiWe image callback bridge production observation smoke:
  `QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/qiwe-image-callback-bridge-production-observation-smoke.sh`
- QiWe image callback bridge production activation after manual Release publish and
  persistent Erhua env approval. The activation script validates the bridge is already
  bound to release/current, production mode, the approved sidecar SHA-256, and the
  approved production database URL hash before restarting Erhua; it must not enable
  timers, process callbacks, call QiWe, or source env files:
  `QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_ACTIVATION=approved-production-qiwe-image-callback-bridge deploy/sidecar/scripts/activate-qiwe-image-callback-bridge-production.sh`
- QiWe image callback bridge immediate rollback after persistent Erhua env disables the
  bridge:
  `QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_ROLLBACK=approved-production-qiwe-image-callback-bridge-rollback deploy/sidecar/scripts/rollback-qiwe-image-callback-bridge-production.sh`
- QiWe image-send immediate timer rollback:
  `QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_ROLLBACK=approved-production-qiwe-image-send-rollback deploy/sidecar/scripts/rollback-qiwe-image-send-production.sh`

#### Core Rules

- Keep QiWe adapter and sidecar NATS identities separate. NATS URLs must not contain
  userinfo; load producer and consumer credentials only from their distinct fixed
  private auth files. The producer may publish the authenticated raw subject but must
  not consume it, while the consumer may consume/ack the fixed durable stream but must
  not publish application events. Production Space automation activation must prove
  anonymous denial and these ACLs before any systemd mutation. That protocol preflight
  proves the configured subject/JetStream ACL only; a real shadow callback remains the
  required end-to-end consumption evidence.
- `qintopia_xiaoman_activity_text_group_message_request_prepare` may only prepare an
  `operations-create` command for an `erhua.send_group_message` /
  `group_message_request` from an approved text announcement artifact. It must require
  `approved_artifact_id`, bind `message_text` to the approved artifact `content_hash`,
  keep the request before human final confirmation, and must not queue, run send-ready,
  call Erhua, call QiWe, publish, send, or accept raw group ids, URLs, Feishu/Base ids,
  secrets, or unapproved text. Text announcement MVP evidence must not be used as
  Xiaoman production-complete evidence or as proof of QiWe group delivery; production
  completion still requires the image/send-ready/QiWe arrival evidence checkers.
- Xiaoman activity lifecycle phase is a Postgres `event_signals` fact. Allowed values
  are `pre_event`, `in_event`, and `post_event`; transitions are forward-only and each
  phase maps to its fixed root/child route. Event-signal root creation must lock and
  match the current phase. Do not accept caller-selected routes, rewrite historical
  phase roots, add a timer, or extend `in_event` routing into visual/image generation,
  Feishu writeback, QiWe send, or publishing. `post_event` may use the reviewed internal
  starter path from approved recap brief to image-generation request and then approved
  generated image to awaiting-publish group-message request; those starters must not
  call providers, write Feishu, confirm, queue, publish, call QiWe, or send.
- `run-xiaoman-activity-send-request-starter-worker` may only create an
  `awaiting_publish` AgentOS `erhua.send_group_message` / `group_message_request` child
  from an approved Xiaoman `generated_image` whose image-generation request is
  completed. It must not record final confirmation, queue the group message, run
  send-ready, publish, call QiWe, write Feishu, or call external adapters.
- `run-xiaoman-activity-image-generation-starter-worker` may only create an
  `image_generation_request` from an approved Xiaoman `poster_brief`; it must not call
  an image provider, upload media, write Feishu, send QiWe, or publish.
- `qintopia-agentos-xiaoman-activity-image-generation-starter-worker.timer` may only run
  `run-xiaoman-activity-image-generation-starter-worker --once --apply` for AgentOS
  image-generation request intake. Do not repurpose it for provider calls, media upload,
  generated-image creation, Feishu writeback, QiWe sends, or publishing.
- QiWe asynchronous `cmd=20000` callback events must be sanitized before NATS capture
  publication and independently before the sidecar writes Postgres. Persist only hashed
  correlation and fixed field-presence metadata; never publish or persist callback file
  credentials, media URLs, filenames, identities, message content, unknown values, or an
  unredacted callback event id. Invalid/dead-letter payloads must store only a digest
  and byte count, never the raw payload. A callback id is already sanitized only when it
  is exactly `qiwe-callback:` plus a 64-character hexadecimal SHA-256 digest; a prefix
  alone is untrusted and the complete value must be hashed again.
- QiWe callback credential-shape reports may emit only a fixed reviewed schema id and an
  additional-field count. They must reject simultaneous canonical and alias spellings
  and must never emit request ids, credential values, filenames, MD5 values, unknown
  field names, or unknown values.
- `qintopia_agent_os.qiwe_image_send_attempts` may store only canonical hashes, AgentOS
  UUIDs, claim state, allowlisted failure codes, and sanitized audit metadata. Never
  persist QiWe callback file credentials or raw request/callback/message ids. Commit
  `sending` before calling `/msg/sendImage`; an uncertain result becomes `ambiguous` and
  must record `external_send_executed=null` with outcome `unknown` and must not be
  retried automatically. Non-2xx and non-success business responses after the request
  may have been sent are also ambiguous unless a reviewed failure-code allowlist proves
  no send occurred. Treat QiWe target group ids as opaque, case-sensitive values;
  allowlists must use exact matching. A callback arriving after the upload claim TTL
  must terminalize that attempt as `expired` and release the work item for a new
  correlation. Claim scans must also expire an `awaiting_callback` attempt whose
  callback never arrived; an active attempt must not remain solely because no callback
  invoked the callback handler. Once `sending` is committed, the same attempt and claim
  token may record `sent`, `failed`, or `ambiguous` after the short TTL; HTTP failures
  or provider non-success after the send gate are ambiguous unless the bounded client
  proves the request was not sent. Wall-clock expiry must not leave an external outcome
  stuck in `sending`.
- The QiWe upload claim transaction must persist an `uploading` attempt before external
  I/O. A stale `uploading` attempt or legacy unrecorded claim has an unknown external
  outcome and must become terminal `ambiguous` with `automatic_retry_allowed=false`;
  never requeue it automatically. Dry-run and disabled previews must enforce the same
  exact target-group and media-host allowlists as apply.
- QiWe image-send production activation is guarded, not automatic. Activation requires
  the persistent sidecar env file to contain exactly one
  `QINTOPIA_QIWE_IMAGE_SEND_ENABLED=1`, exactly one
  `QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_APPROVAL=approved-production-qiwe-image-send`,
  and exactly one canonical `QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256`
  before starting `qintopia-agentos-qiwe-image-send-preflight.service`. That service
  must run release-local `qiwe-image-send-production-preflight`, and activation must
  stop before `enable --now` unless the production apply gate validates the actual
  database URL hash, Feishu delivery config, and immutable `release/current`
  `qiwe-production` artifact identity. If the post-enable enabled-state observation
  fails, activation must disable the timer and stop/reset the worker service before
  returning failure. Rollback must stop the timer first and may report completion only
  after the persistent enablement flag is present exactly once and set to `0`. Do not
  install or enable QiWe production apply units by hot-editing systemd outside the
  reviewed release runner and activation scripts. If a test or accidental build includes
  both `qiwe-production-adapter` and `qiwe-staging-adapter`, apply commands must still
  select the production gate and must never fall back to staging approval or staging
  database hashing. Production observation may inspect only the immutable
  `release/current/sidecar-profiles/qiwe-production` companion and fixed production env
  file to prove the QiWe send path remains disabled. It must fail if the QiWe image-send
  flag is enabled, and it must not pass database/QiWe secrets to observation children,
  bypass the async callback/send state machine, write Feishu as part of sending, or
  treat staging evidence as production completion.
- In a separately owner-approved staging-feature build, `run-qiwe-image-send-worker` may
  only claim one reviewed send-ready work item, call the reviewed asynchronous
  URL-upload method, and persist hashed upload correlation. Its dry-run preview must
  reuse the apply path's exact target-group, media-host, and approved JPEG identity
  validation; preview must not report policy-ineligible work. Staging-feature apply must
  additionally require the exact reviewed one-shot owner approval phrase before adapter
  configuration, callback stdin, Postgres, or network access; feature compilation,
  enable flags, or credentials alone are insufficient.
  `process-qiwe-image-send-callback` must read one bounded callback from stdin and keep
  file credentials memory-only. It must always match callback MD5 and byte size to the
  approved final JPEG; a callback filename is optional, but when present it must also
  match. The filename sent to QiWe must come from the transaction-locked approved
  artifact before committing `sending`, not from a fallback or unknown callback field.
  Commit that state before one send call and terminalize every outcome. Scheduling or
  production enablement must remain bound to approved staging evidence, isolated group
  allowlists, release/database hash gates, and rollback.
- A staging-feature QiWe callback apply must validate explicit enablement, exact
  API/media/group allowlists, and webhook readiness before reading stdin. Upload apply
  must validate the same adapter configuration before connecting to Postgres.
- `qiwe-image-send-staging-readiness-smoke.sh` is the read-only gate before the real
  QiWe staging preflight. It may only check metadata for the fixed staging env file,
  fixed immutable staging release root, owner-approved release SHA, and packaged sidecar
  binary digest. Its sidecar release directory and binary permission checks must match
  the staging smoke's owner/group/world-writable rejection, while the secret-bearing env
  file may remain owner-writable. Path checks must lstat every parent component and
  reject symlinks, non-directories, group/world-writable parents, unexpected parent
  owners, and a sidecar binary the running user cannot execute; tests for these checks
  must use repository-local temporary roots, not `/tmp`. It must not read env file
  contents, execute the sidecar, connect to Postgres, contact QiWe/Feishu/provider/media
  endpoints, create release directories, install or enable services/timers, or report
  secret-bearing values.
- A QiWe webhook bridge for `cmd=20000` may invoke only one explicitly configured
  staging sidecar with fixed `process-qiwe-image-send-callback --apply` arguments. It
  must default disabled, require the exact staging owner phrase and canonical approved
  staging database URL hash before callback stdin, stream the bounded raw callback only
  through child stdin, discard child stderr, bound and validate the sanitized Rust
  report, and never persist or log callback bytes, credentials, request ids, filenames,
  MD5 values, unknown fields, or subprocess output. Ordinary callback capture must still
  be sanitized independently before NATS publication. If the bridge is explicitly
  enabled but any gate is invalid, callback handling must return a non-2xx response; it
  must not silently downgrade to disabled and acknowledge an unprocessed callback.
  Callback detection must require the reviewed top-level QiWe success envelope, bounded
  `data` list, request id, `msgData` object, and the complete core credential fields
  `fileAesKey`, `fileId`, `fileMd5`, and `fileSize`. `filename`/`fileName` is optional
  because the approved upload claim already locks the send filename. It must fail closed
  on excessive JSON depth and must not classify an arbitrary nested `cmd=20000` field as
  an image callback. The child process may inherit only the fixed staging callback
  allowlist: sidecar database URL and pool size, approved database hash,
  send/webhook/owner gates, QiWe API URL/token/guid, API/media host allowlists, and
  target-group allowlist. It must not inherit Hermes webhook secrets, NATS, Feishu,
  proxy, or unrelated runtime variables. Any explicit callback-processor enable value
  other than `0` or `1` is configuration invalid and must also return a non-2xx callback
  response. The processor path must be exactly
  `/home/ubuntu/qintopia-agent-os-staging-releases/<40-hex-sha>/sidecar/qintopia-message-sidecar`
  with the matching configured release root and approved binary SHA-256. The fixed
  staging root, release directory, sidecar directory, and binary must not be symlinks,
  owned by an unexpected uid, or group/world-writable. Revalidate the path and digest
  immediately before spawn; never accept a writable staging-like path such as `/tmp`.

#### Sidecar Rules

- Do not place NATS credentials in `QINTOPIA_SIDECAR_NATS_URL`. Use the fixed private
  consumer auth file and keep it distinct from the QiWe producer auth file. Before
  production Space automation activation, the release-local preflight must prove
  anonymous denial, producer-only authenticated publication, consumer-only durable
  consumption/ack permissions, and the expected stream/consumer filters. This proves ACL
  configuration, not end-to-end JetStream delivery; retain one real authenticated shadow
  callback as consumption evidence before event automation activation.
- Sanitize QiWe asynchronous `cmd=20000` callback credentials before raw-event
  persistence. Dead letters may keep only payload length and digest; malformed payloads
  must not become a bypass that stores callback credentials or raw private text. Only
  preserve callback event/message ids matching `qiwe-callback:<64 hex SHA-256>`; hash
  the complete id again when a prefixed value has any other suffix.
- QiWe image-send state transitions must lock both the work item and attempt, recheck
  the same unexpired claim plus approved artifact/target/final-confirmation facts, and
  store only canonical hashes. The `sending` transition is the at-most-once boundary;
  crashes or transport uncertainty after it require `ambiguous` human reconciliation,
  never an automatic retry with callback credentials. A non-2xx or non-success business
  response after the request may have been sent is also ambiguous without a reviewed
  no-send failure-code allowlist. Treat QiWe target group ids as opaque and
  case-sensitive, and match their allowlist exactly. An ambiguous send audit must use
  `external_send_executed=null` and outcome `unknown`, never a definite false. Late
  callbacks must atomically expire the awaiting attempt and requeue the same work item
  before returning. After the send gate commits, terminal writes must still require the
  exact attempt and claim token but must not fail only because its short TTL elapsed.
  Before selecting new work, the claim transaction must expire and requeue a stale
  `awaiting_callback` attempt even when no callback ever arrives; never apply that
  timeout retry path to `sending`.
- The QiWe upload worker and callback processor may compile live helpers only through
  `qiwe-staging-adapter` or `qiwe-production-adapter`. Default builds must fail apply
  before Postgres or network access, and callback apply must do so before reading stdin.
  If a test or accidental build includes both QiWe live features, apply must select the
  production owner/database gate and must never fall back to staging approval or staging
  database hashing. Runtime env flags are not a substitute for the compile and owner
  gates. Callback JSON is accepted from bounded stdin only, never CLI arguments or
  environment variables. File credentials may open the send gate only when canonical MD5
  and byte size exactly match the approved final JPEG identity snapshotted at upload. A
  callback filename is optional and must match when supplied; the send filename must
  always come from the transaction-locked approved artifact. Callback credentials,
  request ids, media URLs, target groups, tokens, device ids, response bodies, and
  provider message ids must not appear in reports or logs; sensitive in-memory buffers
  must be zeroized on drop.
- A staging-feature QiWe apply must require
  `QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL=approved-staging-qiwe-image-send` before
  adapter configuration, stdin, Postgres, or network access. The Cargo feature, enable
  flag, secrets, and allowlists do not substitute for this owner-reviewed one-shot gate.
- QiWe upload dry-run must use the same exact group/media allowlists and approved JPEG
  identity validator as apply. It may skip locks and writes, but not policy checks.
