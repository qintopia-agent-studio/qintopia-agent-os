# Workflow: Erhua Morning Brief

`workflows/erhua-morning-brief` generates a daily morning text draft for 二花. The draft
combines today's Xiaoman activity preview with AI news from QunMind when available, or
from public RSS/Atom feeds when QunMind is not installed in production. In production it
can also publish the reviewed text to the allowlisted QiWe group when the explicit
auto-publish gates are enabled.

## Responsibility

- Read today's Xiaoman activity announcement through the existing
  `qintopia_xiaoman_activity_announcement_prepare(mode="same_day_preview")` path.
- If no activity is confirmed today, include a friendly prompt that encourages members
  to start an activity.
- On Sunday morning, if there are still no publishable activities after the Saturday
  collection reminder, generate a second gentle collection prompt.
- Run QunMind in `daily-report --public-only` mode when configured; otherwise fetch up
  to three public RSS/Atom items from the built-in AI news feeds.
- Produce a single Erhua-style morning text draft and an operator-review envelope.
- After a reviewed `text_announcement` artifact exists, prepare an AgentOS
  `group_message_request` payload for Erhua/QiWe delivery.
- In the release-managed worker only, auto-approve and send the early brief when
  `QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_PUBLISH_ENABLED=1`, the owner approval phrase is
  present, the target group id is allowlisted, and the QiWe text-send production
  boundary is valid.

## Why this exists

The request is "每天早上二花发一个内容": activities first, then fresh AI news. The
activity half already exists in Agent OS, and QunMind can own public-source AI news
collection where it is installed. Production can still run without host-local QunMind by
using a small public RSS fallback.

## How it works

`morning_brief.py` runs three read-only steps by default:

1. It gets same-day activity text from the Xiaoman wrapper, using read-through in real
   runtime or a sanitized fixture in tests.
2. It asks QunMind to generate a public-only daily report into a temporary markdown file
   if QunMind is configured or available on `PATH`.
3. If QunMind is unavailable, it fetches the configured public RSS/Atom feeds, strips
   URLs and internal markers, and composes the final morning brief.

When the run date is Sunday and the activity preview returns zero publishable
activities, the activity section switches from the generic "no confirmed activity today"
copy to a second collection prompt. This covers the weekend case where the Saturday
reminder ran but nobody has added or initiated an activity yet.

With `--prepare-send-request`, the script also builds the `operations-work-item-create`
payload for an approved `text_announcement` artifact. Without `--execute-send-request`,
this is only a command preview. With `--execute-send-request --apply-send-request`, the
sidecar creates an `awaiting_publish` `group_message_request`; that still does not send
to QiWe until a human final-confirmation step queues it.

The release-managed `erhua-morning-brief-worker.sh` owns the production auto-publish
mode. It generates the text once, creates the pending artifact, records an explicit
auto-review decision, creates the group-message request from the same text and content
hash, records final confirmation, records send-ready, and then calls
`run-qiwe-text-send-worker --once --apply` from the reviewed QiWe production sidecar
companion. If any gate is missing or QiWe returns an ambiguous outcome, the worker exits
non-zero instead of silently retrying or sending a fallback.

The default behavior fails closed only if neither QunMind nor the public feed fallback
can produce AI news. Operators may use `--allow-news-unavailable` for an explicit
degraded preview, but production scheduling should not silently send a "news missing"
fallback.

## Running it

### Fixture preview

```bash
python workflows/erhua-morning-brief/morning_brief.py \
  --date 2026-08-08 \
  --activity-fixture workflows/erhua-morning-brief/tests/fixtures/activity-one.json \
  --news-fixture workflows/erhua-morning-brief/tests/fixtures/qunmind-ai-report.md
```

### Runtime preview

```bash
export QINTOPIA_PROFILE_ID=xiaoman
export QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE=1
export QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE=1

python workflows/erhua-morning-brief/morning_brief.py --json
```

Optional QunMind override:

