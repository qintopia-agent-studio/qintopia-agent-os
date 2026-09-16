# Daily report contract — part 1

These are retained operating constraints, not a new permission grant. Read the sections
relevant to the change before editing. Original conditions and exceptions remain
binding; summaries in AGENTS.md do not replace them.

Backtick paths in root-source rules are repository-relative; paths in Sidecar-source
rules retain their original `runtime/sidecar/` base. New navigation links are relative
to this document.

[Migration inventory](../../../docs/plans/active/agents-guidance/README.md) records the
baseline and source locations.

<a id="root-015"></a>

## Commands — root-015

<!-- preserved-rule: root-015 -->

- Xiaoman daily case-report character-universe local readiness check:
  `node tools/deploy/check-xiaoman-daily-case-report-character-universe-local.mjs`

<!-- /preserved-rule: root-015 -->

<a id="root-016"></a>

## Commands — root-016

<!-- preserved-rule: root-016 -->

- Xiaoman daily case-report private review bundle now includes `.draft-bundle.json`. It
  may contain ordinary digest, light-roast, public-draft, storyline timeline, and
  7/14/30-day lookback callback candidates. The ordinary digest should follow the
  reference `wx-cli` content-workshop shape: weather slot, one-sentence summary, main
  topics, people notes, local-life notes, open questions, risk items, and public-topic
  candidates. Production evidence may retain only `draft_counts` plus privacy flags,
  never the candidate text.

<!-- /preserved-rule: root-016 -->

<a id="root-017"></a>

## Commands — root-017

<!-- preserved-rule: root-017 -->

- Xiaoman daily case-report reference-project attachment/media slots must stay explicit
  empty review fields until a reviewed attachment source exists. Do not read
  `messages.raw`, attachment tokens, filenames, media URLs, or image payloads for daily
  digest/poster content.

<!-- /preserved-rule: root-017 -->

<a id="root-018"></a>

## Commands — root-018

<!-- preserved-rule: root-018 -->

- Xiaoman daily case-report production evidence may retain only the fixed
  `public_output_style` schema/boolean contract proving the character-daily layout,
  storyline-first output, image-first group delivery, PDF-non-default delivery, roast
  review boundary, and private-draft boundary. Never retain rendered Markdown, labels,
  quotes, relationship text, or candidate draft text as style evidence. Worker metadata
  must preserve negative boundaries as negative booleans: `pdf_default_delivery=false`
  and `public_surface_contains_private_draft=false` are success evidence, not failures
  to coerce to `true`.

<!-- /preserved-rule: root-018 -->

<a id="root-019"></a>

## Commands — root-019

<!-- preserved-rule: root-019 -->

- Xiaoman daily case-report JPEG rendering must stay storyline/character-first in both
  HTML screenshot and Pillow fallback paths. Keep `人物出场表`, `今日台词`,
  `梗和回调候选`, `同场关系`, `地点 / 本地生活线索`, `待解决问题`, and `故事线候选`
  before `24H 活跃节奏` / `发言出场榜` so production hosts without Playwright do not
  regress to a statistics-first poster. The production auto-publish worker must call the
  renderer with `--json-summary-only`; it may consume only `character_universe_summary`,
  `public_output_style`, private-review counts, artifact identity, and paths. Do not
  parse or forward full `daily_report_markdown`, full `character_universe`, quote-map,
  wiki, draft, run-manifest, or operator-review text in the send chain.

<!-- /preserved-rule: root-019 -->

<a id="root-020"></a>

## Commands — root-020

<!-- preserved-rule: root-020 -->

- Xiaoman daily case-report auto-publish uses the Rust renderer as the primary path, but
  the worker may fall back to the reviewed Python/Pillow pipeline only when the Rust
  path fails before upload at `rasterize rendered HTML`. Do not fallback after media
  upload, auto-publish creation, or QiWe send-state errors; those phases are idempotency
  and delivery boundaries, not safe rerender triggers.

