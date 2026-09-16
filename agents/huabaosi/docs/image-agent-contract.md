# Huabaosi image contract — part 1

These are retained operating constraints, not a new permission grant. Read the sections
relevant to the change before editing. Original conditions and exceptions remain
binding; summaries in AGENTS.md do not replace them.

Backtick paths in root-source rules are repository-relative; paths in Sidecar-source
rules retain their original `runtime/sidecar/` base. New navigation links are relative
to this document.

[Migration inventory](../../../docs/plans/active/agents-guidance/README.md) records the
baseline and source locations.

<a id="root-081"></a>

## Commands — root-081

<!-- preserved-rule: root-081 -->

- Huabaosi image generation staging readiness smoke:

  ```bash
  QINTOPIA_HUABAOSI_IMAGE_STAGING_READINESS_ENABLE=1 \
  QINTOPIA_HUABAOSI_IMAGE_STAGING_APPROVAL=approved-staging-image-generation \
  QINTOPIA_HUABAOSI_IMAGE_STAGING_RELEASE_SHA=<approved-staging-release-sha> \
  QINTOPIA_HUABAOSI_IMAGE_STAGING_SIDECAR_SHA256=<approved-staging-sidecar-sha256> \
  deploy/sidecar/scripts/huabaosi-image-generation-staging-readiness-smoke.sh
  ```

<!-- /preserved-rule: root-081 -->

<a id="root-082"></a>

## Commands — root-082

<!-- preserved-rule: root-082 -->

- Huabaosi image generation production state observation smoke:
  `QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/huabaosi-image-generation-production-observation-smoke.sh`

<!-- /preserved-rule: root-082 -->

<a id="root-083"></a>

## Commands — root-083

<!-- preserved-rule: root-083 -->

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

<!-- /preserved-rule: root-083 -->

<a id="root-084"></a>

## Commands — root-084

<!-- preserved-rule: root-084 -->

- Huabaosi image generation production canary evidence validation:
  `node tools/deploy/check-huabaosi-image-production-canary-evidence.mjs <production-canary-output.txt>`

<!-- /preserved-rule: root-084 -->

<a id="root-085"></a>

## Commands — root-085

<!-- preserved-rule: root-085 -->

- Huabaosi Feishu-backed generated-image read-only revalidation:
  `qintopia-message-sidecar huabaosi-feishu-primary-storage-revalidate --artifact-id <generated-image-uuid>`

<!-- /preserved-rule: root-085 -->

<a id="root-086"></a>

## Commands — root-086

<!-- preserved-rule: root-086 -->

- Huabaosi generated-image Feishu mirror production observation smoke:
  `QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/huabaosi-feishu-artifact-mirror-production-observation-smoke.sh`

<!-- /preserved-rule: root-086 -->

<a id="root-087"></a>

## Commands — root-087

<!-- preserved-rule: root-087 -->

- Huabaosi generated-image Feishu mirror activation is guarded, not automatic. It
  requires the persistent mirror flag to be present exactly once and set to `1`, then
  runs the release-local preflight service through the fixed `/usr/bin/systemctl`
  boundary before enabling the dedicated timer:
  `QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_ACTIVATION=approved-production-huabaosi-feishu-artifact-mirror deploy/sidecar/scripts/activate-huabaosi-feishu-artifact-mirror-production.sh`

<!-- /preserved-rule: root-087 -->

<a id="root-088"></a>

## Commands — root-088

<!-- preserved-rule: root-088 -->

- Huabaosi generated-image Feishu mirror immediate timer rollback:
  `QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_ROLLBACK=approved-production-huabaosi-feishu-artifact-mirror-rollback deploy/sidecar/scripts/rollback-huabaosi-feishu-artifact-mirror-production.sh`

<!-- /preserved-rule: root-088 -->

<a id="root-089"></a>

## Commands — root-089

<!-- preserved-rule: root-089 -->

- Huabaosi image generation production activation after manual Release publish. The
  activation and rollback scripts must use a fixed minimal `PATH` and
  `/usr/bin/systemctl`, never a caller-provided `SYSTEMCTL`:
  `QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_ACTIVATION=approved-production-image-generation deploy/sidecar/scripts/activate-huabaosi-image-generation-production.sh`

<!-- /preserved-rule: root-089 -->

<a id="root-090"></a>

## Commands — root-090

<!-- preserved-rule: root-090 -->

- Huabaosi image generation immediate timer rollback:
  `QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_ROLLBACK=approved-production-image-generation-rollback deploy/sidecar/scripts/rollback-huabaosi-image-generation-production.sh`

