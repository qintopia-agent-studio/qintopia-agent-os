# Huabaosi image contract — part 2

These are retained operating constraints, not a new permission grant. Read the sections
relevant to the change before editing. Original conditions and exceptions remain
binding; summaries in AGENTS.md do not replace them.

Backtick paths in root-source rules are repository-relative; paths in Sidecar-source
rules retain their original `runtime/sidecar/` base. New navigation links are relative
to this document.

[Migration inventory](../../../docs/plans/active/agents-guidance/README.md) records the
baseline and source locations.

<a id="root-227"></a>

## Core Rules — root-227

<!-- preserved-rule: root-227 -->

- A QiWe production-enablement PR must retain Huabaosi staging generated-image evidence
  and QiWe staging send evidence that pass
  `tools/deploy/check-xiaoman-image-send-staging-evidence.mjs`, proving the Huabaosi
  final JPEG `content_hash` equals the QiWe `artifact_content_hash` without recording
  media URI, filename, MD5 value, callback credentials, group id, database URL, or raw
  provider output. Record that cross-flow result only in
  `docs/reports/templates/xiaoman-image-send-staging-evidence.md`; staging evidence is a
  prerequisite, not proof that production sending is complete.

<!-- /preserved-rule: root-227 -->

<a id="root-235"></a>

## Core Rules — root-235

<!-- preserved-rule: root-235 -->

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

<!-- /preserved-rule: root-235 -->

<a id="root-241"></a>

## Core Rules — root-241

<!-- preserved-rule: root-241 -->

- `run-huabaosi-image-generation-worker` defaults to
  `QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED=0`. Production generation may run only
  from a release artifact compiled with the reviewed `huabaosi-production-adapter`
  feature, explicit production enablement bound to the deployed release SHA and database
  URL hash, valid provider/media configuration, and the fixed production timer. It may
  create only pending `generated_image` artifacts; it must not approve, publish, write
  Feishu, or send QiWe.

<!-- /preserved-rule: root-241 -->

<a id="root-246"></a>

## Core Rules — root-246

<!-- preserved-rule: root-246 -->

- The Huabaosi production image-generation service and timer may be installed from the
  immutable release but must not be enabled by the ordinary release installer. After the
  owner manually publishes the Release, the reviewed activation command must run the
  no-network preflight from that release and then enable the fixed timer for canary
  generation. Rollback disables the timer first and turns the generation enable flag off
  through reviewed runtime configuration. Do not repurpose this timer for artifact
  approval, Feishu, QiWe, or publishing.

<!-- /preserved-rule: root-246 -->

<a id="root-248"></a>

## Core Rules — root-248

<!-- preserved-rule: root-248 -->

- `operations-artifact-review-decision` may approve a `generated_image` only after its
  Huabaosi worker provenance, stable JPEG HTTPS URI, final JPEG sha256/metadata, source
  PNG sha256, fixed `png_to_jpeg_white_background_q92_v1` transform metadata, source
  brief/prompt refs, and `generated_image_created` audit match its image-generation
  request. Human approval applies to the exact final JPEG bytes; the transient provider
  PNG is never an approvable artifact. Integrity denial must leave the artifact pending
  and must not complete the work item or unlock downstream send intake.

<!-- /preserved-rule: root-248 -->

<a id="root-249"></a>

## Core Rules — root-249

<!-- preserved-rule: root-249 -->

- Generated-image media URIs used by Huabaosi artifact creation, operations approval,
  and QiWe send intake must reject raw backslashes and percent-encoded path separators
  before URL parsing; parsers or downstream services may normalize them into path
  separators, which can hide unstable or secret-shaped input from later filename checks.

<!-- /preserved-rule: root-249 -->

<a id="root-251"></a>

## Core Rules — root-251

<!-- preserved-rule: root-251 -->

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

<!-- /preserved-rule: root-251 -->

<a id="root-252"></a>

## Core Rules — root-252

<!-- preserved-rule: root-252 -->

- An expired or structurally incomplete Huabaosi image-generation `processing` claim is
  an unknown provider/media outcome. Reconciliation must atomically mark it failed,
  release the complete claim tuple, append one sanitized ambiguous-outcome event, and
  disable automatic retry; it must never reclaim the row for another external attempt.

<!-- /preserved-rule: root-252 -->

<a id="root-253"></a>

## Core Rules — root-253

<!-- preserved-rule: root-253 -->

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

<!-- /preserved-rule: root-253 -->

<a id="root-254"></a>

## Core Rules — root-254

<!-- preserved-rule: root-254 -->

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

<!-- /preserved-rule: root-254 -->

<a id="root-260"></a>

## Core Rules — root-260

<!-- preserved-rule: root-260 -->

- `huabaosi-image-generation-preflight` may only validate and emit a sanitized summary
  of local image-adapter configuration. It must not open network or database
  connections, reveal configuration values, enable generation, write Feishu, send QiWe,
  or publish. Its `missing_configuration` field may contain only fixed public env names
  already documented in `.env.example`; it must never contain values, URLs, hosts, ids,
  or enable flags.

<!-- /preserved-rule: root-260 -->

<a id="root-261"></a>

## Core Rules — root-261

<!-- preserved-rule: root-261 -->

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

<!-- /preserved-rule: root-261 -->

<a id="root-262"></a>

## Core Rules — root-262

<!-- preserved-rule: root-262 -->

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

<!-- /preserved-rule: root-262 -->

<a id="root-263"></a>

## Core Rules — root-263

<!-- preserved-rule: root-263 -->

- `render-staging-runtime-env.py` must render the same reviewed Huabaosi Feishu Base
  staging key contract consumed by `huabaosi-image-generation-staging-smoke.sh`. It must
  not require or emit the retired HTTP media upload/public URL keys for the Huabaosi
  staging path, and the generation evidence must prove the worker returned a
  `feishu-base://` artifact boundary rather than trusting the env-selected storage
  backend.

<!-- /preserved-rule: root-263 -->

<a id="root-265"></a>

## Core Rules — root-265

<!-- preserved-rule: root-265 -->

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

<!-- /preserved-rule: root-265 -->

<a id="sidecar-032"></a>

## Rules — sidecar-032

<!-- preserved-rule: sidecar-032 -->

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

<!-- /preserved-rule: sidecar-032 -->

<a id="sidecar-040"></a>

## Rules — sidecar-040

<!-- preserved-rule: sidecar-040 -->

- Expired or incomplete Huabaosi image-generation `processing` claims must become a
  sanitized terminal ambiguous outcome before new work is selected. Never infer from a
  lost lease that provider generation or media upload stayed local, and never reclaim
  that row for automatic external retry.

<!-- /preserved-rule: sidecar-040 -->
