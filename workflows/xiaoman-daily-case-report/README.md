# Workflow: Xiaoman Daily Character Universe Report

`workflows/xiaoman-daily-case-report` generates a character-driven daily community
poster for Xiaoman community groups. The production recurrence is a Xiaoman Hermes cron
job that reads the latest rolling 24 hours of QiWe group messages, calls the
release-managed worker to render a JPEG poster, and publishes it automatically to the
reviewed target group through the governed QiWe image-send boundary.

The current script generates the report image locally and emits artifact identity. The
sidecar has the reviewed binding command that turns a durable JPEG URI into an approved
`generated_image` artifact plus one automatic QiWe send-ready work item. Production
recurrence is installed through the Hermes cron apply path, while render/upload,
observation, rollback, and send boundaries still run from the immutable release.

## Responsibility

- Read QiWe group messages from `qintopia_messages.messages` for the latest rolling
  24-hour window.
- Count only text messages with non-empty `text`; image, emoji-only, system, and blank
  messages are excluded from activity statistics.
- Keep raw text-message counts as the top-line activity metric, but filter obvious
  payment prompts, copy-token promotions, and external-platform shopping redirects out
  of highlights, topic cards, and MVP ranking.
- Keep every displayed highlight and topic-card excerpt traceable to a source-group
  message in the report window. The renderer must omit a section when it has no
  qualifying source text; it must not fill the space with generated copy, fixed quotes,
  or synthetic fallback commentary.
- Aggregate message count, active participant count, hourly timeline, and topical case
  cards.
- Add a `今日剧中人` section from the same latest message window. When production
  read-through is active, the ranking may use sanitized long-term role recurrence counts
  from `qintopia_identity.member_facts`, but it never displays `fact_text` or hidden
  profile snapshot content.
- Show a keyword hotlist only from repeated source-message tokens or repeated complete
  Chinese phrases, together with the matching message and participant counts. A phrase
  must occur in at least two distinct source messages; omit the hotlist when the report
  window has no qualifying keyword.
- Keep the character-universe body intact: headline metrics, 今日主线, source-message
  highlight, 人物出场表, hot topics, 梗和回调候选, 同场关系, and story cards remain the
  primary sections.
- Write a private Markdown daily report alongside the poster so downstream operators can
  review a text日报, not only the image artifact.
- Write a private `.character-universe.json` second-pass export alongside the poster and
  Markdown. It keeps people, topics, events, memes, callbacks, same-topic relationships,
  creative-profile candidates, storyline candidates, and graph edges from curated report
  content only; it does not retain raw messages or hidden profile fact text.
- Write a `wx-cli`-style private review bundle alongside the poster: `.quote-map.json`,
  `.wiki-bundle.json`, `.run-manifest.json`, `.review.md`, and
  `.creative-profile-review-payload.draft.json`. These files add the reference project's
  quote-map / Wiki / run-manifest / review structure while preserving the latest
  Postgres chat-record source of truth. They are internal review artifacts, not public
  or send-ready payloads.
- Render a mobile-friendly JPEG poster from the character-daily template. The HTML
  preview and production image share the same report layout.
- Emit the content hash, file MD5, byte size, MIME type, and filename needed for the
  downstream sendable artifact boundary.
- Bind safe production metadata for the new report shape: content counts,
  character-universe schema/source, candidate counts, and privacy flags only. Do not
  persist Markdown body, raw character-universe nodes, member names, or excerpts in
  send-ready metadata.
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

Topic clustering is heuristic by default: explicit colon markers are honored only when
they look like real thread labels such as topics, recaps, shares, asks, or activity
discussion markers; weak chatty colon sentences are treated as normal messages. Messages
are otherwise grouped around top keywords extracted from discussion-quality text. Case
storyline titles are also promoted into the private Wiki topic layer when they are real
topics; generic time-bucket fillers such as `早场 10:00 时段` are excluded from Wiki
topics. Promotional payment/copy-token redirects remain counted in raw activity totals,
but they are not allowed to become the daily highlight, topic cards, or MVP entries.
This keeps the workflow deterministic and free of LLM costs. A future iteration can add
an optional LLM-based case title step behind an explicit flag.

