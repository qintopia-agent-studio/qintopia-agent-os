# Agent: Huabaosi

`huabaosi` is 阿靓（画报司）, the visual asset Agent for internal poster briefs, visual
prompts, caption drafts, and related creative artifacts.

## Scope

- Produce internal visual drafts and creative briefs from approved, sanitized inputs.
- Work through governed capability requests such as `huabaosi.create_visual_asset`.
- Return artifacts for human review before any external use.
- For Xiaoman activity promotion, wait for the sibling `evidence_summary` before
  creating a `poster_brief`.
- After a human approves a `poster_brief`, create an auditable
  `image_generation_request`; a real image is still blocked on the dedicated provider
  and media-storage approval gate.

## Resident welcome responsibility (local integration; production not enabled)

The independent room Agent 岸岸 (`anan`) requests the welcome card
from 阿靓 (`huabaosi`), who produces it from approved, purpose-filtered inputs and
returns the versioned Artifact and controlled attachment reference. Welcome cards belong
in the application Base's welcome-card attachment, not the ordinary design-output Base.
`huabaosi` neither selects the delivery group nor sends the welcome.

The room workflow then applies each building's policy. A single-item or standing
authorization that explicitly covers the full welcome card and text permits that
target's Erhua to forward them without per-item human review; a review-required policy
waits for the responsible human's approval. Artifact availability alone authorizes
neither path. This welcome-specific contract is defined in
[welcome §6.4](../../docs/plans/active/unified-person-welcome-v1-contract.md#64-欢迎编排制卡与楼栋转发职责负责人纠偏2026-09-22).
The local executor now creates a real PNG from bounded synthetic material and returns
its versioned Artifact and an application-attachment test receipt. Its Pillow renderer
is a local layout fixture; it does not establish migration of the unavailable legacy
`generate_card_v10.py`, real-model generation or real Feishu attachment writes. Existing
activity-poster review and provider gates remain in force; production is not enabled.

## Boundaries

- Must not publish, send, or modify externally visible material without review.
- Must not use member photos, private stories, private chat, or identifiable personal
  material unless the request includes explicit approval and source evidence.
- Must not treat server-side shadow or Rust exploration as an approved production
  migration.
- Must not call an image model, upload user media, write the Feishu design ledger, or
  publish a poster from the internal `poster_brief` workflow until the dedicated adapter
  is reviewed and explicitly enabled.
- An approved staging adapter may retry only recoverable provider failures and must stop
  after three total attempts. Media upload/readback, content validation, authentication,
  persistence, and claim failures remain terminal.

## Runtime Source

- Server profile observed read-only: `/home/ubuntu/.hermes/profiles/huabaosi`
- Current service observed read-only: `hermes-gateway-huabaosi.service`
- Current package status stays draft because Huabaosi shadow/Rust work remains
  review-pool until owner approval.
- Runtime `.env`, memories, sessions, caches, auth files, locks, logs, and databases are
  excluded from this package.

The AgentOS visual-brief worker is an active internal control-plane capability, but it
does not make this Hermes profile package or a real image provider adapter
production-ready.

## Validation

```bash
pnpm smoke:sidecar
pnpm registry:check
pnpm policy:check
```

## Operating rules

Paths below are repository-relative; Sidecar subsections use `runtime/sidecar/`.

### Commands

- Huabaosi image generation staging readiness smoke:

  ```bash
  QINTOPIA_HUABAOSI_IMAGE_STAGING_READINESS_ENABLE=1 \
  QINTOPIA_HUABAOSI_IMAGE_STAGING_APPROVAL=approved-staging-image-generation \
  QINTOPIA_HUABAOSI_IMAGE_STAGING_RELEASE_SHA=<approved-staging-release-sha> \
  QINTOPIA_HUABAOSI_IMAGE_STAGING_SIDECAR_SHA256=<approved-staging-sidecar-sha256> \
  deploy/sidecar/scripts/huabaosi-image-generation-staging-readiness-smoke.sh
  ```

- Huabaosi image generation production state observation smoke:
  `QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/huabaosi-image-generation-production-observation-smoke.sh`
- Huabaosi image generation one-shot production canary:

  ```bash
  QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_ENABLE=1 \
  QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_APPROVAL=approved-production-image-generation-canary \
  QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_BRIEF_ARTIFACT_ID=<pending-poster-brief-uuid> \
  QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_DATABASE_URL_SHA256=<approved-database-url-sha256> \
  QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_RELEASE_SHA=<approved-release-sha> \
  QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_SIDECAR_SHA256=<approved-sidecar-sha256> \
    deploy/sidecar/scripts/huabaosi-image-generation-production-canary-smoke.sh
  ```

- Huabaosi image generation production canary evidence validation:
  `node tools/deploy/check-huabaosi-image-production-canary-evidence.mjs <production-canary-output.txt>`
- Huabaosi Feishu-backed generated-image read-only revalidation:
  `qintopia-message-sidecar huabaosi-feishu-primary-storage-revalidate --artifact-id <generated-image-uuid>`
- Huabaosi generated-image Feishu mirror production observation smoke:
  `QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/huabaosi-feishu-artifact-mirror-production-observation-smoke.sh`
- Huabaosi generated-image Feishu mirror activation is guarded, not automatic. It
  requires the persistent mirror flag to be present exactly once and set to `1`, then
  runs the release-local preflight service through the fixed `/usr/bin/systemctl`
  boundary before enabling the dedicated timer:
  `QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_ACTIVATION=approved-production-huabaosi-feishu-artifact-mirror deploy/sidecar/scripts/activate-huabaosi-feishu-artifact-mirror-production.sh`
- Huabaosi generated-image Feishu mirror immediate timer rollback:
  `QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_ROLLBACK=approved-production-huabaosi-feishu-artifact-mirror-rollback deploy/sidecar/scripts/rollback-huabaosi-feishu-artifact-mirror-production.sh`
- Huabaosi image generation production activation after manual Release publish. The
  activation and rollback scripts must use a fixed minimal `PATH` and
  `/usr/bin/systemctl`, never a caller-provided `SYSTEMCTL`:
  `QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_ACTIVATION=approved-production-image-generation deploy/sidecar/scripts/activate-huabaosi-image-generation-production.sh`
- Huabaosi image generation immediate timer rollback:
  `QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_ROLLBACK=approved-production-image-generation-rollback deploy/sidecar/scripts/rollback-huabaosi-image-generation-production.sh`
- QiWe image-send production activation after manual Release publish and production env
  approval. The activation script must fail before preflight or timer changes unless the
  persistent `QINTOPIA_SIDECAR_DATABASE_URL` hashes to the approved
  `QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256`; never weaken this to a
  format-only hash check. It must also prove the fixed
  `release/current/sidecar-profiles/qiwe-production` companion is the separately
  reviewed `qiwe-production` artifact with exactly `qiwe-production-adapter` and
  `huabaosi-feishu-mirror-adapter` before preflight or timer enablement. After preflight
  and timer enablement, activation must rerun the release-local QiWe production
  observation with `EXPECTED_STATE=enabled` so the immutable `release/current` artifact
  profile and timer state are re-verified at the reviewed boundary; if that final
  observation fails, activation must immediately disable the timer, stop/reset the
  worker service, and exit non-zero. Activation and rollback must read the fixed
  reviewed `/etc/qintopia/message-sidecar.env` and must not accept env-file or systemctl
  command overrides from the caller; use a fixed system PATH and absolute systemctl
  path:
  `QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_ACTIVATION=approved-production-qiwe-image-send deploy/sidecar/scripts/activate-qiwe-image-send-production.sh`
- Release-local QiWe production observations must inspect only the fixed
  `sidecar-profiles/qiwe-production` companion and require exactly
  `qiwe-production-adapter` plus `huabaosi-feishu-mirror-adapter` in both disabled and
  enabled states. They must reject the primary Huabaosi binary. Never restore QiWe by
  mixing it into or replacing the Huabaosi artifact.
- Huabaosi WeCom gateway read-only observation smoke:
  `QINTOPIA_HUABAOSI_WECOM_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/huabaosi-wecom-gateway-observation-smoke.sh`
  Run it as the `ubuntu` Hermes systemd user, not through `sudo`, because it inspects
  `systemctl --user`. Its journal scan is fixed to the latest 30 minutes and 160 lines;
  production commands, paths, and the journal window must not accept caller-controlled
  overrides. Keep test doubles in the Node fixture only.
- Huabaosi WeCom canary disabled-state observation smoke:
  `QINTOPIA_HUABAOSI_WECOM_CANARY_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/huabaosi-wecom-canary-observation-smoke.sh`
- Huabaosi WeCom shadow capture fixture replay:
  `cargo test --manifest-path runtime/sidecar/Cargo.toml huabaosi_wecom_shadow`
- Huabaosi WeCom policy preview fixture replay:
  `cargo test --manifest-path runtime/sidecar/Cargo.toml huabaosi_wecom_policy`
- Huabaosi WeCom canary gateway fixture replay:
  `cargo test --manifest-path runtime/sidecar/Cargo.toml huabaosi_wecom_canary`

### Core Rules

- For the Huabaosi production image canary, the owner-selected first storage boundary is
  the fixed Feishu Base `huabaosi-generated-image-v1` table. The image worker may upload
  the exact final JPEG attachment and idempotently upsert one row by
  `generated_image_artifact_id`; it must read the uploaded bytes back through the
  authenticated Feishu media API and verify the complete JPEG identity before creating a
  pending AgentOS artifact. Do not require a separate media upload/public URL service
  for this Feishu-backed canary. The production image storage backend must be
  `QINTOPIA_HUABAOSI_IMAGE_STORAGE_BACKEND=feishu-base`; the generated-image table id
  comes from the owner-provided Feishu URL's `table` query parameter and must not be
  committed to git. `QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED=1` is required by the
  current production binary for Feishu-backed storage validation, but it does not enable
  the mirror worker timer by itself. Feishu automation may notify reviewers or mirror
  reviewed status after the row exists, but it must not generate images, approve
  artifacts, become the fact source, call QiWe, or publish.
- Huabaosi generated-image Feishu mirroring must use the fixed
  `huabaosi-generated-image-v1` artifact-version schema and key idempotency by
  `generated_image_artifact_id`. It may mirror only a fully revalidated immutable final
  JPEG and sanitized review metadata. It must not update the legacy poster task summary
  without a stable AgentOS workflow id, treat Feishu state as approval, call QiWe, or
  publish. Production artifacts compile exactly `huabaosi-production-adapter`, the
  guarded `huabaosi-feishu-mirror-adapter`, and the default-disabled
  `xiaoman-feishu-poster-adapter`; staging, QiWe, and all-features production artifacts
  remain forbidden. The ordinary release installer may install the dedicated mirror
  preflight, worker, and timer units, but must not enable the external write timer
  automatically.
- Huabaosi Feishu mirror apply must validate the exact owner phrase, production release
  SHA binding, database URL hash, Base and table exact allowlists, fixed schema version,
  Huabaosi profile path, and media host allowlist before Postgres or external I/O. The
  production observation may run only the non-secret mirror observation preflight; it
  must not run full configuration preflight, preview the queue, upload media, write
  Feishu/Postgres, approve, publish, call QiWe, or send.
- Huabaosi Feishu primary-storage apply must reuse the bounded Rust Feishu client and
  the same exact Base/table allowlists, schema version, profile path, production release
  SHA, and database URL hash gates as the reviewed mirror. Feishu attachment tokens and
  credentials are memory-only and must not appear in Postgres metadata, reports, logs,
  CLI arguments, or environment-derived output. A failed or ambiguous Feishu write must
  not create a pending artifact or be retried automatically as if no external write
  occurred.
- Huabaosi image-generation worker reports intentionally omit the generated artifact URI
  even for Feishu-backed storage. Production canary evidence must not depend on a worker
  stdout `artifact_uri`; use the reviewed Feishu primary-storage revalidation step to
  prove the stored JPEG identity.
- A Feishu-backed image canary may cross from `pending` to `approved` only through an
  explicit human apply that first completes authenticated Feishu attachment revalidation
  and then matches the memory-only evidence against the transaction-locked Postgres
  artifact identity. Rejection and changes-requested decisions must not require Feishu
  I/O. Feishu fields, automation, and workbench events alone must never approve. QiWe
  intake must continue to fail closed for `feishu-base://` until a separate reviewed
  delivery path exists. The `huabaosi-feishu-primary-storage-revalidate` sidecar command
  is read-only evidence: it may authenticate to Feishu, reload the fixed record by
  generated-image artifact id, download the `最终JPEG` attachment, and emit a sanitized
  report, but it must not approve, write Postgres or Feishu, call QiWe, publish, send,
  or expose attachment tokens/record ids/Base/table ids/credentials. The current QiWe
  async upload contract requires a stable allowlisted HTTPS `fileUrl`; do not bridge
  Feishu private attachments by exposing Feishu attachment tokens, storing a private
  media URL, introducing an unreviewed public proxy/upload service, or falling back to
  QiWe synchronous upload APIs marked deprecated in the reviewed protocol plan.
- Huabaosi Feishu production observation must discover the immutable
  `release/current/sidecar/qintopia-message-sidecar` binary, or accept an explicit
  `QINTOPIA_SIDECAR_BIN` only when it resolves to that same release-local binary with
  the approved production features; it must fail closed instead of falling back to
  `cargo run` or a mutable source tree. Its shell may parse only the mirror enable flag;
  the child launcher may pass only that parsed flag and the non-secret release SHA to
  the immutable binary, without `source`, `eval`, command substitution, shell secret
  import, or a secret-bearing temporary file. It must not pass database URL, Base token,
  table id, Feishu token, profile env path, or allowlist values to the child process.
  Ignore non-allowlisted env values before applying mirror-flag value validation.
  Activation must fail before preflight or timer changes unless the persistent mirror
  enable flag is present exactly once and exactly `1`. Rollback must stop the timer
  first and may report completion only after that flag is present exactly once and
  exactly `0` in the reviewed sidecar environment file.
- Huabaosi image-generation production systemd services must bind
  `QINTOPIA_DEPLOYED_COMMIT_SHA`, `QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_RELEASE_SHA`, and
  `QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA` to the immutable release SHA when
  units are rendered. Feishu-mirror-only services and QiWe production services that use
  the Feishu primary-storage delivery bridge bind the deployed and Feishu release
  variables, but must not inherit the image adapter release variable. The read-only
  image production observation must derive all three release-bound values from the
  verified `release/current` target and pass the persistent image approval, database
  hash, timeout, and media bound needed by the real production preflight. Do not repair
  release binding by editing `/etc/qintopia/message-sidecar.env` during a deployment.
- Default sidecar builds must fail QiWe upload/callback apply before configuration,
  Postgres claim/mutation, or network access even if runtime enable flags are
  misconfigured; callback apply must also fail before reading stdin. Production artifact
  manifests must record exactly
  `cargo_features: [huabaosi-production-adapter, huabaosi-feishu-mirror-adapter, xiaoman-feishu-poster-adapter]`;
  artifact and server-source build checks must reject `qiwe-staging-adapter`,
  `huabaosi-staging-adapter`, and all-features builds. The Huabaosi production feature
  alone must not make QiWe live helpers available.
- The Hermes QiWe image callback bridge is a memory-only callback ingress, not a
  scheduler or release activation path. Production mode must require
  `QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_MODE=production`, exact production owner
  approval, canonical production database URL hash, image-send/webhook readiness, and
  the exact
  `/home/ubuntu/qintopia-agent-os-releases/current/sidecar-profiles/qiwe-production/qintopia-message-sidecar`
  binary with root `/home/ubuntu/qintopia-agent-os-releases/current`. Production must
  derive that path and SHA-256 from the companion manifest and `SHA256SUMS`, not trust
  runtime-local `BIN`, `ROOT`, or `SHA256` values. It must reject direct
  release-directory paths, mutable checkout binaries, staging roots, missing `current`
  symlinks, unsafe ownership or group/world-writable paths, and sidecar SHA-256 drift.
  Staging mode must continue to use only the fixed staging release root and staging
  owner/database gates. The child process may receive only the reviewed database, QiWe,
  target allowlist, and Huabaosi Feishu primary-storage delivery environment for its
  selected mode; do not inherit Hermes, NATS, proxy, unrelated runtime state, callback
  credentials, or raw provider values. Callback bytes may flow only through bounded
  stdin, and bridge enablement must never approve artifacts, enable timers, publish a
  Release, write Feishu by itself, or bypass the Rust production apply gate.
- `xiaoman-real-activity-production-evidence` is a read-only retention exporter. It may
  run only from the immutable
  `/home/ubuntu/qintopia-agent-os-releases/current/sidecar-profiles/qiwe-production/qintopia-message-sidecar`
  binary whose resolved release directory matches `QINTOPIA_DEPLOYED_COMMIT_SHA` and
  whose SHA-256 matches `QINTOPIA_XIAOMAN_REAL_ACTIVITY_PRODUCTION_SIDECAR_SHA256`. It
  must also prove the adjacent reviewed artifact manifest is bound to the same release
  SHA and exactly `runtime_artifact_profile=qiwe-production` with
  `qiwe-production-adapter` and `huabaosi-feishu-mirror-adapter`. It must hash the
  configured database URL and match
  `QINTOPIA_XIAOMAN_REAL_ACTIVITY_PRODUCTION_DATABASE_URL_SHA256` before opening a
  database connection. It may read Postgres, hash the verified release-local sidecar
  binary, and emit the fixed `xiaoman_real_activity_production_evidence=` records for
  one already completed Xiaoman activity chain. It must not run from a mutable checkout,
  connect to a database whose URL hash is not owner-approved, write Postgres or Feishu,
  approve artifacts, call QiWe, publish, send, expose raw group ids, request ids,
  callback bodies, file credentials, message ids, media URLs, database URLs, provider
  responses, raw chat, or logs. Its send-ready query must bind the completed
  `group_message_request`, queued-state final confirmation, send-ready event payload,
  approved generated-image artifact id, and sanitized QiWe `sent` attempt before
  emitting production-complete evidence. The final completion manifest must bind the
  Huabaosi first-record canary to `runtime_artifact_profile=huabaosi-production` and the
  retained real-activity/QiWe arrival evidence to
  `runtime_artifact_profile=qiwe-production`.
- The Feishu-backed QiWe staging bridge may claim `feishu-base://` generated images only
  when the immutable staging artifact contains both `huabaosi-staging-adapter` and
  `qiwe-staging-adapter`. It must commit the existing `uploading` attempt before Feishu
  or QiWe I/O, authenticated-readback the approved JPEG, upload those bytes only to the
  non-deprecated QiWe SDK temporary-storage endpoint, keep the returned `cloudUrl`
  memory-only, exact-allowlist and read back that URL, and prove SHA-256, MD5, and byte
  size before invoking the existing asynchronous URL upload. Default, production,
  Huabaosi-only, and QiWe-only builds must fail closed. Temporary URLs, Feishu tokens,
  multipart bodies, and raw bytes must not enter Postgres, reports, logs, CLI arguments,
  or environment-derived output.
- `qiwe-image-send-staging-smoke.sh` is the only reviewed one-shot staging entrypoint
  for the async upload and callback send exercise. It requires an exact work item UUID
  for upload/callback, owner phrase, staging env path, exact owner-reviewed staging
  database URL hash, exact owner-reviewed staging release SHA, exact owner-reviewed
  packaged sidecar binary SHA-256, and explicit `preflight`, `upload`, or `callback`
  phase. It must fail closed unless it is running from
  `/home/ubuntu/qintopia-agent-os-staging-releases/<approved 40-hex sha>` and executing
  that release-local `sidecar/qintopia-message-sidecar`; do not allow
  `QINTOPIA_SIDECAR_BIN`, `cargo run`, symlinked binaries, untrusted owners,
  owner/group/world-writable binary paths, mutable source-tree fallbacks, or a binary
  whose SHA-256 changes between the initial check and any child sidecar spawn. Callback
  credentials may flow only from bounded stdin to the callback processor and memory-only
  send request; never store them in a file, environment variable, CLI argument, NATS
  event, report, or log. The smoke must attach `/dev/null` to preflight and upload
  subprocesses, parse only the fixed staging env key allowlist without evaluating the
  env file as shell, revalidate the sidecar path and digest immediately before each
  child sidecar command, and run child sidecar commands with a minimal explicit
  environment rather than inheriting ambient operator secrets. Subprocess output must be
  captured, scanned, and schema-validated through memory and anonymous pipes, and raw
  child JSON must not be passed to a sanitizer through environment variables; no
  subprocess output may be written to a file. The local fake smoke test creates a
  temporary `sidecar/qintopia-message-sidecar`; if interrupted, remove any leftover
  source-tree `sidecar/` before treating the checkout as clean or rerunning staging
  checks. Upload and callback evidence may retain only the canonical final JPEG
  `artifact_content_hash` for Huabaosi/QiWe hash matching; it must not retain media URI,
  filename, MD5 value, file size, or callback credentials. It must not install a
  listener, service, timer, production feature build, Feishu write, or broad group send.
- A QiWe production-enablement PR must retain Huabaosi staging generated-image evidence
  and QiWe staging send evidence that pass
  `tools/deploy/check-xiaoman-image-send-staging-evidence.mjs`, proving the Huabaosi
  final JPEG `content_hash` equals the QiWe `artifact_content_hash` without recording
  media URI, filename, MD5 value, callback credentials, group id, database URL, or raw
  provider output. Record that cross-flow result only in
  `docs/reports/templates/xiaoman-image-send-staging-evidence.md`; staging evidence is a
  prerequisite, not proof that production sending is complete.
- The staging-only sidecar artifact `qintopia-message-sidecar-staging-linux-x86_64-gnu`
  may be built only by manual artifact workflow dispatch or
  `pnpm artifact:sidecar:staging`. It must compile exactly `huabaosi-staging-adapter`
  and `qiwe-staging-adapter`, record `staging_only=true` and
  `production_eligible=false`, and be installed only under
  `/home/ubuntu/qintopia-agent-os-staging-releases/<approved 40-hex sha>` for
  owner-approved evidence smokes. Because staging image storage is fixed to
  `feishu-base`, the staging Huabaosi adapter must compile the guarded Feishu Base
  primary-storage upload/readback path while preserving the staging owner approval,
  database hash, Base/table allowlists, schema, and Huabaosi profile gates before
  external I/O. Production deploy, COS upload, Release builds, and production artifact
  fetchers must never fetch or promote it.
- `run-huabaosi-image-generation-worker` defaults to
  `QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED=0`. Production generation may run only
  from a release artifact compiled with the reviewed `huabaosi-production-adapter`
  feature, explicit production enablement bound to the deployed release SHA and database
  URL hash, valid provider/media configuration, and the fixed production timer. It may
  create only pending `generated_image` artifacts; it must not approve, publish, write
  Feishu, or send QiWe.
- The Huabaosi production image-generation service and timer may be installed from the
  immutable release but must not be enabled by the ordinary release installer. After the
  owner manually publishes the Release, the reviewed activation command must run the
  no-network preflight from that release and then enable the fixed timer for canary
  generation. Rollback disables the timer first and turns the generation enable flag off
  through reviewed runtime configuration. Do not repurpose this timer for artifact
  approval, Feishu, QiWe, or publishing.
- `operations-artifact-review-decision` may approve a `generated_image` only after its
  Huabaosi worker provenance, stable JPEG HTTPS URI, final JPEG sha256/metadata, source
  PNG sha256, fixed `png_to_jpeg_white_background_q92_v1` transform metadata, source
  brief/prompt refs, and `generated_image_created` audit match its image-generation
  request. Human approval applies to the exact final JPEG bytes; the transient provider
  PNG is never an approvable artifact. Integrity denial must leave the artifact pending
  and must not complete the work item or unlock downstream send intake.
- Generated-image media URIs used by Huabaosi artifact creation, operations approval,
  and QiWe send intake must reject raw backslashes and percent-encoded path separators
  before URL parsing; parsers or downstream services may normalize them into path
  separators, which can hide unstable or secret-shaped input from later filename checks.
- When the Huabaosi adapter is explicitly enabled in an approved staging boundary, it
  may retry only provider transport failures and HTTP 408, 429, or 5xx responses. It
  must stop after three total attempts, use delayed requeueing, and record only
  sanitized attempt/stage/outcome metadata. Authentication, payload, PNG decode,
  PNG-to-JPEG conversion, JPEG validation, media upload, readback, persistence, and
  claim failures are terminal and must not be retried. When explicitly enabled in a
  reviewed staging configuration, every provider, upload, and readback response must be
  size-capped before parsing, and an already reviewed `generated_image` must never be
  overwritten or returned to `pending` by a retry. Every outbound HTTP header name/value
  must reject control characters before socket connection. Each work-item claim must use
  a unique token; artifact or failure writes must lock and match that unexpired token,
  with exactly one affected work-item row.
- An expired or structurally incomplete Huabaosi image-generation `processing` claim is
  an unknown provider/media outcome. Reconciliation must atomically mark it failed,
  release the complete claim tuple, append one sanitized ambiguous-outcome event, and
  disable automatic retry; it must never reclaim the row for another external attempt.
- `huabaosi-image-generation-production-observation-smoke.sh` may verify either the
  disabled pre-activation state or the enabled production timer state, run configuration
  preflight, and run `run-huabaosi-image-generation-worker --once --dry-run` for a
  read-only queue preview. It must discover the immutable
  `/home/ubuntu/qintopia-agent-os-releases/current/sidecar/qintopia-message-sidecar`
  binary, or accept an explicit `QINTOPIA_SIDECAR_BIN` only when it resolves to that
  same release-local binary with exactly `huabaosi-production-adapter`,
  `huabaosi-feishu-mirror-adapter`, and `xiaoman-feishu-poster-adapter`, not QiWe
  production features. It must parse only allowlisted production env keys, launch child
  sidecar commands with a minimal explicit environment, and fail closed instead of
  accepting test-mode/path override env vars, `source`-ing env files, using `cargo run`,
  or falling back to a mutable source tree. It must not use `--apply`, contact
  provider/media endpoints, write Postgres or Feishu, call QiWe, create a generated
  image, or publish.
- `huabaosi-image-generation-production-canary-smoke.sh` is the release-local one-shot
  entrypoint for the first post-deploy image. It must run from the exact immutable
  release with the provider timer disabled and inactive, use a fixed minimal `PATH` and
  `/usr/bin/systemctl` in production mode, reject test mode from production release
  roots, parse only allowlisted keys from the fixed production env without `source` or
  `eval`, use an existing operations reviewer while emitting only the fixed
  `allowlisted-production-reviewer` evidence alias, and bind one pending brief to one
  new request, one pending Feishu-backed JPEG, and authenticated same-byte revalidation.
  Its approval request must transactionally require the target artifact to be a pending
  `poster_brief` before mutation or Feishu revalidation. Each retained canary evidence
  phase must preserve `release_binary_verified=true`,
  `approved_sidecar_sha256_matched=true`, and
  `approved_database_url_sha256_matched=true` so the standalone and final completion
  checkers can prove the immutable release/database boundary. The retained preflight
  phase must also preserve `timer_enabled=false` and `timer_active=false`, and the
  retained revalidation phase must preserve `sensitive_fields_redacted=true` so the
  evidence itself proves the disabled one-shot timer boundary and authenticated
  same-byte readback stayed sanitized. It must not approve the generated image, enable
  timers, run the mirror worker, publish, call QiWe, send, or retry terminal/ambiguous
  outcomes.
- `huabaosi-image-generation-preflight` may only validate and emit a sanitized summary
  of local image-adapter configuration. It must not open network or database
  connections, reveal configuration values, enable generation, write Feishu, send QiWe,
  or publish. Its `missing_configuration` field may contain only fixed public env names
  already documented in `.env.example`; it must never contain values, URLs, hosts, ids,
  or enable flags.
- `huabaosi-image-generation-staging-readiness-smoke.sh` may only inspect staging env
  file metadata, immutable staging release root metadata, the exact owner-reviewed
  release SHA, and packaged sidecar binary SHA-256. It must not read env file contents,
  execute the sidecar, run Cargo, connect to Postgres, call Huabaosi/provider/media,
  write Feishu, send QiWe, inspect services, install timers, or reveal paths containing
  secrets. Path checks must lstat every parent component and reject symlinks,
  non-directories, group/world-writable parents, unexpected parent owners, and a sidecar
  binary the running user cannot execute; tests for these checks must use
  repository-local temporary roots, not `/tmp`. It is a read-only prerequisite before
  the owner-approved Huabaosi staging generation smoke.
- `huabaosi-image-generation-staging-smoke.sh` may only run one owner-approved staging
  image request after the fail-closed preflight, explicit smoke flag and approval
  phrase, staging-only env file, a repository-reviewed database URL hash allowlist, and
  an explicit UUID work item id. It must parse the staging env file through a fixed key
  allowlist without evaluating it as shell. Known unrelated QiWe staging keys may exist
  in the same env file, but the Huabaosi smoke must ignore them and never pass them to
  child sidecar/cargo commands. Child commands must run with a minimal explicit
  environment rather than inheriting ambient operator secrets, keep subprocess output in
  memory for sensitive-output scanning, emit only sanitized
  `huabaosi_image_generation_staging_evidence` records for preflight and the pending
  final JPEG, and pass raw child JSON to evidence sanitizers only through
  stdin/anonymous pipes. It must leave the image pending review and must not run in
  production, add a timer, write Feishu, send QiWe, or publish. Evidence may include the
  staging database URL hash, work item UUID, final JPEG SHA-256, dimensions, byte count,
  MIME type, and pending review state; it must not include provider/media URLs,
  filenames, tokens, database URLs, provider responses, Feishu ids, or QiWe credentials.
  Record the retained result in
  `docs/reports/templates/huabaosi-image-generation-staging-evidence.md` before it is
  used as input to QiWe staging evidence.
- `render-staging-runtime-env.py` must render the same reviewed Huabaosi Feishu Base
  staging key contract consumed by `huabaosi-image-generation-staging-smoke.sh`. It must
  not require or emit the retired HTTP media upload/public URL keys for the Huabaosi
  staging path, and the generation evidence must prove the worker returned a
  `feishu-base://` artifact boundary rather than trusting the env-selected storage
  backend.
- `qiwe-image-send-preflight` may only validate the disabled async URL-upload/send-image
  contract from local configuration. It must report whether a live adapter was compiled
  and fail disabled/default release checks when any QiWe live adapter is present. It
  must not open network or database connections, emit tokens, device/group ids, media
  URLs, file credentials, or message identifiers, write Feishu, or send externally. The
  production artifact may compile only the reviewed `qiwe-production-adapter` and
  `huabaosi-feishu-mirror-adapter` pair; it must still fail closed unless the dedicated
  `qiwe-image-send-production-preflight` validates the exact owner approval phrase,
  actual database URL hash, Feishu delivery config, live feature bridge, enablement
  flag, and allowlisted configuration before timer activation. Final request
  construction must recheck the target group allowlist, response parsing must fail
  closed unless both `code=0` and `isSendSuccess=1`, and this disabled-state preflight
  must fail when the send-enable flag is `1`. All future outbound header values must
  reject every control character before socket connection. Its `missing_configuration`
  field follows the same public-name-only rule as the image preflight and must never
  include enable flags or configuration values.
- Huabaosi image generation may override the shared 60-second socket timeout only
  through `QINTOPIA_HUABAOSI_IMAGE_HTTP_TIMEOUT_SECONDS`, defaulting to 180 seconds and
  bounded from 60 through 240 seconds. The upper bound must leave room inside the fixed
  10-minute image claim lease for the five bounded Feishu auth/search/upload/readback/
  upsert calls, transform, and the final transaction. A provider transport or protocol
  error after request bytes may have been sent is ambiguous and must not be retried
  automatically. Image-generation failure audits may record
  `external_generation_executed=false` only before provider request execution, `true`
  only after a valid provider payload is accepted, and `null` when the provider outcome
  cannot be proved. `external_media_write_executed` must likewise remain `null` for an
  unprovable upload or Feishu write and become `true` only after confirmed storage. Do
  not change the shared timeout for QiWe, WeCom, Feishu, or other adapters to remediate
  image-provider latency.
- 阿靓生产 WeCom Bot 的 `Interrupting current task` / `Response formatting failed`
  用户可见中断提示来自 live Hermes gateway busy-ack and platform send fallback
  (`hermes-gateway-huabaosi.service`, `gateway/run.py`, `gateway/platforms/base.py`),
  not Rust sidecar image generation or QiWe image-send state. Diagnose this path through
  Huabaosi Hermes/WeCom runtime first, and do not hot-edit the server.
- `huabaosi-wecom-gateway-observation-smoke.sh` may only inspect the live Huabaosi
  Hermes WeCom user-service active state through `systemctl --user`, fixed service
  command, fixed
  `/home/ubuntu/.config/systemd/user/hermes-gateway-huabaosi.service.d/env.conf`
  drop-in, fixed required `/home/ubuntu/.hermes/profiles/huabaosi/.env`, public
  `busy_input_mode`, release/current presence, and sanitized user-journal marker counts.
  It must reject missing or additional drop-ins and optional or alternate environment
  files. It must not source `.env`, print raw journal lines, print user messages, read
  tokens, restart services, send WeCom messages, run image generation, write Postgres or
  Feishu, call QiWe/provider/media endpoints, or modify live Hermes profile state.
- `huabaosi-wecom-canary-observation-smoke.sh` may only verify that the canary gateway
  remains unscheduled and disabled, then run `huabaosi-wecom-canary-preflight` for a
  sanitized local configuration summary. From release/current it must discover the
  immutable `sidecar/qintopia-message-sidecar` binary rather than fall back to source
  Cargo execution. It must not use `--apply`, read stdin, source `.env`, print
  endpoint/token/id values, write Postgres or Feishu, call WeCom, QiWe, provider, or
  media endpoints, run image generation, publish messages, install units, or modify the
  live Hermes profile.
- `huabaosi-wecom-shadow-capture` may only preview one supplied WeCom event from bounded
  stdin and emit sanitized metadata, hashes, byte counts, field presence, and fixed
  guardrails. It must not add `--apply`, open Postgres or network connections, write
  artifacts, send WeCom/QiWe messages, call image providers, upload media, write Feishu,
  or emit raw ids, user text, media URLs, filenames, tokens, or callback file
  credentials.
- `huabaosi-wecom-policy-preview` may only preview one supplied WeCom event from bounded
  stdin and emit sanitized policy decisions for message classification, busy-session
  handling, internal-process filtering, formatting fallback, user-safe fallback copy,
  and idempotency. It must not add `--apply`, open Postgres or network connections,
  write artifacts, send WeCom/QiWe messages, call image providers, upload media, write
  Feishu, or emit raw ids, user text, media URLs, filenames, tokens, or callback file
  credentials. Suppression rules must match narrow complete internal templates; do not
  block ordinary user requests through broad words such as `plain text` or `纯文本`.
- `huabaosi-wecom-canary-preflight` must not read stdin, open network or database
  connections, source env files, reveal configuration values, write Feishu/Postgres, or
  send WeCom/QiWe messages. `huabaosi-wecom-canary-gateway --apply` is allowed only in
  an owner-reviewed staging command built with the non-default
  `huabaosi-wecom-canary-gateway` Cargo feature, explicit enable flag, approval phrase,
  HTTPS endpoint, token, and exact Bot/chat/user allowlists. Default production builds
  must fail closed before stdin, network, database, or send access. The command must not
  change the production Bot route, install timers, broaden sends beyond the allowlist,
  run image generation, upload media, or write Feishu/Postgres.

### Sidecar Rules

- Production sidecar artifacts compile exactly `huabaosi-production-adapter`, the
  guarded `huabaosi-feishu-mirror-adapter`, and the default-disabled
  `xiaoman-feishu-poster-adapter`. QiWe live features, staging adapters, mixed
  staging/production builds, and all-features production artifacts remain forbidden.
  Mirror apply must still fail before Postgres or external I/O unless the exact owner
  phrase, deployed release SHA, database hash, fixed Base/table allowlists, schema,
  profile path, media host policy, and persistent enable flag all pass. QiWe production
  apply code must still fail before Postgres, callback stdin, or network access unless
  the exact owner phrase, production database hash, Feishu delivery config, webhook
  readiness, and persistent enablement all pass, but QiWe live features must not be
  bundled into the Huabaosi production artifact. Ordinary release installation may
  install mirror and QiWe production preflights, workers, services, and timers, but only
  the explicit owner activation scripts may enable external timers. Feishu primary
  storage for the first canary is part of the Huabaosi production adapter path and still
  creates only pending AgentOS artifacts.
- Expired or incomplete Huabaosi image-generation `processing` claims must become a
  sanitized terminal ambiguous outcome before new work is selected. Never infer from a
  lost lease that provider generation or media upload stayed local, and never reclaim
  that row for automatic external retry.
- `huabaosi-wecom-shadow-capture` is a preview-only migration command. It may read one
  event from bounded stdin and emit only sanitized hashes, byte counts, field presence,
  classification, and fixed guardrails. It must not gain an apply mode, connect to
  Postgres or external services, send WeCom/QiWe messages, generate or upload media,
  write Feishu, create artifacts, or print raw ids, user text, media URLs, filenames,
  tokens, or callback credentials.
- `huabaosi-wecom-policy-preview` is a preview-only migration command. It may read one
  event from bounded stdin and emit only sanitized policy classifications, fixed
  fallback copy, and hash-based idempotency metadata. It must not gain an apply mode,
  connect to Postgres or external services, send WeCom/QiWe messages, generate or upload
  media, write Feishu, create artifacts, or print raw ids, user text, media URLs,
  filenames, tokens, or callback credentials. Internal-process suppression must use
  narrow full-template matches with negative fixture coverage for ordinary user text
  containing terms such as `plain text`.
- `huabaosi-wecom-canary-preflight` is a local configuration preflight only. It must not
  read stdin, open network or database connections, source env files, or emit
  endpoint/token/id values. `huabaosi-wecom-canary-gateway --apply` is staging-only,
  requires the non-default `huabaosi-wecom-canary-gateway` Cargo feature plus explicit
  enablement, approval phrase, HTTPS endpoint, token, and exact Bot/chat/user
  allowlists, and must remain unscheduled. Default builds must fail closed before stdin,
  network, database, or send access. It must not change production routing, run image
  generation, upload media, write Feishu/Postgres, or send outside the allowlist.

- As of 2026-07-15, 阿靓/Huabaosi real image production has not completed final
  activation. Do not treat it as live until the same reviewed release has follow-up
  deploy evidence, the Huabaosi timer is activated, and the first real pending
  `generated_image` has review evidence. The `v0.2.10` follow-up deploy and systemd
  installation evidence now exist, but the no-network preflight remains fail-closed
  because provider/media configuration is not provisioned; the timer must remain
  disabled until that gate passes.

本地欢迎的脚本测试替身位于
`fixtures/agents/huabaosi/welcome_runtime.py`，不代表真实 Hermes 接线或正式制卡能力。
