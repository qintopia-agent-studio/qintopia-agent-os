# Workflow: Xiaoman Daily Community Case-File Report

`workflows/xiaoman-daily-case-report` generates a playful daily "group chat case file"
poster for Xiaoman community groups. It reads the last 24 hours of QiWe group messages,
clusters discussion topics into "cases", highlights active participants as "suspects",
and renders a PNG poster for human review. It never sends automatically.

## Responsibility

- Read QiWe group messages from `qintopia_messages.messages` for a 24-hour window.
- Aggregate message count, active participant count, hourly timeline, and topical case
  cards.
- Render a mobile-friendly PNG poster styled like a detective case file.
- Produce an `operator_review_message` with the HTML and PNG file paths.
- Never send, publish, or write to external systems. A human confirms first.

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

### Render PNG with Playwright

```bash
# Playwright must be installed in the Python environment.
python -m pip install playwright
python -m playwright install chromium

python workflows/xiaoman-daily-case-report/daily_case_report.py --dry-run --render png
```

### Production read-through

```bash
export QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_READ_THROUGH_ENABLE=1
export QINTOPIA_MESSAGE_STORE_DATABASE_URL="postgresql://..."
export QINTOPIA_DAILY_CASE_REPORT_MEMBER_COUNT=148

python workflows/xiaoman-daily-case-report/daily_case_report.py \
  --chat-id 10859791146538059 \
  --group-name "秦托邦的小伙伴（新）"
```

> `--chat-id` and `--group-name` both default to the 秦托邦的小伙伴（新） group
> (`10859791146538059`), so in production you can omit both and just enable
> read-through.

## Acceptance Scenarios

- `--dry-run --render html` exits 0, writes a retained HTML file, and prints a review
  message with stats and file paths.
- `--render png --keep-html` exits 0 and writes both `.html` and `.png` files when
  Playwright is available; without `--keep-html`, the HTML file remains only an
  intermediate render surface.
- Database mode fails closed (non-zero exit) if read-through is not enabled or the
  database URL is missing.
- An empty 24-hour window exits 0 with a report showing zero messages and no cases.
- `external_send_executed` is always `false`; `requires_human_confirmation` is always
  `true`.

## Production Boundary

- Reads `qintopia_messages.messages` only; does not write to the message store.
- Does not send to QiWe, Feishu, or any other external channel.
- Requires the same Postgres read credentials as the message-store search path.
- Rendering happens locally in the runtime environment; no external image service,
  remote font, or other third-party network resource is called.
- The report window is computed in the configured `--timezone` (default `Asia/Shanghai`)
  before querying Postgres.

## Validation

- `pnpm workflows:check` validates the workflow manifest.
- `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s workflows/xiaoman-daily-case-report/tests -v`
  validates the local render boundary and date/output contracts.
- `python3 workflows/xiaoman-daily-case-report/daily_case_report.py --dry-run --render html`
  validates the template generation path without image rendering dependencies.
