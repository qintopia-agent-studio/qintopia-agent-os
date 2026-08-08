# Workflow: Erhua Morning Brief

`workflows/erhua-morning-brief` generates a daily morning text draft for 二花. The draft
combines today's Xiaoman activity preview with AI news extracted from QunMind's
public-source daily report.

## Responsibility

- Read today's Xiaoman activity announcement through the existing
  `qintopia_xiaoman_activity_announcement_prepare(mode="same_day_preview")` path.
- If no activity is confirmed today, include a friendly prompt that encourages members
  to start an activity.
- On Sunday morning, if there are still no publishable activities after the Saturday
  collection reminder, generate a second gentle collection prompt.
- Run QunMind in `daily-report --public-only` mode and extract up to three items from
  its `AI 前沿` section.
- Produce a single Erhua-style morning text draft and an operator-review envelope.
- After a reviewed `text_announcement` artifact exists, prepare an AgentOS
  `group_message_request` payload for Erhua/QiWe delivery.
- Never send, publish, or bypass the final confirmation gate.

## Why this exists

The request is "每天早上二花发一个内容": activities first, then fresh AI news. The
activity half already exists in Agent OS, and QunMind already owns public-source AI news
collection. This workflow connects the two without making a new crawler or teaching
Erhua to improvise from raw sources every morning.

## How it works

`morning_brief.py` runs three read-only steps by default:

1. It gets same-day activity text from the Xiaoman wrapper, using read-through in real
   runtime or a sanitized fixture in tests.
2. It asks QunMind to generate a public-only daily report into a temporary markdown
   file.
3. It extracts the `AI 前沿` section, strips URLs and internal markers, and composes the
   final morning brief.

When the run date is Sunday and the activity preview returns zero publishable
activities, the activity section switches from the generic "no confirmed activity today"
copy to a second collection prompt. This covers the weekend case where the Saturday
reminder ran but nobody has added or initiated an activity yet.

With `--prepare-send-request`, the script also builds the `operations-work-item-create`
payload for an approved `text_announcement` artifact. Without `--execute-send-request`,
this is only a command preview. With `--execute-send-request --apply-send-request`, the
sidecar creates an `awaiting_publish` `group_message_request`; that still does not send
to QiWe until a human final-confirmation step queues it.

The default behavior fails closed if QunMind cannot produce an AI section. Operators may
use `--allow-news-unavailable` for an explicit degraded preview, but production
scheduling should not silently send a "news missing" fallback.

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
export QINTOPIA_ERHUA_MORNING_BRIEF_QUNMIND_BIN=qunmind
export QINTOPIA_ERHUA_MORNING_BRIEF_QUNMIND_CONFIG="<local-qunmind-config-path>"

python workflows/erhua-morning-brief/morning_brief.py --json
```

In the current local desktop workspace, `qunmind` may not be on `PATH`. Use the existing
local build when needed:

```bash
export QINTOPIA_ERHUA_MORNING_BRIEF_QUNMIND_BIN=/Users/qiaopengjun/Code/Rust/qunmind/target/debug/qunmind
export QINTOPIA_ERHUA_MORNING_BRIEF_QUNMIND_CONFIG="<local-qunmind-config-path>"
```

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

The QunMind config path stays local runtime state. Do not commit it, print it into
operator-facing copy, or copy QunMind secrets into this repository.

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
- `external_send_executed` is always `false`; `requires_human_confirmation` is always
  `true`.

## Production Boundary

- Reads Xiaoman activity records through the existing read-through boundary.
- Reads QunMind public-source output only; it must not read local QunMind chat-history
  inputs for this workflow.
- Writes no database rows unless `--execute-artifact-create --apply-artifact-create` or
  `--execute-send-request --apply-send-request` is explicitly set.
- The write path can only create a pending `text_announcement` artifact or an
  `awaiting_publish` AgentOS `group_message_request`. Neither path approves, confirms,
  queues, runs send-ready, calls Erhua, calls QiWe, or sends.
- Does not call WeChat, Feishu, or public news sources directly. Network access, if any,
  is owned by QunMind's reviewed public-source daily-report pipeline.
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

Merging this workflow does not install a morning timer or send to a group. Activation
needs a separate owner-approved deploy change that:

- installs a release-managed timer for `08:05 Asia/Shanghai`
  (`OnCalendar=*-*-* 08:05:00`);
- keeps the morning brief after the Xiaoman daily case-report `07:45` window so the two
  morning group outputs do not start at the same minute;
- includes the Sunday morning no-publishable-activity follow-up path after the Saturday
  collection reminder;
- inventories any existing Hermes `cron/jobs.json` job for the same Saturday/Sunday
  collection surface before changing schedule ownership;
- proves the reviewed QunMind binary/config are available on the server;
- creates/imports and approves the reviewed `text_announcement` artifact for the morning
  text;
- routes the final text through Erhua/QiWe with idempotency, target allowlist,
  observation, and rollback evidence.

Do not activate this through a hand-copied cron, server-local script, or direct QiWe
send.
