# QiWe media contract

These are retained operating constraints, not a new permission grant. Read the sections
relevant to the change before editing. Original conditions and exceptions remain
binding; summaries in AGENTS.md do not replace them.

Backtick paths in root-source rules are repository-relative; paths in Sidecar-source
rules retain their original `runtime/sidecar/` base. New navigation links are relative
to this document.

[Migration inventory](../../../docs/plans/active/agents-guidance/README.md) records the
baseline and source locations.

<a id="root-025"></a>

## Commands — root-025

<!-- preserved-rule: root-025 -->

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

<!-- /preserved-rule: root-025 -->

<a id="root-093"></a>

## Commands — root-093

<!-- preserved-rule: root-093 -->

- QiWe image-send staging readiness smoke:

  ```bash
  QINTOPIA_QIWE_IMAGE_STAGING_READINESS_ENABLE=1 \
  QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL=approved-staging-qiwe-image-send \
  QINTOPIA_QIWE_IMAGE_STAGING_RELEASE_SHA=<approved-staging-release-sha> \
  QINTOPIA_QIWE_IMAGE_STAGING_SIDECAR_SHA256=<approved-staging-sidecar-sha256> \
    deploy/sidecar/scripts/qiwe-image-send-staging-readiness-smoke.sh
  ```

<!-- /preserved-rule: root-093 -->

<a id="root-094"></a>

## Commands — root-094

<!-- preserved-rule: root-094 -->

- QiWe image-send production observation smoke:
  `QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/qiwe-image-send-production-observation-smoke.sh`

<!-- /preserved-rule: root-094 -->

<a id="root-095"></a>

## Commands — root-095

<!-- preserved-rule: root-095 -->

- QiWe image callback bridge production observation smoke:
  `QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/qiwe-image-callback-bridge-production-observation-smoke.sh`

<!-- /preserved-rule: root-095 -->

<a id="root-096"></a>

## Commands — root-096

<!-- preserved-rule: root-096 -->

- QiWe image callback bridge production activation after manual Release publish and
  persistent Erhua env approval. The activation script validates the bridge is already
  bound to release/current, production mode, the approved sidecar SHA-256, and the
  approved production database URL hash before restarting Erhua; it must not enable
  timers, process callbacks, call QiWe, or source env files:
  `QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_ACTIVATION=approved-production-qiwe-image-callback-bridge deploy/sidecar/scripts/activate-qiwe-image-callback-bridge-production.sh`

<!-- /preserved-rule: root-096 -->

<a id="root-097"></a>

## Commands — root-097

<!-- preserved-rule: root-097 -->

- QiWe image callback bridge immediate rollback after persistent Erhua env disables the
  bridge:
  `QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_ROLLBACK=approved-production-qiwe-image-callback-bridge-rollback deploy/sidecar/scripts/rollback-qiwe-image-callback-bridge-production.sh`

<!-- /preserved-rule: root-097 -->

<a id="root-099"></a>

## Commands — root-099

<!-- preserved-rule: root-099 -->

- QiWe image-send immediate timer rollback:
  `QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_ROLLBACK=approved-production-qiwe-image-send-rollback deploy/sidecar/scripts/rollback-qiwe-image-send-production.sh`

<!-- /preserved-rule: root-099 -->

<a id="root-145"></a>

## Core Rules — root-145

<!-- preserved-rule: root-145 -->

- Keep QiWe adapter and sidecar NATS identities separate. NATS URLs must not contain
  userinfo; load producer and consumer credentials only from their distinct fixed
  private auth files. The producer may publish the authenticated raw subject but must
  not consume it, while the consumer may consume/ack the fixed durable stream but must
  not publish application events. Production Space automation activation must prove
  anonymous denial and these ACLs before any systemd mutation. That protocol preflight
  proves the configured subject/JetStream ACL only; a real shadow callback remains the
  required end-to-end consumption evidence.

<!-- /preserved-rule: root-145 -->

<a id="root-192"></a>

## Core Rules — root-192

<!-- preserved-rule: root-192 -->