<!-- /preserved-rule: root-020 -->

<a id="root-021"></a>

## Commands — root-021

<!-- preserved-rule: root-021 -->

- Xiaoman daily case-report creative-profile apply boundary test:

  ```bash
  PYTHONDONTWRITEBYTECODE=1 python3 -m unittest \
    workflows/xiaoman-daily-case-report/tests/test_build_creative_profile_review_payload.py \
    workflows/xiaoman-daily-case-report/tests/test_apply_creative_profile_candidates.py -v
  ```

<!-- /preserved-rule: root-021 -->

<a id="root-022"></a>

## Commands — root-022

<!-- preserved-rule: root-022 -->

- Xiaoman daily case-report production approval repair is the only reviewed one-shot
  path for restoring a missing
  `QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_PRODUCTION_APPROVAL` after Hermes
  cutover. Use `Run Production Runtime One-Shot` with
  `runtime_one_shot_target=xiaoman-daily-case-report-approval-repair` and
  `approval=approved-production-xiaoman-daily-case-report-config-v1`; it may write only
  the fixed approval constant to `/etc/qintopia/message-sidecar.env`, must fail closed
  on duplicate/wrong values, and must never accept or expose chat ids, group ids, DB
  hashes, payload JSON, env values, or arbitrary config fields.

<!-- /preserved-rule: root-022 -->

<a id="root-023"></a>

## Commands — root-023

<!-- preserved-rule: root-023 -->

- Xiaoman daily case-report production read-through repair is the only reviewed one-shot
  path for restoring a missing
  `QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_READ_THROUGH_ENABLE=1` after Hermes cutover. Use
  `Run Production Runtime One-Shot` with
  `runtime_one_shot_target=xiaoman-daily-case-report-read-through-repair` and
  `approval=approved-production-xiaoman-daily-case-report-config-v1`; it may write only
  the fixed read-through enable constant to `/etc/qintopia/message-sidecar.env`, must
  fail closed on duplicate/wrong values, and must never accept or expose chat ids, group
  ids, DB hashes, payload JSON, env values, or arbitrary config fields.

<!-- /preserved-rule: root-023 -->

<a id="root-024"></a>

## Commands — root-024

<!-- preserved-rule: root-024 -->

- Xiaoman daily case-report production chat-id repair is the only reviewed one-shot path
  for restoring a missing `QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_CHAT_ID` after Hermes
  cutover. Use `Run Production Runtime One-Shot` with
  `runtime_one_shot_target=xiaoman-daily-case-report-chat-id-repair` and
  `approval=approved-production-xiaoman-daily-case-report-config-v1`; it may write only
  the chat id copied from the fixed Xiaoman Hermes profile env `WECOM_HOME_CHANNEL` to
  `/etc/qintopia/message-sidecar.env`, must fail closed on duplicate/wrong values, and
  must never accept or expose chat ids, group ids, DB hashes, payload JSON, env values,
  or arbitrary config fields.

<!-- /preserved-rule: root-024 -->

<a id="root-058"></a>

## Commands — root-058

<!-- preserved-rule: root-058 -->