```bash
export QINTOPIA_ERHUA_MORNING_BRIEF_QUNMIND_BIN=/Users/qiaopengjun/Code/Rust/qunmind/target/debug/qunmind
export QINTOPIA_ERHUA_MORNING_BRIEF_QUNMIND_CONFIG="<local-qunmind-config-path>"
```

Optional RSS override:

```bash
export QINTOPIA_ERHUA_MORNING_BRIEF_NEWS_FEED_URLS="https://openai.com/news/rss.xml,https://blog.google/technology/ai/rss/"
```

The RSS fallback accepts only `https` URLs on the reviewed public hosts `openai.com`,
`blog.google`, and `deepmind.google`. It does not follow feed redirects, and it
revalidates the final response URL before reading the feed body.

### Prepare a reviewed send request

Create or preview the pending `text_announcement` artifact first:

```bash
python workflows/erhua-morning-brief/morning_brief.py \
  --date 2026-08-08 \
  --prepare-artifact \
  --json
```

This returns an `operations-text-announcement-artifact-create --dry-run` shell preview.
To execute the sidecar dry-run, add `--execute-artifact-create`. To create the pending
artifact in AgentOS, use both `--execute-artifact-create` and `--apply-artifact-create`.
The artifact must then be approved through `operations-artifact-review-decision`.

After approval, prepare the group-message request:

```bash
python workflows/erhua-morning-brief/morning_brief.py \
  --date 2026-08-08 \
  --prepare-send-request \
  --approved-artifact-id <approved-text-announcement-artifact-uuid> \
  --json
```

This returns the `operations-work-item-create --dry-run` shell preview. To execute the
sidecar dry-run, add `--execute-send-request`. To create the `awaiting_publish` work
item in AgentOS after artifact approval, use both `--execute-send-request` and
`--apply-send-request`.

The QunMind config path stays local runtime state when used. Do not commit it, print it
into operator-facing copy, or copy QunMind secrets into this repository.

### Tomorrow morning release path

Use `--publish-plan` to get the generated text plus the exact next command templates:

```bash
python workflows/erhua-morning-brief/morning_brief.py \
  --prepare-artifact \
  --execute-artifact-create \
  --apply-artifact-create \
  --publish-plan \
  --json
```

The JSON contains:

- `morning_brief_text`: the text 二花 should say.
- `artifact_create.stdout.artifact_id`: the pending `text_announcement` artifact id when
  artifact creation was applied.
- `publish_plan.steps`: templates for artifact approval, send-request creation, final
  confirmation, send-ready recording, and manual QiWe posting fallback.

After reviewing the text, approve the `text_announcement` artifact with an allowlisted
human `reviewer_id`, then create the group-message request:

```bash
python workflows/erhua-morning-brief/morning_brief.py \
  --prepare-send-request \
  --approved-artifact-id <approved-text-announcement-artifact-uuid> \
  --execute-send-request \
  --apply-send-request \
  --publish-plan \
  --json
```

Then final-confirm the created `group_message_request` work item and run
`run-group-message-send-worker --once --work-item-id <group-message-request-work-item-uuid> --apply`
to record send-ready state. This worker does not call QiWe. If no reviewed QiWe text
sender is active tomorrow morning, paste `morning_brief_text` into the approved group
channel manually after the AgentOS evidence steps.

For production auto-publish, set the reviewed runtime gates before activating the timer:

```bash
QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_PUBLISH_ENABLED=1
QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_PUBLISH_APPROVAL=approved-production-erhua-morning-brief-auto-publish
QINTOPIA_ERHUA_MORNING_BRIEF_TARGET_GROUP_ID=<allowlisted-qiwe-group-id>
QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_REVIEWER_ID=<allowlisted-reviewer-id>
QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_CONFIRMER_ID=<allowlisted-confirmer-id>
QINTOPIA_QIWE_TEXT_SEND_ENABLED=1
QINTOPIA_QIWE_TEXT_SEND_PRODUCTION_APPROVAL=approved-production-qiwe-text-send
QINTOPIA_QIWE_TEXT_SEND_PRODUCTION_DATABASE_URL_SHA256=<approved-production-database-url-sha256>
```