- `qintopia_xiaoman_activity_text_group_message_request_prepare` may only prepare an
  `operations-create` command for an `erhua.send_group_message` /
  `group_message_request` from an approved text announcement artifact. It must require
  `approved_artifact_id`, bind `message_text` to the approved artifact `content_hash`,
  keep the request before human final confirmation, and must not queue, run send-ready,
  call Erhua, call QiWe, publish, send, or accept raw group ids, URLs, Feishu/Base ids,
  secrets, or unapproved text. Text announcement MVP evidence must not be used as
  Xiaoman production-complete evidence or as proof of QiWe group delivery; production
  completion still requires the image/send-ready/QiWe arrival evidence checkers.

<!-- /preserved-rule: root-192 -->

<a id="root-193"></a>

## Core Rules — root-193

<!-- preserved-rule: root-193 -->

- Xiaoman activity lifecycle phase is a Postgres `event_signals` fact. Allowed values
  are `pre_event`, `in_event`, and `post_event`; transitions are forward-only and each
  phase maps to its fixed root/child route. Event-signal root creation must lock and
  match the current phase. Do not accept caller-selected routes, rewrite historical
  phase roots, add a timer, or extend `in_event` routing into visual/image generation,
  Feishu writeback, QiWe send, or publishing. `post_event` may use the reviewed internal
  starter path from approved recap brief to image-generation request and then approved
  generated image to awaiting-publish group-message request; those starters must not
  call providers, write Feishu, confirm, queue, publish, call QiWe, or send.

<!-- /preserved-rule: root-193 -->

<a id="root-204"></a>

## Core Rules — root-204

<!-- preserved-rule: root-204 -->

- `run-xiaoman-activity-send-request-starter-worker` may only create an
  `awaiting_publish` AgentOS `erhua.send_group_message` / `group_message_request` child
  from an approved Xiaoman `generated_image` whose image-generation request is
  completed. It must not record final confirmation, queue the group message, run
  send-ready, publish, call QiWe, write Feishu, or call external adapters.

<!-- /preserved-rule: root-204 -->

<a id="root-207"></a>

## Core Rules — root-207

<!-- preserved-rule: root-207 -->

- `run-xiaoman-activity-image-generation-starter-worker` may only create an
  `image_generation_request` from an approved Xiaoman `poster_brief`; it must not call
  an image provider, upload media, write Feishu, send QiWe, or publish.

<!-- /preserved-rule: root-207 -->

<a id="root-208"></a>

## Core Rules — root-208

<!-- preserved-rule: root-208 -->

- `qintopia-agentos-xiaoman-activity-image-generation-starter-worker.timer` may only run
  `run-xiaoman-activity-image-generation-starter-worker --once --apply` for AgentOS
  image-generation request intake. Do not repurpose it for provider calls, media upload,
  generated-image creation, Feishu writeback, QiWe sends, or publishing.

<!-- /preserved-rule: root-208 -->

<a id="root-209"></a>

## Core Rules — root-209

<!-- preserved-rule: root-209 -->

- QiWe asynchronous `cmd=20000` callback events must be sanitized before NATS capture
  publication and independently before the sidecar writes Postgres. Persist only hashed
  correlation and fixed field-presence metadata; never publish or persist callback file
  credentials, media URLs, filenames, identities, message content, unknown values, or an
  unredacted callback event id. Invalid/dead-letter payloads must store only a digest
  and byte count, never the raw payload. A callback id is already sanitized only when it
  is exactly `qiwe-callback:` plus a 64-character hexadecimal SHA-256 digest; a prefix
  alone is untrusted and the complete value must be hashed again.

<!-- /preserved-rule: root-209 -->

<a id="root-210"></a>

## Core Rules — root-210

<!-- preserved-rule: root-210 -->

- QiWe callback credential-shape reports may emit only a fixed reviewed schema id and an
  additional-field count. They must reject simultaneous canonical and alias spellings
  and must never emit request ids, credential values, filenames, MD5 values, unknown
  field names, or unknown values.