- Production Hermes cron live apply should use the `Apply Production Hermes Crons`
  GitHub workflow after the reviewed release containing the runner support is deployed.
  It creates a signed `production-hermes-cron-apply` deploy-runner request and accepts
  only `apply_mode=install|enable` plus these fixed targets: `erhua-morning-brief`,
  `erhua-activity-recruitment`, `xiaoman-daily-case-report`,
  `xiaoman-weekly-recruitment`, `xiaoman-weekly-plan-confirmation`, and
  `xiaoman-weekly-preview`. This is the repository-to-live step for
  `/home/ubuntu/.hermes/profiles/<profile>/cron/jobs.json`: install first writes the
  reviewed job disabled and installs the reviewed wrapper under
  `/home/ubuntu/.hermes/profiles/<profile>/scripts/`; enable is a later explicit request
  after live declaration parity is proven. Hermes no-agent scheduler resolves the
  `script` field from this profile-local scripts directory, not from the global
  `/home/ubuntu/.hermes/scripts/` helper area. Apply scripts must converge profile-local
  wrapper ownership to the live cron file/profile owner and mode `0700` even when the
  wrapper content is already current; preserving a stale root-owned wrapper can make
  Hermes report install success while the ubuntu profile cannot execute it. The approval
  strings authorize the production action boundary, not a cryptographic signature over
  `jobs.json`. When adding reviewed registry entries, update the workflow input
  allowlist, deploy-request schema, deploy-runner target allowlist, runner dispatch, and
  fixtures in the same PR; otherwise live parity can require a job that the reviewed
  apply workflow cannot install. Any runner-invoked apply script and its wrapped worker
  must also be listed in `tools/deploy/build-deploy-bundle.mjs`; otherwise production
  deploy can pass while the server release tree still lacks the script and returns
  `exit 127`. The workflow must not accept chat ids, cron JSON, script paths, approval
  strings, env-file paths, systemctl commands, or arbitrary shell from inputs, and
  deploy results must not record live cron content, group ids, prompts, env values, or
  raw script output. Apply scripts may emit a bounded safe failure reason only through
  the explicit `qintopia_hermes_cron_apply_safe_failure=` marker; the deploy runner must
  ignore all other stdout/stderr for result details. Because each apply ends by running
  `sync-hermes-cron-snapshot.sh`, the deploy-runner service must grant `ReadWritePaths`
  to the fixed server-local snapshot repo
  `/home/ubuntu/.local/state/qintopia-agentos/hermes-cron-snapshot`; do not broaden this
  to the whole qintopia-agentos state directory or whole home. Live Hermes cron
  `jobs.json` envelopes may exceed 64 KiB after multiple reviewed jobs are installed;
  apply and live-parity observation should accept the fixed 1 MiB ceiling, while
  wrappers and bounded evidence files keep their smaller limits. Legacy live `jobs.json`
  files may be an object with `jobs` but no `schema_version`; apply scripts should
  normalize that envelope to `schema_version: 1`, while rejecting any explicit
  unsupported schema version.

<!-- /preserved-rule: root-058 -->

<a id="root-104"></a>

## Commands — root-104

<!-- preserved-rule: root-104 -->

- Xiaoman daily case-report auto-publish binding after a reviewed render/upload step has
  produced a durable JPEG URI and identity. This creates/updates the approved
  `generated_image` artifact and one automatic `group_message_request`; it does not
  upload the local file, call QiWe directly, trust a bare caller-provided media URL, or
  accept a committed target group id. The create payload must include
  `media_upload_evidence` from the reviewed upload step and revalidate the reviewed
  media boundary plus JPEG identity before send-ready. Current production has no
  reviewed public HTTPS media upload endpoint for this workflow; use
  `QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_STORAGE_BACKEND=feishu-base` so the upload step
  stores the JPEG in the existing Huabaosi Feishu primary-storage table and QiWe reads
  it back through the Feishu delivery bridge:
  `qintopia-message-sidecar operations-daily-case-report-media-upload --payload-json <local-jpeg-identity-json> --apply`,
  `qintopia-message-sidecar operations-daily-case-report-auto-publish-create --payload-json <sanitized-json> --apply`
  Feishu-backed publish idempotency may reuse only an existing artifact whose id matches
  the reviewed upload evidence; reject conflicting random-id artifacts instead of
  writing an `artifact_uri` whose Feishu suffix no longer matches `artifacts.id`.

<!-- /preserved-rule: root-104 -->

<a id="root-105"></a>

## Commands — root-105

<!-- preserved-rule: root-105 -->

