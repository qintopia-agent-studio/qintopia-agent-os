# Weekly workflow contract

These are retained operating constraints, not a new permission grant. Read the sections
relevant to the change before editing. Original conditions and exceptions remain
binding; summaries in AGENTS.md do not replace them.

Backtick paths in root-source rules are repository-relative; paths in Sidecar-source
rules retain their original `runtime/sidecar/` base. New navigation links are relative
to this document.

[Migration inventory](../plans/active/agents-guidance/README.md) records the baseline
and source locations.

<a id="root-076"></a>

## Commands — root-076

<!-- preserved-rule: root-076 -->

- Xiaoman activity Feishu read-through is not a generic Base browser: it stays
  read-only, bound to the two configured tables, and output-capped. Record columns pass
  through to chat-safe outputs by default (`ActivityRecordView.fields` from
  `runtime/sidecar/src/xiaoman_activity.rs` `passthrough_fields`, sanitized under the
  `fields` key by `skills/qintopia-tools/variants/xiaoman/__init__.py`), so a new
  activity-table column needs no code change to become chat-visible. Only internal
  identifiers, attachment payloads, and record links are blocked: the reviewed denylist
  lives in both `FEISHU_READ_DENIED_FIELDS` (Rust) and
  `XIAOMAN_ACTIVITY_READ_DENIED_FIELDS` (Python) and must stay in sync; adding a new
  sensitive column means an owner-reviewed denylist PR in both places, nothing else. A
  field still needs canonical treatment only when it drives workflow logic (parity
  comparison, weekly workers, schedule confirmation): then add it to
  `ActivityRecord`/`ActivityRecordView` normalization,
  `XIAOMAN_ACTIVITY_RECORD_READ_FIELDS`,
  `deploy/sidecar/scripts/xiaoman-activity-parity-check.py` (`COMPARABLE_FIELDS` and
  `canonical_record` aliases), and
  `deploy/sidecar/scripts/xiaoman-activity-production-parity-capture.sh`
  (`canonical_legacy_record`), or production parity reports false drift.

<!-- /preserved-rule: root-076 -->

<a id="root-112"></a>

## Commands — root-112

<!-- preserved-rule: root-112 -->

- Xiaoman weekly loop production:
  - Saturday recruitment recurrence lives in Xiaoman Hermes cron, not in a
    release-managed systemd timer.
    `/home/ubuntu/.hermes/profiles/xiaoman/cron/jobs.json` owns the schedule, the
    enablement, and the delivery target, while the business logic stays release-managed
    in `xiaoman-weekly-recruitment-worker.sh`. The reviewed declaration is
    `runtime/hermes/cron/xiaoman/weekly-recruitment.job.json`, the wrapper is
    `runtime/hermes/scripts/qintopia_xiaoman_weekly_recruitment.sh`, and the allowlist
    entry lives in `runtime/hermes/cron/reviewed-cron-jobs.json`. Apply it only with:

    ```bash
    QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_HERMES_CRON=approved-production-xiaoman-weekly-recruitment-hermes-cron \
      deploy/sidecar/scripts/apply-xiaoman-weekly-recruitment-hermes-cron.sh --install
    QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_HERMES_CRON=approved-production-xiaoman-weekly-recruitment-hermes-cron \
      deploy/sidecar/scripts/apply-xiaoman-weekly-recruitment-hermes-cron.sh --enable
    ```

    Because the rollback script requires `QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_ENABLED=0`
    in the sidecar env while the worker requires `1`, the wrapper exports that flag
    itself and the env file keeps `0` for the retired systemd path. Health signals after
    cutover are `xiaoman-legacy-cron-observation-smoke.sh` reporting reviewed
    declarations only plus the `sync-hermes-cron-snapshot.sh` history. The full
    procedure is `docs/operations/xiaoman-weekly-recruitment-hermes-cron-runbook.md`;
    the legacy systemd activation path in
    `docs/operations/xiaoman-weekly-loop-cutover-runbook.md` is kept only as the
    rollback target.

  - Xiaoman weekly plan confirmation now uses a Hermes cron job (task 3), not the
    release-managed Sunday timer. The reviewed declaration is
    `runtime/hermes/cron/xiaoman/weekly-plan-confirmation.job.json`, the wrapper is
    `runtime/hermes/scripts/qintopia_xiaoman_weekly_plan_confirmation.sh`, and the
    registry entry pins expr `0 20 * * 0`. Persistent config still goes through
    `apply-xiaoman-weekly-plan-confirmation-production-config.sh`; install and enable
    the Hermes job with
    `QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_HERMES_CRON=approved-production-xiaoman-weekly-plan-confirmation-hermes-cron`
    plus `apply-xiaoman-weekly-plan-confirmation-hermes-cron.sh --install` then
    `--enable`. Disable the old timer with
    `rollback-xiaoman-weekly-plan-confirmation-production.sh` before enabling the job;
    because the rollback script requires
    `QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_ENABLED=0` in the sidecar env while the
    worker requires `1`, the wrapper exports that flag itself and the env file keeps `0`
    for the retired systemd path. The wrapper also re-exports
    `QINTOPIA_DEPLOYED_COMMIT_SHA` from `release/current` after sourcing the persistent
    env, then executes the worker from the same resolved release directory. Health is
    the allowlist observation smoke plus the snapshot git history. Follow
    `docs/operations/xiaoman-weekly-plan-confirmation-hermes-cron-runbook.md`.

  The recruitment and plan-confirmation Hermes crons both keep using the Xiaoman
  activity wrapper boundary. Config, activation, observation, and workers must require
  `QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE=1`,
  `QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE=1`, and
  `QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE=1`. Activation and observation for both
  jobs must verify the installed unit's `QINTOPIA_DEPLOYED_COMMIT_SHA` against the
  owner-reviewed release SHA and must first pass
  `xiaoman-legacy-cron-observation-smoke.sh`. The recruitment Hermes cron's allowlist
  coverage is proven by that same smoke after cutover, so its health no longer depends
  on a live systemd unit. Rollback scripts are
  `rollback-xiaoman-weekly-recruitment-production.sh` and
  `rollback-xiaoman-weekly-plan-confirmation-production.sh`.