Character notes follow the reference `wx-cli` project’s useful pattern: daily output
separates current-window behavior from long-term character memory. The displayed role is
derived from today’s source messages; long-term Postgres profile facts only contribute
bounded recurrence counts and a coarse role label such as `活动推进者` or `故事线雷达`.
When owner-reviewed `creative_profile` snapshots already exist, the workflow reads only
their `safe_reply_hints` / `communication_style` safe fields and reuses them for
cross-day arc, meme, callback, and story-function hints. It does not read or publish
snapshot `summary`, raw facts, or hidden profile text; today's role and evidence still
come from the latest message window. The workflow also emits private
`xiaoman-daily-case-report-*` review artifacts: `character-universe.json`,
`quote-map.json`, `wiki-bundle.json`, `run-manifest.json`, and `review.md`. This matches
the reference project's Wiki/graph/review idea with a safer source policy: people,
topics, events, storyline candidates, quote anchors, and edges come from the generated
daily report layer, not from raw chat archives. It also emits
`creative_profile_candidates` as private review material with
`public_surface_allowed=false`, safe evidence anchors, recurrence evidence counts, and
`profile_upgrade_status`. `daily_note_only` candidates remain daily notes. Only
`eligible_for_review` candidates may be copied into the separate reviewed payload for
`apply_creative_profile_candidates.py`, where an owner-reviewed `person_id` mapping is
required before any durable `creative_profile` snapshot can be written.

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

Image rendering prefers Playwright when the reviewed runtime already provides it, and
falls back to the local Pillow renderer with system fonts when browser binaries are not
available. Do not install Python packages or browsers manually on production servers.

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
media upload values. The Hermes cron wrapper is
`runtime/hermes/scripts/qintopia_xiaoman_daily_case_report.sh`, with the reviewed
declaration at `runtime/hermes/cron/xiaoman/daily-case-report.job.json`. The retired
systemd timer is retained only as a rollback target after the Hermes job is disabled.

### Reviewed creative-profile apply

`apply_creative_profile_candidates.py` is the narrow apply path for owner-approved
long-term character profiles. It does not read raw chat archives or infer identity from
display names. The input must be a reviewed JSON payload that copies only
`eligible_for_review` candidates and supplies a reviewed `person_id` UUID for each one.
Use `build_creative_profile_review_payload.py` to turn a daily
`.character-universe.json` file into an owner-review draft. The draft intentionally
leaves `person_id` blank, marks eligible candidates as `pending_review`, and marks
`daily_note_only` candidates as rejected unless `--include-rejected` is used for review
context; the owner must bind stable UUIDs and explicitly change accepted candidates to
`approved` before the payload can pass the apply validator.

Review draft generation:

```bash
python workflows/xiaoman-daily-case-report/build_creative_profile_review_payload.py \
  --character-universe-json xiaoman-daily-case-report.character-universe.json \
  --reviewed-by owner-review > reviewed-creative-profile-payload.draft.json
```

Dry-run validation:

```bash
python workflows/xiaoman-daily-case-report/apply_creative_profile_candidates.py \
  --payload-json reviewed-creative-profile-payload.json
```

Database apply additionally requires:

```bash
python workflows/xiaoman-daily-case-report/apply_creative_profile_candidates.py \
  --payload-json reviewed-creative-profile-payload.json \
  --apply \
  --approval approved-production-xiaoman-creative-profile-candidates
```

The apply report retains only sanitized counts and privacy flags. It must not print
person ids, database URLs, raw messages, or profile fact text.