- Xiaoman daily case-report auto-publish now uses a Hermes cron job (task 4), not the
  release-managed daily timer. The reviewed declaration is
  `runtime/hermes/cron/xiaoman/daily-case-report.job.json`, the wrapper is
  `runtime/hermes/scripts/qintopia_xiaoman_daily_case_report.sh`, and the registry entry
  pins expr `0 9 * * *`. Install and enable the Hermes job with
  `QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_HERMES_CRON=approved-production-xiaoman-daily-case-report-hermes-cron`
  plus `apply-xiaoman-daily-case-report-hermes-cron.sh --install` then `--enable`.
  Disable the old timer with
  `rollback-xiaoman-daily-case-report-auto-publish-production.sh` before enabling the
  job; the rollback script requires
  `QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED=0` in the sidecar env while
  the worker requires `1`, so the wrapper exports that flag itself and the env file
  keeps `0` for the retired systemd path. Disabled production config must preserve the
  reviewed chat id, target group, read-through, storage backend, and message text keys
  already present in the sidecar env; only the retired systemd enablement flag should
  stay `0`, otherwise the Hermes worker fires on time but fails before render/upload.
  Production configuration still goes through the fixed release-local config entrypoint:
  `deploy/sidecar/scripts/apply-xiaoman-daily-case-report-production-config.py --stdin --apply --approval approved-production-xiaoman-daily-case-report-config-v1`.
  Observation and rollback for the retired systemd path are
  `xiaoman-daily-case-report-auto-publish-production-observation-smoke.sh` and
  `rollback-xiaoman-daily-case-report-auto-publish-production.sh`; the systemd
  activation script is kept only as the rollback target. Follow
  `docs/operations/xiaoman-daily-case-report-hermes-cron-runbook.md`. The send chain is
  unchanged: the worker uploads through the Huabaosi Feishu primary-storage boundary and
  records the `generated_image` artifact plus one automatic `group_message_request`;
  actual QiWe delivery rides the separate `operations-group-send-ready` chain. The
  `xiaoman-daily-case-report-auto-publish-backfill` one-shot workflow target stays valid
  after migration because it calls the worker boundary directly, temporarily exporting
  the worker enablement/date override for that process while leaving the persistent
  sidecar env flag at `0` and the retired systemd timer disabled.

<!-- /preserved-rule: root-105 -->

<a id="root-106"></a>

## Commands — root-106

<!-- preserved-rule: root-106 -->

- The Xiaoman daily case-report worker uploads through the Huabaosi Feishu primary
  storage boundary. Its release SHA binding now lives in the Hermes wrapper: after
  sourcing the persistent env, the wrapper derives the release SHA from
  `release/current` and exports it as both `QINTOPIA_DEPLOYED_COMMIT_SHA` and
  `QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA`; do not rely on stale persistent env
  release keys from `/etc/qintopia/message-sidecar.env`.

<!-- /preserved-rule: root-106 -->

<a id="root-107"></a>

## Commands — root-107

<!-- preserved-rule: root-107 -->

- The Xiaoman daily case-report production host does not provide Python `psycopg`,
  Python Playwright, or a Playwright browser binary by default. The reviewed production
  path must keep the fixed `/usr/bin/psql` database fallback and system Pillow renderer
  available through `/usr/bin/python3`; do not replace this with runtime package
  installation or browser downloads on the server. Database fallback must use a minimal
  `PATH`, keep the database URL out of process arguments, pass connection fields through
  `PG*` environment variables only, and feed SQL on stdin so `psql` variable
  substitution is applied.

<!-- /preserved-rule: root-107 -->

<a id="root-108"></a>

## Commands — root-108

<!-- preserved-rule: root-108 -->