## Acceptance Scenarios

- With confirmed same-day activities, the brief includes the activity announcement and
  up to three AI news items.
- With no confirmed activity, the brief says there is no confirmed activity today and
  invites members to start one.
- On Sunday morning, with no publishable activity yet, the brief explicitly says the
  Saturday collection reminder still has no publishable activities and asks once more
  for activity ideas.
- QunMind markdown URLs and raw source links are not copied into the chat-facing brief.
- Missing QunMind news fails closed unless `--allow-news-unavailable` is explicitly set.
- `--prepare-artifact` builds a pending `text_announcement` artifact-create request and
  does not approve or send it.
- `--prepare-send-request` requires a UUID approved artifact id and binds the message
  text to `approved_artifact_content_hash`.
- CLI preview and preparation commands keep `external_send_executed=false` and
  `requires_human_confirmation=true`.
- Production auto-publish records `external_send_executed=true` only after the reviewed
  QiWe text worker receives a successful business response from `/msg/sendHyperText`.

## Production Boundary

- Reads Xiaoman activity records through the existing read-through boundary.
- Reads QunMind public-source output only; it must not read local QunMind chat-history
  inputs for this workflow.
- CLI writes no database rows unless `--execute-artifact-create --apply-artifact-create`
  or `--execute-send-request --apply-send-request` is explicitly set.
- The CLI write path can only create a pending `text_announcement` artifact or an
  `awaiting_publish` AgentOS `group_message_request`; it does not approve, confirm,
  queue, run send-ready, call Erhua, call QiWe, or send.
- The release-managed worker may approve, confirm, queue, record send-ready, and call
  QiWe only when the explicit auto-publish gates are present. It must use a runtime
  target group id that is also present in `QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS`.
- Does not call WeChat or Feishu. Public news network access, if any, is owned by
  QunMind or the reviewed RSS fallback; QiWe text delivery is owned by the reviewed
  sidecar companion.
- Requires the same secrets as Xiaoman activity read-through plus the runtime-local
  QunMind config, but none of those values are committed to git.
- Some historical Erhua/Xiaoman scheduled work may still live in Hermes profile
  `cron/jobs.json`. This workflow must not be activated by directly editing that runtime
  file. If a Hermes job currently owns the Saturday collection reminder or a morning
  broadcast, first capture its non-secret shape through read-only inventory, then move
  or replace it through reviewed deploy/runner or profile-bundle code.

## Validation

- `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s workflows/erhua-morning-brief/tests -v`
  validates fixture generation, no-activity fallback, AI section extraction, fail-closed
  news behavior, and send-request payload preparation.
- `node tools/workflows/check-workflows.mjs` validates the workflow manifest.

## Production Activation

The release deploy bundle includes this workflow and the release-local activation
scripts under `deploy/sidecar/scripts/`. The systemd renderer installs the timer
disabled; activation still requires an explicit owner command and reviewed persistent
runtime values.

The reviewed production shape:

- installs a release-managed timer for `08:10 Asia/Shanghai`
  (`OnCalendar=*-*-* 08:10:00`);
- keeps the morning brief after the Xiaoman daily case-report `07:45` window so the two
  morning group outputs do not start at the same minute;
- includes the Sunday morning no-publishable-activity follow-up path after the Saturday
  collection reminder;
- inventories any existing Hermes `cron/jobs.json` job for the same Saturday/Sunday
  collection surface before changing schedule ownership;
- proves the reviewed absolute QunMind binary/config are available on the server;
- creates a pending `text_announcement` artifact for the morning text;
- when auto-publish is disabled, leaves artifact approval, group-message request
  creation, final confirmation, and send-ready recording as separate gates;
- when auto-publish is enabled, completes those gates inside the release worker and
  calls the reviewed QiWe text sender once.

Use `docs/operations/erhua-morning-brief-production-activation-runbook.md` for the
activation and rollback sequence.

Do not activate this through a hand-copied cron, server-local script, or an unreviewed
direct QiWe sender.
