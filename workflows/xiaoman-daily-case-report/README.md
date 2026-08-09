# Workflow: Xiaoman Daily Community Case-File Report

`workflows/xiaoman-daily-case-report` generates a playful daily "group chat case file"
poster for Xiaoman community groups. The production target is a release-managed daily
timer that reads the latest rolling 24 hours of QiWe group messages, renders a JPEG
poster, and publishes it automatically to the reviewed target group through the governed
QiWe image-send boundary.

The current script generates the report image locally and emits artifact identity. The
sidecar has the reviewed binding command that turns a durable JPEG URI into an approved
`generated_image` artifact plus one automatic QiWe send-ready work item. Production
activation still needs the render/upload entrypoint, runtime packaging, timer,
observation, and rollback path from the immutable release.

## Responsibility

- Read QiWe group messages from `qintopia_messages.messages` for the latest rolling
  24-hour window.
- Count only text messages with non-empty `text`; image, emoji-only, system, and blank
  messages are excluded from activity statistics.
- Aggregate message count, active participant count, hourly timeline, and topical case
  cards.
- Render a mobile-friendly JPEG poster styled like a detective case file.
- Emit the content hash, file MD5, byte size, MIME type, and filename needed for the
  downstream sendable artifact boundary.
- Publish once per daily window to the reviewed QiWe target group after production
  activation.
- Never send from a local image path, a conversation-created cron, or an unreviewed
  server-local script.

## Why this exists

Community groups generate a lot of informal discussion every day. A visual, playful
summary helps members catch up on what happened, who was active, and which topics
mattered—without requiring anyone to scroll through hundreds of messages.

## How it works

`daily_case_report.py` runs in three modes:

1. **Database mode** (production): reads from Postgres when
   `QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_READ_THROUGH_ENABLE=1` and a database URL is
   configured.
2. **Fixture mode** (`--fixture path.json`): reads pre-canned messages for tests or
   demos.
3. **Dry-run mode** (`--dry-run`): uses deterministic demo data to validate the template
   and rendering pipeline.

Automatic publication is not implemented inside `daily_case_report.py` directly. The
release worker renders the JPEG, calls `operations-daily-case-report-media-upload` to
obtain a durable HTTPS URI, then calls
`operations-daily-case-report-auto-publish-create` so retries, allowlists, storage
identity, callback correlation, and send evidence stay in Postgres.

Topic clustering is heuristic by default: messages are grouped around top keywords
extracted from the text. This keeps the workflow deterministic and free of LLM costs. A
future iteration can add an optional LLM-based case title step behind an explicit flag.

## Running it

### Dry-run preview (no database)

```bash
python workflows/xiaoman-daily-case-report/daily_case_report.py --dry-run
```

### Render only HTML

```bash
python workflows/xiaoman-daily-case-report/daily_case_report.py --dry-run --render html
```

This mode treats the HTML file as the deliverable and keeps it on disk.

### Render JPEG with Playwright

```bash
python workflows/xiaoman-daily-case-report/daily_case_report.py --dry-run --render image
```

Image rendering requires Playwright and Chromium from a reviewed repository/runtime
package boundary. Do not install them manually on production servers; keep production
activation blocked until that dependency path is reviewed.

JPEG is the default image encoding because the governed QiWe image-send boundary uses
JPG identity. Use `--image-format png` only for local debugging.

### Production read-through

```bash
export QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_READ_THROUGH_ENABLE=1
export QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_CHAT_ID="<reviewed-qiwe-group-chat-id>"
export QINTOPIA_MESSAGE_STORE_DATABASE_URL="postgresql://..."
export QINTOPIA_DAILY_CASE_REPORT_MEMBER_COUNT=148

python workflows/xiaoman-daily-case-report/daily_case_report.py \
  --group-name "秦托邦的小伙伴（新）"
```

> Production read-through requires `--chat-id` or
> `QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_CHAT_ID`; do not commit real QiWe group IDs to
> git. Without `--date`, the query window is the latest rolling 24 hours ending at run
> time. Use `--date YYYY-MM-DD` only for a specific calendar-day backfill in
> `--timezone`.

