# Morning brief contract

These are retained operating constraints, not a new permission grant. Read the sections
relevant to the change before editing. Original conditions and exceptions remain
binding; summaries in AGENTS.md do not replace them.

Backtick paths in root-source rules are repository-relative; paths in Sidecar-source
rules retain their original `runtime/sidecar/` base. New navigation links are relative
to this document.

[Migration inventory](../../../docs/plans/active/agents-guidance/README.md) records the
baseline and source locations.

<a id="root-032"></a>

## Commands — root-032

<!-- preserved-rule: root-032 -->

- Erhua morning brief fixture test:
  `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s workflows/erhua-morning-brief/tests -v`

<!-- /preserved-rule: root-032 -->

<a id="root-033"></a>

## Commands — root-033

<!-- preserved-rule: root-033 -->

- Erhua morning brief systemd timer observation (rollback-to-timer path only):
  `QINTOPIA_ERHUA_MORNING_BRIEF_TIMER_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/erhua-morning-brief-timer-observation-smoke.sh`

<!-- /preserved-rule: root-033 -->

<a id="root-034"></a>

## Commands — root-034

<!-- preserved-rule: root-034 -->

- Erhua morning brief reviewed production schedule is `08:10 Asia/Shanghai`, pinned by
  the Hermes cron registry expr `10 8 * * *` in
  `runtime/hermes/cron/reviewed-cron-jobs.json`.

<!-- /preserved-rule: root-034 -->

<a id="root-035"></a>

## Commands — root-035

<!-- preserved-rule: root-035 -->

- Erhua morning brief AI news defaults to eight items. English items must carry explicit
  Chinese title and summary translations before they can appear in the brief; do not
  send English-only RSS fallback items as-is.

<!-- /preserved-rule: root-035 -->

<a id="root-036"></a>

## Commands — root-036

<!-- preserved-rule: root-036 -->

- Erhua morning brief should top up QunMind public AI news with reviewed RSS fallback
  items when QunMind returns fewer than eight usable items. The QunMind parser must not
  treat body labels such as summaries, source links, or translation notes as additional
  news items just to fill the limit.

<!-- /preserved-rule: root-036 -->

<a id="root-037"></a>

## Commands — root-037

<!-- preserved-rule: root-037 -->

- Erhua morning brief RSS dedup history is a publish-success record for selected RSS
  titles. When QunMind is topped up by RSS, record only the RSS additions; do not record
  QunMind's own public report titles in the RSS history file.

<!-- /preserved-rule: root-037 -->

<a id="root-038"></a>

## Commands — root-038

<!-- preserved-rule: root-038 -->

- Erhua morning brief Rust RSS parsing reads public XML; keep `quick-xml >= 0.41.0` and
  preserve explicit handling for text, CDATA, and `GeneralRef` entity events. Do not
  downgrade or simplify this path without rerunning RustSec advisories and RSS parser
  regressions.

<!-- /preserved-rule: root-038 -->

<a id="root-039"></a>

## Commands — root-039

<!-- preserved-rule: root-039 -->

- Erhua morning brief RSS fallback should prefer fewer high-signal items over filling
  the card with weakly related items. Keep the built-in fallback list to clearly AI
  focused public sources; do not add generic tech/news feeds as defaults unless there is
  a reviewed quality gate proving they cannot dominate the brief with low-signal items.

<!-- /preserved-rule: root-039 -->

<a id="root-040"></a>

## Commands — root-040

<!-- preserved-rule: root-040 -->

- Erhua morning brief news presentation should keep safe public article links as
  `来源：...` lines and end the section with a resident-facing discussion prompt. Keep
  only `https` links without embedded credentials; local paths, credentials, internal
  ids, and other internal markers stay blocked.

<!-- /preserved-rule: root-040 -->

<a id="root-041"></a>

## Commands — root-041

<!-- preserved-rule: root-041 -->

- Erhua morning brief poster rendering should stay in an editorial brief style: clean
  paper background, strong title, two-digit numbered news rows, thin dividers, source
  lines, and a one-sentence summary. Do not reintroduce cartoon-like heavy borders,
  decorative badges, placeholder logo marks, internal producer labels, or brand marks
  copied from reference images.

<!-- /preserved-rule: root-041 -->

<a id="root-042"></a>

## Commands — root-042

<!-- preserved-rule: root-042 -->

- Erhua morning brief chat-facing text must read like a resident-facing group message,
  not an operations ticket. Block internal planning wording such as `需要前置`,
  `可宣发`, `宣发判断`, `计划类活动`, `活动状态`, `宣发状态`, and Feishu status labels
  before artifact creation or QiWe send; the worker should fail closed rather than send
  that wording to the group. Keep activity status and readiness evidence in logs or
  sanitized evidence, not in the group-facing brief.

<!-- /preserved-rule: root-042 -->

<a id="root-043"></a>

## Commands — root-043

<!-- preserved-rule: root-043 -->

- Erhua morning brief QiWe text-send fixture:
  `cargo run --quiet --manifest-path runtime/sidecar/Cargo.toml -- run-qiwe-text-send-worker --once --fixture-mode`

<!-- /preserved-rule: root-043 -->