<!-- /preserved-rule: root-210 -->

<a id="root-213"></a>

## Core Rules — root-213

<!-- preserved-rule: root-213 -->

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

<!-- /preserved-rule: root-213 -->

<a id="root-214"></a>

## Core Rules — root-214

<!-- preserved-rule: root-214 -->

- The QiWe upload claim transaction must persist an `uploading` attempt before external
  I/O. A stale `uploading` attempt or legacy unrecorded claim has an unknown external
  outcome and must become terminal `ambiguous` with `automatic_retry_allowed=false`;
  never requeue it automatically. Dry-run and disabled previews must enforce the same
  exact target-group and media-host allowlists as apply.

<!-- /preserved-rule: root-214 -->

<a id="root-218"></a>

## Core Rules — root-218

<!-- preserved-rule: root-218 -->

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

<!-- /preserved-rule: root-218 -->

<a id="root-223"></a>

## Core Rules — root-223

<!-- preserved-rule: root-223 -->

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

<!-- /preserved-rule: root-223 -->

<a id="root-225"></a>

## Core Rules — root-225

<!-- preserved-rule: root-225 -->

- A staging-feature QiWe callback apply must validate explicit enablement, exact
  API/media/group allowlists, and webhook readiness before reading stdin. Upload apply
  must validate the same adapter configuration before connecting to Postgres.

<!-- /preserved-rule: root-225 -->

<a id="root-236"></a>

## Core Rules — root-236

<!-- preserved-rule: root-236 -->

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

<!-- /preserved-rule: root-236 -->

<a id="root-238"></a>

## Core Rules — root-238

<!-- preserved-rule: root-238 -->

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

<!-- /preserved-rule: root-238 -->

<a id="sidecar-017"></a>

## Rules — sidecar-017

<!-- preserved-rule: sidecar-017 -->

- Do not place NATS credentials in `QINTOPIA_SIDECAR_NATS_URL`. Use the fixed private
  consumer auth file and keep it distinct from the QiWe producer auth file. Before
  production Space automation activation, the release-local preflight must prove
  anonymous denial, producer-only authenticated publication, consumer-only durable
  consumption/ack permissions, and the expected stream/consumer filters. This proves ACL
  configuration, not end-to-end JetStream delivery; retain one real authenticated shadow
  callback as consumption evidence before event automation activation.

<!-- /preserved-rule: sidecar-017 -->

<a id="sidecar-042"></a>

## Rules — sidecar-042

<!-- preserved-rule: sidecar-042 -->

- Sanitize QiWe asynchronous `cmd=20000` callback credentials before raw-event
  persistence. Dead letters may keep only payload length and digest; malformed payloads
  must not become a bypass that stores callback credentials or raw private text. Only
  preserve callback event/message ids matching `qiwe-callback:<64 hex SHA-256>`; hash
  the complete id again when a prefixed value has any other suffix.

<!-- /preserved-rule: sidecar-042 -->

<a id="sidecar-044"></a>

## Rules — sidecar-044

<!-- preserved-rule: sidecar-044 -->

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

<!-- /preserved-rule: sidecar-044 -->

<a id="sidecar-047"></a>

## Rules — sidecar-047

<!-- preserved-rule: sidecar-047 -->

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

<!-- /preserved-rule: sidecar-047 -->

<a id="sidecar-049"></a>

## Rules — sidecar-049

<!-- preserved-rule: sidecar-049 -->

- A staging-feature QiWe apply must require
  `QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL=approved-staging-qiwe-image-send` before
  adapter configuration, stdin, Postgres, or network access. The Cargo feature, enable
  flag, secrets, and allowlists do not substitute for this owner-reviewed one-shot gate.

<!-- /preserved-rule: sidecar-049 -->

<a id="sidecar-051"></a>

## Rules — sidecar-051

<!-- preserved-rule: sidecar-051 -->

- QiWe upload dry-run must use the same exact group/media allowlists and approved JPEG
  identity validator as apply. It may skip locks and writes, but not policy checks.

<!-- /preserved-rule: sidecar-051 -->