- Production timer activation should use the `Activate Production Timers` GitHub
  workflow after the reviewed release containing the runner support is deployed. It
  creates a signed `production-activation` deploy-runner request and accepts only these
  fixed targets: `erhua-morning-brief`, `space-automation-runtime`,
  `xiaoman-weekly-recruitment`, `xiaoman-weekly-plan-confirmation`,
  `xiaoman-weekly-preview`, and `xiaoman-daily-case-report-auto-publish`. The activation
  request does not retire legacy cron files, write persistent production config, or
  promise automatic rollback; each selected target requires its owner-approved
  production config to have been applied first. Legacy cron retirement must be handled
  through the explicit `production-legacy-cron-retirement` request and evidenced before
  activation retries. The `xiaoman-weekly-recruitment` and
  `xiaoman-weekly-plan-confirmation` target additions are a 2026-08-09 owner-approved
  fixed-boundary expansion for the Xiaoman weekly minimum loop; they may enable only
  their own release-managed systemd timers and must not send, publish, write Feishu,
  call Erhua, or call QiWe. `space-automation-runtime` must be the only target in its
  activation request. It may enable only the generic dispatcher timer and Space
  execution worker after the fixed production approval, database hash, Qiwe host,
  companion artifact, authenticated ingress, and exact-unit observation checks pass. A
  release installation disables and stops both units; every new release therefore
  requires a fresh explicit activation.

<!-- /preserved-rule: root-108 -->

<a id="root-109"></a>

## Commands — root-109

<!-- preserved-rule: root-109 -->

- Production runtime observation should use the `Observe Production Runtime` GitHub
  workflow after the reviewed release containing the runner support is deployed. It
  creates a signed `production-observation` deploy-runner request and accepts only these
  fixed targets: `qiwe-image-send`, `space-automation-runtime`,
  `xiaoman-daily-case-report-auto-publish`, `hermes-cron-snapshot`,
  `hermes-cron-live-parity`, and the worker-run evidence targets
  `erhua-morning-brief-worker-run`, `xiaoman-daily-case-report-worker-run`,
  `xiaoman-weekly-recruitment-worker-run`,
  `xiaoman-weekly-plan-confirmation-worker-run`, and
  `xiaoman-weekly-preview-worker-run`. `hermes-cron-snapshot` reports only safe
  server-local snapshot unit/repo facts, and `hermes-cron-live-parity` reports only
  reviewed/live/enabled counts after comparing the reviewed registry to live
  declarations including `deliver` and `origin` boundaries. Do not hard-code the
  reviewed job count in live-parity observation; the registry may grow while the safe
  output remains bounded counts only. Migrated worker-run targets prove the reviewed
  Hermes cron wrapper wrote a latest `<timestamp> <task> run=ok` sentinel for the
  expected Asia/Shanghai schedule date and the worker exited successfully; stale
  sentinels from an older scheduled date must fail as `scheduled_run_missing`, not be
  classified as the current worker result. Erhua morning brief observation must also
  parse the worker's sanitized summary from that latest log segment and verify the text
  artifact was created plus the optional auto-publish summary reports
  `external_send_executed=true`; never fall back to sentinel-only success for Erhua
  sends. Weekly targets also validate the worker's `latest-summary.json` draft
  invariants. When the fixed Hermes cron log is absent or contains no reviewed sentinel
  for the task, the observation passes with `<key>_worker_run_result=not_started`;
  before the first scheduled trigger this means the Hermes job has not fired yet, not a
  regression, while `not_started` after the scheduled time means the Hermes job did not
  reach the reviewed wrapper and needs reviewed investigation. Observation is read-only:
  it may run only fixed release-local observation scripts, must not enable or disable
  timers, write persistent config, retire legacy cron files, call QiWe/Feishu/Postgres
  mutation commands, or run activation/rollback scripts, and must not print live
  `jobs.json`, group ids, prompts, env values, snapshot contents, or raw script output.
  `space-automation-runtime` additionally verifies the exact release-bound unit bytes,
  enabled/active state, scheduled timer value, and that the live worker PID resolves to
  the current immutable Qiwe companion binary; it never reports process arguments or
  environment values.

<!-- /preserved-rule: root-109 -->