<a id="root-061"></a>

## Commands — root-061

<!-- preserved-rule: root-061 -->

- Erhua morning brief reviewed production config apply/disable:

  ```bash
  QINTOPIA_ERHUA_MORNING_BRIEF_PRODUCTION_CONFIG=approved-production-erhua-morning-brief-config \
    deploy/sidecar/scripts/apply-erhua-morning-brief-production-config.sh --enable
  QINTOPIA_ERHUA_MORNING_BRIEF_PRODUCTION_CONFIG=approved-production-erhua-morning-brief-config \
    deploy/sidecar/scripts/apply-erhua-morning-brief-production-config.sh --disable
  ```

<!-- /preserved-rule: root-061 -->

<a id="root-062"></a>

## Commands — root-062

<!-- preserved-rule: root-062 -->

- Erhua morning brief systemd timer activation (rollback-to-timer path only) after
  Release promotion and reviewed persistent env approval:
  `QINTOPIA_ERHUA_MORNING_BRIEF_ACTIVATION=approved-production-erhua-morning-brief deploy/sidecar/scripts/activate-erhua-morning-brief-production.sh`

<!-- /preserved-rule: root-062 -->

<a id="root-063"></a>

## Commands — root-063

<!-- preserved-rule: root-063 -->

- Erhua morning brief systemd timer rollback after persistent env disables the timer:
  `QINTOPIA_ERHUA_MORNING_BRIEF_ROLLBACK=approved-production-erhua-morning-brief-rollback deploy/sidecar/scripts/rollback-erhua-morning-brief-production.sh`

<!-- /preserved-rule: root-063 -->

<a id="root-101"></a>

## Commands — root-101

<!-- preserved-rule: root-101 -->

- Erhua morning brief text auto-publish must keep external sending in the separate
  `run-qiwe-text-send-worker` path. It may send only the reviewed
  `text_activity_announcement` / `text_announcement` work item after artifact approval,
  final confirmation, send-ready evidence, exact content hash binding, target group
  allowlist, `approved-production-erhua-morning-brief-auto-publish`, and
  `approved-production-qiwe-text-send`. Do not make `run-group-message-send-worker`
  perform a real QiWe send, and do not generalize the text worker into arbitrary group
  messaging.

<!-- /preserved-rule: root-101 -->

<a id="root-113"></a>

## Commands — root-113

<!-- preserved-rule: root-113 -->

- Erhua morning brief now uses a Hermes cron job (task 5), not the release-managed daily
  timer. The reviewed declaration is `runtime/hermes/cron/erhua/morning-brief.job.json`,
  the wrapper is `runtime/hermes/scripts/qintopia_erhua_morning_brief.sh`, and the
  registry entry pins expr `10 8 * * *`. Install and enable the Hermes job with
  `QINTOPIA_ERHUA_MORNING_BRIEF_HERMES_CRON=approved-production-erhua-morning-brief-hermes-cron`
  plus `apply-erhua-morning-brief-hermes-cron.sh --install` then `--enable`. Disable the
  old timer with `rollback-erhua-morning-brief-production.sh` before enabling the job;
  the wrapper must export `QINTOPIA_ERHUA_MORNING_BRIEF_ENABLED=1` and
  `QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_PUBLISH_ENABLED=1` only for the Hermes worker
  process, so the retired systemd path can remain disabled in persistent env without
  blocking the Hermes run; the auto-publish approvals
  `approved-production-erhua-morning-brief-auto-publish` and
  `approved-production-qiwe-text-send` are unchanged. Before enabling the Hermes job,
  activate the reviewed `hermes-profile-erhua` overlay so the Erhua profile has
  `channel.wecom.enabled=true`; the overlay may manage only that boolean under
  `channel.wecom`, preserving runtime-local bot bindings and never reporting channel
  values. Ordinary release `smoke-release` must not verify live profile files against a
  new release overlay unless it is running the `hermes-profile-erhua` activation path
  with profile metadata; otherwise a release that merely carries the next overlay can
  fail before the reviewed profile activation request has a chance to apply it. Health
  is the allowlist observation smoke plus the snapshot git history. Follow
  `docs/operations/erhua-morning-brief-hermes-cron-runbook.md`.

<!-- /preserved-rule: root-113 -->

<a id="sidecar-045"></a>

## Rules — sidecar-045

<!-- preserved-rule: sidecar-045 -->

- `run-qiwe-text-send-worker` is only for the Erhua morning-brief text send path. It may
  process only `text_activity_announcement` work items backed by approved
  `text_announcement` artifacts, final confirmation, send-ready evidence, exact
  artifact/content-hash binding, and an allowlisted QiWe target group. Apply must use
  the `qiwe-production` companion runtime, `QINTOPIA_QIWE_TEXT_SEND_ENABLED=1`, the
  exact `approved-production-qiwe-text-send` phrase, and the reviewed production
  database URL hash before Postgres or QiWe network access. Do not reuse it as a generic
  text sender, do not bypass `run-group-message-send-worker` send-ready evidence, and
  record ambiguous QiWe outcomes with `external_send_executed=null`.

<!-- /preserved-rule: sidecar-045 -->