<!-- /preserved-rule: root-112 -->

<a id="root-114"></a>

## Commands — root-114

<!-- preserved-rule: root-114 -->

- Xiaoman weekly preview now uses a Hermes cron job (task 1), not the release-managed
  Monday timer. The reviewed declaration is
  `runtime/hermes/cron/xiaoman/weekly-preview.job.json`, the wrapper is
  `runtime/hermes/scripts/qintopia_xiaoman_weekly_preview.sh`, and the registry entry
  pins expr `30 9 * * 1`. Install and enable the Hermes job with
  `QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_HERMES_CRON=approved-production-xiaoman-weekly-preview-hermes-cron`
  plus `apply-xiaoman-weekly-preview-hermes-cron.sh --install` then `--enable`. Disable
  the old timer with `rollback-xiaoman-weekly-preview-production.sh` before enabling the
  job; the rollback script requires `QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_ENABLED=0` in the
  sidecar env while the worker requires `1`, so the wrapper exports that flag itself and
  the env file keeps `0` for the retired systemd path. The wrapper also re-exports
  `QINTOPIA_DEPLOYED_COMMIT_SHA` from `release/current` after sourcing the persistent
  env, so a stale env value cannot override the release binding. Health is the allowlist
  observation smoke plus the snapshot git history. Follow
  `docs/operations/xiaoman-weekly-preview-hermes-cron-runbook.md`; the legacy systemd
  activation path in `docs/operations/xiaoman-weekly-preview-cutover-runbook.md` is kept
  only as the rollback target. The weekly preview worker now writes both
  `latest-operator-review-message.txt` and `latest-weekly-poster-brief.json`. The poster
  brief is a review artifact only: it must not call the image provider, approve a
  generated image, queue QiWe, or send to a group unless a later reviewed AgentOS
  image-generation and auto-publish policy gate explicitly does that work. The reviewed
  intake for a ready weekly poster brief is
  `qintopia_xiaoman_weekly_poster_workflow_prepare`. It emits a bounded
  `operations-workflow-start` command (dry-run by default, week-plus-content idempotency
  key, `source_record_ref=weekly_preview:<monday>`) that creates one
  `activity_promotion` parent plus evidence and visual children. It must not call image
  providers, write Feishu, approve artifacts, queue QiWe, publish, or send; poster brief
  approval, generated-image review, and final group-send confirmation stay on the
  existing AgentOS human gates.

<!-- /preserved-rule: root-114 -->

<a id="root-115"></a>

## Commands — root-115

<!-- preserved-rule: root-115 -->

- Xiaoman activity read-through production config for release-managed Erhua/weekly
  workers must be applied through the reviewed release-local allowlist copier, not by
  sourcing the Xiaoman Hermes profile or hand-editing
  `/etc/qintopia/message-sidecar.env`. It copies only the five reviewed Xiaoman activity
  read-through keys from the fixed `0600 ubuntu:ubuntu` profile env into the fixed
  `0640 root:ubuntu` sidecar env after verifying `release/current`, and writes the
  non-secret `QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE=1` switch required by the Hermes
  wrappers:

  ```bash
  sudo -n /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/apply-xiaoman-activity-read-through-production-config.py \
    --release-sha <published-production-release-sha> \
    --apply \
    --approval approved-production-xiaoman-activity-read-through-config-v1
  ```

<!-- /preserved-rule: root-115 -->