### Production auto-publish entrypoint

The release-managed worker is:

```bash
deploy/sidecar/scripts/xiaoman-daily-case-report-auto-publish-worker.sh
```

It requires the persistent production env to provide read-through, target group, and
media upload values. The systemd timer is rendered as
`qintopia-agentos-xiaoman-daily-case-report-auto-publish.timer` and is activated only by
`activate-xiaoman-daily-case-report-auto-publish-production.sh`.

## Acceptance Scenarios

- `--dry-run --render html` exits 0, writes a retained HTML file, and prints a review
  message with stats and file paths.
- `--render image --keep-html` exits 0 and writes both `.html` and a `.jpg` file by
  default when Playwright is available; without `--keep-html`, the HTML file remains
  only an intermediate render surface.
- `--image-format png` is accepted for local debugging but is not the production
  auto-publish target.
- Database mode fails closed (non-zero exit) if read-through is not enabled or the
  database URL is missing.
- An empty rolling 24-hour window exits 0 with a report showing zero messages and no
  cases.
- Local generation reports `requires_human_confirmation=false` and
  `auto_publish_ready=false`: per-day human confirmation is not part of the target
  design, but the local script has not uploaded or sent anything.
- Production auto-publish mode creates exactly one sendable daily report artifact for a
  window and exactly one QiWe send attempt for the reviewed target group.

## Production Boundary

- This workflow becomes `risk_level: high` before activation because production
  read-through handles real group-message content and automatic publication performs an
  external QiWe send.
- Reads `qintopia_messages.messages` only; does not write to the message store.
- Current local generation does not send to QiWe, Feishu, or any other external channel.
  The future auto-publish step may send only through the reviewed QiWe image-send
  production adapter.
- Requires the same Postgres read credentials as the message-store search path.
- Production read-through accepts only `QINTOPIA_MESSAGE_STORE_DATABASE_URL` or
  `QINTOPIA_SIDECAR_DATABASE_URL`; generic `DATABASE_URL` is ignored.
- Production read-through requires a reviewed runtime `chat_id` and fails closed without
  one; no real QiWe group ID is committed as a source default.
- Rendering happens locally in the runtime environment; no external image service,
  remote font, or other third-party network resource is called.
- The default report window is the latest rolling 24 hours in the configured
  `--timezone` (default `Asia/Shanghai`) before querying Postgres. `--date` is an
  explicit calendar-day backfill mode.
- Production read-through rejects `--render html` and `--keep-html`. Intermediate HTML
  can contain real member names and message excerpts, so it is written only into a
  `0700` output directory as a `0600` file and is removed after image rendering or
  failure.
- Production database read-through prefers `psycopg` and falls back only to fixed
  `/usr/bin/psql` with a minimal `PATH`, without putting the database URL in process
  arguments. Production JPEG rendering still requires Playwright and Chromium to be
  present through a reviewed runtime packaging path; hand-installed Python packages or
  browsers are outside the approved production boundary for this draft.
- The automatic publisher uses the dedicated `xiaoman.daily_case_report_auto_publish`
  capability and `review_policy=automatic_publish`; only
  `workflow_type=daily_case_report` may bypass per-day human final confirmation.
- Auto-publish creation must carry `media_upload_evidence` from the reviewed media
  upload command. The create command rechecks the public media base, allowed host,
  content hash, MD5, byte size, dimensions, MIME type, and filename before it can
  approve the artifact or queue QiWe send-ready.
- Daily scheduling must be installed by reviewed deploy/runner code with observation and
  rollback checks. A hand-copied systemd unit or conversation-created cron is not an
  acceptable production activation.

## Validation

- `pnpm workflows:check` validates the workflow manifest.
- `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s workflows/xiaoman-daily-case-report/tests -v`
  validates the local render boundary and date/output contracts.
- `python3 workflows/xiaoman-daily-case-report/daily_case_report.py --dry-run --render html`
  validates the template generation path without image rendering dependencies.
