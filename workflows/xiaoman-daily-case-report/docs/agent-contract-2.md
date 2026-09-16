# Daily report contract — part 2

These are retained operating constraints, not a new permission grant. Read the sections
relevant to the change before editing. Original conditions and exceptions remain
binding; summaries in AGENTS.md do not replace them.

Backtick paths in root-source rules are repository-relative; paths in Sidecar-source
rules retain their original `runtime/sidecar/` base. New navigation links are relative
to this document.

[Migration inventory](../../../docs/plans/active/agents-guidance/README.md) records the
baseline and source locations.

## root-110

<!-- preserved-rule: root-110 -->

- Production immediate worker/backfill runs should use the
  `Run Production Runtime One-Shot` GitHub workflow after the reviewed release
  containing the runner support is deployed. Erhua one-shots still require the
  corresponding release-managed timer to be enabled; Xiaoman daily case-report backfill
  is the reviewed Hermes-cutover exception and must not require the retired systemd
  timer to be enabled. Control targets enforce their fixed target-specific
  preconditions. It creates a signed `production-runtime-one-shot` deploy-runner request
  and accepts exactly one fixed target per request: `erhua-morning-brief` with
  `approved-production-erhua-morning-brief-one-shot`, or
  `xiaoman-daily-case-report-auto-publish-backfill` with
  `approved-production-xiaoman-daily-case-report-auto-publish-backfill` and `YYYY-MM-DD`
  backfill date, or `xiaoman-daily-case-report-approval-repair` /
  `xiaoman-daily-case-report-read-through-repair` /
  `xiaoman-daily-case-report-chat-id-repair` with
  `approved-production-xiaoman-daily-case-report-config-v1`, or
  `qiwe-image-send-intro-text-enable` with
  `approved-production-qiwe-image-send-intro-text-v1`, or
  `xiaoman-creative-profile-candidates-apply` with
  `approved-production-xiaoman-creative-profile-candidates` and the 64-hex SHA-256 of
  the fixed server-local reviewed payload, or `hermes-cron-snapshot-install` with
  `approved-production-hermes-cron-snapshot` and empty `backfill_date` when
  `hermes-cron-snapshot` observation reports
  `hermes_cron_snapshot_observation_error=unit_missing`, or `qiwe-webhook-ingress-apply`
  with `approved-production-qiwe-webhook-ingress-apply`, or
  `qiwe-webhook-ingress-rollback` with
  `approved-production-qiwe-webhook-ingress-rollback`, or
  `space-automation-runtime-rollback` with
  `approved-production-space-automation-runtime-rollback`. The snapshot target installs
  only the fixed server-local snapshot timer and baseline snapshot repo, then should be
  followed by observation with `hermes-cron-snapshot,hermes-cron-live-parity`. This path
  may create real production publish/send side effects through the reviewed worker
  boundaries, but it must not write persistent config, enable/disable business worker
  timers, retire cron files, accept multiple targets, or record raw worker output, live
  cron JSON, group ids, prompts, database URLs, tokens, person ids, reviewed profile
  payload content, Feishu payloads, QiWe payloads, message content, snapshot contents,
  or journal logs. Runtime one-shot entrypoints must emit a bounded
  `qintopia_runtime_one_shot_safe_failure=` marker for every pre-worker failure as well
  as worker failures; otherwise deploy results collapse to bare `exit 1` and production
  troubleshooting loses the reviewed boundary.

<!-- /preserved-rule: root-110 -->

## root-118

<!-- preserved-rule: root-118 -->