<!-- /preserved-rule: root-090 -->

<a id="root-098"></a>

## Commands — root-098

<!-- preserved-rule: root-098 -->

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

<!-- /preserved-rule: root-098 -->

<a id="root-100"></a>

## Commands — root-100

<!-- preserved-rule: root-100 -->

- Release-local QiWe production observations must inspect only the fixed
  `sidecar-profiles/qiwe-production` companion and require exactly
  `qiwe-production-adapter` plus `huabaosi-feishu-mirror-adapter` in both disabled and
  enabled states. They must reject the primary Huabaosi binary. Never restore QiWe by
  mixing it into or replacing the Huabaosi artifact.

<!-- /preserved-rule: root-100 -->

<a id="root-174"></a>

## Core Rules — root-174

<!-- preserved-rule: root-174 -->

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

<!-- /preserved-rule: root-174 -->

<a id="root-175"></a>

## Core Rules — root-175

<!-- preserved-rule: root-175 -->

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

<!-- /preserved-rule: root-175 -->

<a id="root-176"></a>

## Core Rules — root-176

<!-- preserved-rule: root-176 -->

- Huabaosi Feishu mirror apply must validate the exact owner phrase, production release
  SHA binding, database URL hash, Base and table exact allowlists, fixed schema version,
  Huabaosi profile path, and media host allowlist before Postgres or external I/O. The
  production observation may run only the non-secret mirror observation preflight; it
  must not run full configuration preflight, preview the queue, upload media, write
  Feishu/Postgres, approve, publish, call QiWe, or send.

<!-- /preserved-rule: root-176 -->

<a id="root-177"></a>

## Core Rules — root-177

<!-- preserved-rule: root-177 -->

- Huabaosi Feishu primary-storage apply must reuse the bounded Rust Feishu client and
  the same exact Base/table allowlists, schema version, profile path, production release
  SHA, and database URL hash gates as the reviewed mirror. Feishu attachment tokens and
  credentials are memory-only and must not appear in Postgres metadata, reports, logs,
  CLI arguments, or environment-derived output. A failed or ambiguous Feishu write must
  not create a pending artifact or be retried automatically as if no external write
  occurred.

<!-- /preserved-rule: root-177 -->

<a id="root-178"></a>

## Core Rules — root-178

<!-- preserved-rule: root-178 -->

- Huabaosi image-generation worker reports intentionally omit the generated artifact URI
  even for Feishu-backed storage. Production canary evidence must not depend on a worker
  stdout `artifact_uri`; use the reviewed Feishu primary-storage revalidation step to
  prove the stored JPEG identity.

<!-- /preserved-rule: root-178 -->

<a id="root-179"></a>

## Core Rules — root-179

<!-- preserved-rule: root-179 -->

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

<!-- /preserved-rule: root-179 -->

<a id="root-180"></a>

## Core Rules — root-180

<!-- preserved-rule: root-180 -->

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

<!-- /preserved-rule: root-180 -->

<a id="root-181"></a>

## Core Rules — root-181

<!-- preserved-rule: root-181 -->

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

<!-- /preserved-rule: root-181 -->

<a id="root-215"></a>

## Core Rules — root-215

<!-- preserved-rule: root-215 -->

- Default sidecar builds must fail QiWe upload/callback apply before configuration,
  Postgres claim/mutation, or network access even if runtime enable flags are
  misconfigured; callback apply must also fail before reading stdin. Production artifact
  manifests must record exactly
  `cargo_features: [huabaosi-production-adapter, huabaosi-feishu-mirror-adapter, xiaoman-feishu-poster-adapter]`;
  artifact and server-source build checks must reject `qiwe-staging-adapter`,
  `huabaosi-staging-adapter`, and all-features builds. The Huabaosi production feature
  alone must not make QiWe live helpers available.

<!-- /preserved-rule: root-215 -->

<a id="root-220"></a>

## Core Rules — root-220

<!-- preserved-rule: root-220 -->

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

<!-- /preserved-rule: root-220 -->

<a id="root-222"></a>

## Core Rules — root-222

<!-- preserved-rule: root-222 -->

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

<!-- /preserved-rule: root-222 -->

<a id="root-224"></a>

## Core Rules — root-224

<!-- preserved-rule: root-224 -->

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

<!-- /preserved-rule: root-224 -->

<a id="root-226"></a>

## Core Rules — root-226

<!-- preserved-rule: root-226 -->

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

<!-- /preserved-rule: root-226 -->