Production apply is intentionally a fixed runtime one-shot, not an ad-hoc script upload.
Use `Run Production Runtime One-Shot` with target
`xiaoman-creative-profile-candidates-apply`, approval
`approved-production-xiaoman-creative-profile-candidates`, and the SHA-256 of the fixed
server-local reviewed payload at
`/home/ubuntu/.local/state/qintopia-agentos/xiaoman-creative-profile-candidates/reviewed-payload.json`.
The workflow must not accept payload JSON, payload paths, names, person ids, or
candidate text.

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
- The private Markdown日报 is generated in the same `0700` output directory with mode
  `0600`. The production auto-publish worker runs in a temporary directory and removes
  it after upload/publish creation; retained production evidence must keep only
  sanitized metrics and artifact identity.
- The private character-universe JSON is generated in the same `0700` output directory
  with mode `0600`. It may contain member display names and curated report excerpts, so
  it follows the same temporary production cleanup policy as Markdown and HTML.
- The private creative-profile review-payload draft is generated in the same `0700`
  output directory with mode `0600`. It may contain candidate role labels and evidence
  anchors, but every eligible candidate remains `pending_review` with blank `person_id`;
  production metadata may retain only candidate counts and privacy flags.
- Reviewed `creative_profile` reuse is a read-only style/memory input for the daily
  report. The query is keyed by reviewed `person_id`, active
  `profile_kind='creative_profile'`, and
  `profile_version='xiaoman-daily-creative-profile-v1'`; it reads only
  `safe_reply_hints` / `communication_style` and fails soft so latest message reporting
  remains available when the profile layer is unavailable.
- The private quote-map, wiki-bundle, run-manifest, and review report are generated in
  the same `0700` output directory with mode `0600`. They may contain curated excerpts,
  labels, and candidate nodes, so production worker metadata may retain only their
  presence, counts, and privacy flags.
- Auto-publish metadata retains only safe counters and schema flags from the private
  Markdown/universe/review-bundle outputs. Production evidence can prove the upgraded
  character universe and private review bundle paths ran, without retaining people
  labels, story labels, quote text, relationship labels, or source excerpts.
- Production JPEG/database runs use fixed local runtime tools only. Database
  read-through prefers `psycopg` when already present and otherwise falls back to the
  reviewed `/usr/bin/psql` boundary without placing the database URL in command
  arguments, with a minimal `PATH`, `PG*` connection fields, and SQL on stdin so `psql`
  variable substitution is applied. Image rendering prefers Python Playwright when
  available and otherwise uses the system Pillow renderer plus system fonts.
  Hand-installed Python packages or browsers remain outside the approved production
  boundary.
- The automatic publisher uses the dedicated `xiaoman.daily_case_report_auto_publish`
  capability and `review_policy=automatic_publish`; only
  `workflow_type=daily_case_report` may bypass per-day human final confirmation.
- Auto-publish creation must carry `media_upload_evidence` from the reviewed media
  upload command. The create command rechecks the public media base, allowed host,
  content hash, MD5, byte size, dimensions, MIME type, and filename before it can
  approve the artifact or queue QiWe send-ready.
- Daily scheduling must be installed by the reviewed Hermes cron apply script with
  observation and rollback checks. A hand-copied systemd unit, local-image-path sender,
  or unreviewed runtime cron edit is not an acceptable production activation.

## Validation

- `pnpm workflows:check` validates the workflow manifest.
- `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s workflows/xiaoman-daily-case-report/tests -v`
  validates the local render boundary and date/output contracts.
- `node tools/deploy/check-xiaoman-daily-case-report-character-universe-local.mjs`
  validates the character-universe generation, private-output boundary, worker metadata,
  production observation allowlist, and runbook coverage needed before release.
- Creative-profile apply boundary test:

  ```bash
  PYTHONDONTWRITEBYTECODE=1 python3 -m unittest \
    workflows/xiaoman-daily-case-report/tests/test_build_creative_profile_review_payload.py \
    workflows/xiaoman-daily-case-report/tests/test_apply_creative_profile_candidates.py -v
  ```

  validates the review-payload draft and reviewed creative-profile apply boundaries.

- `python3 workflows/xiaoman-daily-case-report/daily_case_report.py --dry-run --render html`
  validates the template generation path without image rendering dependencies.