- Build the non-secret Xiaoman production completion manifest after the Huabaosi canary,
  real-activity, and QiWe group-arrival evidence checks pass. Run it where `gh` can
  verify the Release Please PR, QiWe production enablement PR, and published release
  commit facts:

  ```bash
  node tools/deploy/build-xiaoman-production-completion-manifest.mjs \
    --release-please-pr-number <release-please-pr-number> \
    --release-please-head-sha <release-please-head-sha> \
    --release-tag <published-release-tag> \
    --released-commit-sha <published-release-commit-sha> \
    --qiwe-production-enablement-pr-number <qiwe-production-enablement-pr-number> \
    --qiwe-production-enablement-head-sha <qiwe-production-enablement-head-sha> \
    --huabaosi-production-canary <production-canary-output.txt> \
    --production-real-activity <production-evidence-output.txt> \
    --qiwe-group-arrival-confirmation <qiwe-group-arrival-confirmation-output.txt> \
    --daily-case-report-observation <production-observation-deploy-result.json> \
    --output <completed-xiaoman-production-completion-evidence.json>
  ```

<!-- /preserved-rule: root-118 -->

## root-119

<!-- preserved-rule: root-119 -->

- Full Xiaoman production completion evidence validation after all completion gates have
  retained sanitized evidence:

  ```bash
  node tools/deploy/check-xiaoman-production-completion-evidence.mjs \
    --manifest <completed-xiaoman-production-completion-evidence.json> \
    --staging-runtime-readiness <staging-runtime-readiness-output.txt> \
    --huabaosi-staging <huabaosi-staging-output.txt> \
    --qiwe-staging <qiwe-staging-output.txt> \
    --huabaosi-production-canary <huabaosi-production-canary-output.txt> \
    --production-real-activity <production-evidence-output.txt> \
    --qiwe-group-arrival-confirmation <qiwe-group-arrival-confirmation-output.txt> \
    --daily-case-report-observation <production-observation-deploy-result.json>
  ```

<!-- /preserved-rule: root-119 -->

## root-120

<!-- preserved-rule: root-120 -->

- One-shot final Xiaoman production completion manifest build + validation from retained
  sanitized evidence:

  ```bash
  node tools/deploy/finalize-xiaoman-production-completion-evidence.mjs \
    --release-please-pr-number <release-please-pr-number> \
    --release-please-head-sha <release-please-head-sha> \
    --release-tag <published-release-tag> \
    --released-commit-sha <published-release-commit-sha> \
    --qiwe-production-enablement-pr-number <qiwe-production-enablement-pr-number> \
    --qiwe-production-enablement-head-sha <qiwe-production-enablement-head-sha> \
    --staging-runtime-readiness <staging-runtime-readiness-output.txt> \
    --huabaosi-staging <huabaosi-staging-output.txt> \
    --qiwe-staging <qiwe-staging-output.txt> \
    --huabaosi-production-canary <huabaosi-production-canary-output.txt> \
    --production-real-activity <production-evidence-output.txt> \
    --qiwe-group-arrival-confirmation <qiwe-group-arrival-confirmation-output.txt> \
    --daily-case-report-observation <production-observation-deploy-result.json> \
    --output <completed-xiaoman-production-completion-evidence.json>
  ```

<!-- /preserved-rule: root-120 -->

## root-221

<!-- preserved-rule: root-221 -->

- A real Xiaoman activity may be described as production-complete only after the
  retained sanitized evidence passes
  `tools/deploy/check-xiaoman-real-activity-production-evidence.mjs` and the full
  completion manifest plus staging/production evidence files pass
  `tools/deploy/check-xiaoman-production-completion-evidence.mjs`. The report may keep
  only the fixed schema ids, AgentOS UUIDs, release/database hashes, reviewed
  `runtime_artifact_profile` facts, the owner-approved sidecar binary hash,
  release-binary verification booleans, `artifact_content_hash`, reviewed PR
  numbers/head SHAs, production Release commit binding, boolean execution facts, and
  Xiaoman daily case-report safe counters/schema flags (`xiaoman-character-universe-v1`,
  `daily_case_report_second_pass`, `raw_messages_included=false`,
  `profile_fact_text_included=false`); it must not retain raw QiWe callback bodies,
  request ids, file credentials, group ids, message ids, media URLs, database URLs,
  provider responses, raw chat, raw logs, daily-report Markdown, or raw character nodes.

<!-- /preserved-rule: root-221 -->

## root-228

<!-- preserved-rule: root-228 -->

- Xiaoman daily case-report auto-publish must use the reviewed AgentOS artifact plus
  QiWe image-send boundary. Do not treat a local image path, hand-copied systemd unit,
  conversation-created cron, Python QiWe sender, deprecated synchronous upload shortcut,
  caller-provided HTTPS image URL without `media_upload_evidence`, or committed group id
  as an acceptable automatic publication path.

<!-- /preserved-rule: root-228 -->

## root-229

<!-- preserved-rule: root-229 -->

- Xiaoman daily case-report top-line message totals stay raw, but highlights, topic
  cards, and MVP ranking must filter obvious payment prompts, copy-token promotions, and
  external-platform shopping redirects so a long spam-like message cannot become the
  day's featured quote.

<!-- /preserved-rule: root-229 -->

## root-230

<!-- preserved-rule: root-230 -->

- Xiaoman daily case-report colon-based topic markers must be strong labels such as
  topics, recaps, shares, asks, or activity discussions. Ordinary chatty colon sentences
  must break marker carry-over instead of capturing later messages into a fake topic
  card.

<!-- /preserved-rule: root-230 -->

## root-231

<!-- preserved-rule: root-231 -->

- Xiaoman daily case-report backfill must start the reviewed release-local
  `xiaoman-daily-case-report-auto-publish-backfill.sh` entrypoint with the exact owner
  approval, reviewed release SHA, and `YYYY-MM-DD` date. The worker may honor
  `QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_DATE` only when paired with the matching backfill
  approval, and the backfill script may temporarily export
  `QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED=1` only for that worker
  process; do not send missed reports by local image path, retired timer starts, or
  ad-hoc QiWe calls.

<!-- /preserved-rule: root-231 -->

## root-232

<!-- preserved-rule: root-232 -->

- Xiaoman daily case-report character-universe outputs are private second-pass
  artifacts. Production worker logs and send-ready metadata may retain only safe
  counters and schema flags; never retain Markdown body, raw universe nodes, member
  labels, story labels, or source excerpts.

<!-- /preserved-rule: root-232 -->

## root-233

<!-- preserved-rule: root-233 -->

- Xiaoman daily case-report may reuse active reviewed `creative_profile` snapshots only
  as read-only style memory keyed by stable `person_id`. The read path may use only
  `safe_reply_hints` and `communication_style` fields from
  `profile_version='xiaoman-daily-creative-profile-v1'`; never read or publish snapshot
  `summary`, raw messages, fact text, private profile text, or display-name-guessed
  identities. If this layer fails, keep generating the latest-message daily report.

<!-- /preserved-rule: root-233 -->

## root-234

<!-- preserved-rule: root-234 -->

- Xiaoman daily case-report creative-profile candidates may be applied only through the
  reviewed candidate path. The daily export must keep `person_id` out of
  `creative_profile_candidates`; a separate owner-reviewed payload supplies the exact
  person UUID mapping and may include only `eligible_for_review` candidates. The
  production wrapper reads only the fixed
  `/home/ubuntu/.local/state/qintopia-agentos/xiaoman-creative-profile-candidates/reviewed-payload.json`,
  requires its approved SHA-256 and
  `approved-production-xiaoman-creative-profile-candidates`, and may be triggered only
  by the fixed `production-runtime-one-shot` target
  `xiaoman-creative-profile-candidates-apply`. The workflow accepts only the release
  SHA, fixed target, approval phrase, and payload SHA-256; it must not accept payload
  JSON, payload paths, names, person ids, or candidate text. The production result may
  report only sanitized counts/privacy flags plus the reviewed payload SHA-256. Never
  infer identity from display name, apply `daily_note_only`, print person ids, retain
  candidate text in production evidence, or accept arbitrary payload paths from workflow
  inputs.

<!-- /preserved-rule: root-234 -->
