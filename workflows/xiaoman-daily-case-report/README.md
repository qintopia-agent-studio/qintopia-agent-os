# Workflow: Xiaoman Daily Character Universe Report

`workflows/xiaoman-daily-case-report` generates a character-driven daily community
poster for Xiaoman community groups. The production recurrence is a Xiaoman Hermes cron
job that reads the most recently completed calendar day (00:00–24:00) of QiWe group
messages, calls the release-managed worker to render a JPEG poster, and publishes it
automatically to the reviewed target group through the governed QiWe image-send
boundary.

The current production worker renders through the Rust sidecar pipeline, delegates only
HTML-to-JPEG rasterization to the Python Playwright helper, and emits artifact identity.
The sidecar has the reviewed binding command that turns a durable JPEG URI into an
approved `generated_image` artifact plus one automatic QiWe send-ready work item.
Production recurrence is installed through the Hermes cron apply path, while
render/upload, observation, rollback, and send boundaries still run from the immutable
release.

## Responsibility

- Read QiWe group messages from `qintopia_messages.messages` for the most recently
  completed calendar day (00:00–24:00).
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
  highlight, 人物出场表, hot
  topics, 梗和回调候选, 同场关系, 地点 / 本地生活线索, 待解决问题, and story cards
  remain the primary sections. Activity rhythm and speaker ranking are secondary support
  sections, not the opening shape.
- Write a private Markdown daily report alongside the poster so downstream operators can
  review a text日报, not only the image artifact.
- Write a private `.character-universe.json` second-pass export alongside the poster and
  Markdown. It keeps people, topics, events, memes, callbacks, same-topic relationships,
  creative-profile candidates, creative-universe candidates, storyline candidates, and
  graph edges from curated report content only; it does not retain raw messages or
  hidden profile fact text.
- Write a `wx-cli`-style private review bundle alongside the poster: `.quote-map.json`,
  `.wiki-bundle.json`, `.draft-bundle.json`, `.run-manifest.json`, `.review.md`, and
  `.creative-profile-review-payload.draft.json`. These files add the reference project's
  quote-map / Wiki / draft / run-manifest / review structure while preserving the latest
  Postgres chat-record source of truth. The ordinary digest draft follows the reference
  template fields: weather context slot, one-sentence summary, main topics, people
  notes, local-life notes, open questions, risk items, and public-topic candidates. They
  are internal review artifacts, not public or send-ready payloads.
- Reference-project attachment/media fields are represented as explicit empty review
  slots until a reviewed attachment source exists. The workflow does not read
  `messages.raw`, attachment tokens, filenames, media URLs, or image payloads to infer
  visual content.
- Render a mobile-friendly JPEG poster from the character-daily template. The HTML
  preview and production image share the same report layout. The Pillow fallback is part
  of the production path and must keep the same storyline/character-first order, with
  local-life/open-question notes and then activity rhythm and speaker ranking after the
  character, quote, meme, relation, and storyline sections.
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

The production path runs in the Rust sidecar:

```text
run-daily-case-report-auto-publish-worker
  -> collect/analyze/narrate/render HTML in Rust
  -> rasterize HTML to JPEG through workflows/xiaoman-daily-case-report/rasterize.py
  -> upload and create the reviewed QiWe send-ready work item
```

The legacy Python pipeline remains available only as a reviewed rollback fallback via
`QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_USE_PYTHON_PIPELINE=1` until the scheduled-run
observation window closes.

`daily_case_report.py` remains for rollback and local comparison in three modes:

1. **Database mode** (production): reads from Postgres when
   `QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_READ_THROUGH_ENABLE=1` and a database URL is
   configured.
2. **Fixture mode** (`--fixture path.json`): reads pre-canned messages for tests or
   demos.
3. **Dry-run mode** (`--dry-run`): uses deterministic demo data to validate the template
   and rendering pipeline.

Automatic publication is not implemented inside `daily_case_report.py` directly. The
release worker renders the JPEG through the Rust default path, calls
`operations-daily-case-report-media-upload` to obtain a durable HTTPS URI, then calls
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
required before any durable `creative_profile` snapshot can be written. The same export
emits `creative_universe_candidates` for cross-day memes, relationship labels, and
timeline threads. These are candidate-only review assets with
`public_surface_allowed=false` and `writes_member_profile_snapshots=false`; production
metadata may retain only their counts and privacy flags. Owner-reviewed expressive
labels use a stricter field-level boundary: the daily report may generate
`expressive_label_candidates`, but public Markdown/poster copy can reuse only labels
already reviewed into `safe_reply_hints.public_expressive_labels` with
`public_surface_allowed=true` and `review_status=reviewed|approved`. Unreviewed roast
labels, relationship tension, or cross-day jokes remain private review candidates.

The default visual order is storyline-first and character-first: 今日主线, 人物出场表,
source quote, meme/callback candidates, same-topic relationships, story cards, then
activity rhythm and speaker ranking. This keeps the group-facing poster close to the
reference project's daily story style instead of the old metrics dashboard.

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
> git. Without `--date`, the query window is the most recently completed calendar day
> (00:00–24:00) in the report timezone. Use `--date YYYY-MM-DD` only for a specific
> calendar-day backfill in `--timezone`.

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
- An empty calendar-day window exits 0 with a report showing zero messages and no cases.
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
- The default report window is the most recently completed calendar day (00:00–24:00) in
  the configured `--timezone` (default `Asia/Shanghai`) before querying Postgres.
  `--date` is an explicit calendar-day backfill mode.
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
- The private quote-map, wiki-bundle, draft-bundle, run-manifest, and review report are
  generated in the same `0700` output directory with mode `0600`. They may contain
  curated excerpts, labels, candidate nodes, light-roast material, public-draft title
  candidates, ordinary digest template fields, open questions, risk items, and lookback
  callbacks, so production worker metadata may retain only their presence, counts, and
  privacy flags.
- Auto-publish metadata retains only safe counters and schema flags from the private
  Markdown/universe/review-bundle outputs. Production evidence can prove the upgraded
  character universe and private review bundle paths ran, without retaining people
  labels, story labels, quote text, relationship labels, or source excerpts.
- The auto-publish worker's Python/Pillow fallback invokes
  `daily_case_report.py --json --json-summary-only`. That JSON contains paths, artifact
  identity, `public_output_style`, `character_universe_summary`, and review-bundle
  counts/flags only; it deliberately omits full Markdown, full character-universe nodes,
  quote-map, wiki-bundle, draft-bundle, run-manifest, and operator-review text from the
  send chain.
- The visible Markdown/poster output also emits a fixed `public_output_style` contract
  so production evidence can prove the character-daily, storyline-first, image-first,
  PDF-non-default, roast-review, and private-draft boundaries without retaining rendered
  copy or candidate text. Group-facing delivery defaults to JPEG/image; any future PDF
  output is for internal archive or review, not the default send artifact.
- Production JPEG/database runs use fixed local runtime tools only. Database
  read-through prefers `psycopg` when already present and otherwise falls back to the
  reviewed `/usr/bin/psql` boundary without placing the database URL in command
  arguments, with a minimal `PATH`, `PG*` connection fields, and SQL on stdin so `psql`
  variable substitution is applied. Auto-publish uses the Rust renderer as the primary
  path. If that path fails before upload at `rasterize rendered HTML`, the worker may
  fall back to the reviewed Python/Pillow renderer; upload, publish-create, and QiWe
  send-state failures must not trigger a rerender fallback. Hand-installed Python
  packages or browsers remain outside the approved production boundary.
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
- `cargo llvm-cov nextest --manifest-path runtime/sidecar/Cargo.toml --summary-only`
  records the Rust sidecar coverage baseline that now owns the deterministic daily
  report pipeline.
- Creative-profile apply boundary test:

  ```bash
  PYTHONDONTWRITEBYTECODE=1 python3 -m unittest \
    workflows/xiaoman-daily-case-report/tests/test_build_creative_profile_review_payload.py \
    workflows/xiaoman-daily-case-report/tests/test_apply_creative_profile_candidates.py -v
  ```

  validates the review-payload draft and reviewed creative-profile apply boundaries.

- `python3 workflows/xiaoman-daily-case-report/daily_case_report.py --dry-run --render html`
  validates the template generation path without image rendering dependencies.

## Operating rules

These constraints supplement the scoped AGENTS.md summaries. Conditions and historical
exceptions remain binding. Backtick paths from root rules are repository-relative;
Sidecar rules retain their original `runtime/sidecar/` path base.

### Commands

- Xiaoman daily case-report character-universe local readiness check:
  `node tools/deploy/check-xiaoman-daily-case-report-character-universe-local.mjs`

- Xiaoman daily case-report private review bundle now includes `.draft-bundle.json`. It
  may contain ordinary digest, light-roast, public-draft, storyline timeline, and
  7/14/30-day lookback callback candidates. The ordinary digest should follow the
  reference `wx-cli` content-workshop shape: weather slot, one-sentence summary, main
  topics, people notes, local-life notes, open questions, risk items, and public-topic
  candidates. Production evidence may retain only `draft_counts` plus privacy flags,
  never the candidate text.

- Xiaoman daily case-report reference-project attachment/media slots must stay explicit
  empty review fields until a reviewed attachment source exists. Do not read
  `messages.raw`, attachment tokens, filenames, media URLs, or image payloads for daily
  digest/poster content.

- Xiaoman daily case-report production evidence may retain only the fixed
  `public_output_style` schema/boolean contract proving the character-daily layout,
  storyline-first output, image-first group delivery, PDF-non-default delivery, roast
  review boundary, and private-draft boundary. Never retain rendered Markdown, labels,
  quotes, relationship text, or candidate draft text as style evidence. Worker metadata
  must preserve negative boundaries as negative booleans: `pdf_default_delivery=false`
  and `public_surface_contains_private_draft=false` are success evidence, not failures
  to coerce to `true`.

- Xiaoman daily case-report JPEG rendering must stay storyline/character-first in both
  HTML screenshot and Pillow fallback paths. Keep `人物出场表`, `今日台词`,
  `梗和回调候选`, `同场关系`, `地点 / 本地生活线索`, `待解决问题`, and `故事线候选`
  before `24H 活跃节奏` / `发言出场榜` so production hosts without Playwright do not
  regress to a statistics-first poster. The production auto-publish worker must call the
  renderer with `--json-summary-only`; it may consume only `character_universe_summary`,
  `public_output_style`, private-review counts, artifact identity, and paths. Do not
  parse or forward full `daily_report_markdown`, full `character_universe`, quote-map,
  wiki, draft, run-manifest, or operator-review text in the send chain.

- Xiaoman daily case-report auto-publish uses the Rust renderer as the primary path, but
  the worker may fall back to the reviewed Python/Pillow pipeline only when the Rust
  path fails before upload at `rasterize rendered HTML`. Do not fallback after media
  upload, auto-publish creation, or QiWe send-state errors; those phases are idempotency
  and delivery boundaries, not safe rerender triggers.

- Xiaoman daily case-report creative-profile apply boundary test:

  ```bash
  PYTHONDONTWRITEBYTECODE=1 python3 -m unittest \
    workflows/xiaoman-daily-case-report/tests/test_build_creative_profile_review_payload.py \
    workflows/xiaoman-daily-case-report/tests/test_apply_creative_profile_candidates.py -v
  ```

- Xiaoman daily case-report production approval repair is the only reviewed one-shot
  path for restoring a missing
  `QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_PRODUCTION_APPROVAL` after Hermes
  cutover. Use `Run Production Runtime One-Shot` with
  `runtime_one_shot_target=xiaoman-daily-case-report-approval-repair` and
  `approval=approved-production-xiaoman-daily-case-report-config-v1`; it may write only
  the fixed approval constant to `/etc/qintopia/message-sidecar.env`, must fail closed
  on duplicate/wrong values, and must never accept or expose chat ids, group ids, DB
  hashes, payload JSON, env values, or arbitrary config fields.

- Xiaoman daily case-report production read-through repair is the only reviewed one-shot
  path for restoring a missing
  `QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_READ_THROUGH_ENABLE=1` after Hermes cutover. Use
  `Run Production Runtime One-Shot` with
  `runtime_one_shot_target=xiaoman-daily-case-report-read-through-repair` and
  `approval=approved-production-xiaoman-daily-case-report-config-v1`; it may write only
  the fixed read-through enable constant to `/etc/qintopia/message-sidecar.env`, must
  fail closed on duplicate/wrong values, and must never accept or expose chat ids, group
  ids, DB hashes, payload JSON, env values, or arbitrary config fields.

- Xiaoman daily case-report production chat-id repair is the only reviewed one-shot path
  for restoring a missing `QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_CHAT_ID` after Hermes
  cutover. Use `Run Production Runtime One-Shot` with
  `runtime_one_shot_target=xiaoman-daily-case-report-chat-id-repair` and
  `approval=approved-production-xiaoman-daily-case-report-config-v1`; it may write only
  the chat id copied from the fixed Xiaoman Hermes profile env `WECOM_HOME_CHANNEL` to
  `/etc/qintopia/message-sidecar.env`, must fail closed on duplicate/wrong values, and
  must never accept or expose chat ids, group ids, DB hashes, payload JSON, env values,
  or arbitrary config fields.

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

- The Xiaoman daily case-report worker uploads through the Huabaosi Feishu primary
  storage boundary. Its release SHA binding now lives in the Hermes wrapper: after
  sourcing the persistent env, the wrapper derives the release SHA from
  `release/current` and exports it as both `QINTOPIA_DEPLOYED_COMMIT_SHA` and
  `QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA`; do not rely on stale persistent env
  release keys from `/etc/qintopia/message-sidecar.env`.

- The Xiaoman daily case-report production host does not provide Python `psycopg`,
  Python Playwright, or a Playwright browser binary by default. The reviewed production
  path must keep the fixed `/usr/bin/psql` database fallback and system Pillow renderer
  available through `/usr/bin/python3`; do not replace this with runtime package
  installation or browser downloads on the server. Database fallback must use a minimal
  `PATH`, keep the database URL out of process arguments, pass connection fields through
  `PG*` environment variables only, and feed SQL on stdin so `psql` variable
  substitution is applied.

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

### Core Rules

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

- Xiaoman daily case-report auto-publish must use the reviewed AgentOS artifact plus
  QiWe image-send boundary. Do not treat a local image path, hand-copied systemd unit,
  conversation-created cron, Python QiWe sender, deprecated synchronous upload shortcut,
  caller-provided HTTPS image URL without `media_upload_evidence`, or committed group id
  as an acceptable automatic publication path.

- Xiaoman daily case-report top-line message totals stay raw, but highlights, topic
  cards, and MVP ranking must filter obvious payment prompts, copy-token promotions, and
  external-platform shopping redirects so a long spam-like message cannot become the
  day's featured quote.

- Xiaoman daily case-report colon-based topic markers must be strong labels such as
  topics, recaps, shares, asks, or activity discussions. Ordinary chatty colon sentences
  must break marker carry-over instead of capturing later messages into a fake topic
  card.

- Xiaoman daily case-report backfill must start the reviewed release-local
  `xiaoman-daily-case-report-auto-publish-backfill.sh` entrypoint with the exact owner
  approval, reviewed release SHA, and `YYYY-MM-DD` date. The worker may honor
  `QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_DATE` only when paired with the matching backfill
  approval, and the backfill script may temporarily export
  `QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED=1` only for that worker
  process; do not send missed reports by local image path, retired timer starts, or
  ad-hoc QiWe calls.

- Xiaoman daily case-report character-universe outputs are private second-pass
  artifacts. Production worker logs and send-ready metadata may retain only safe
  counters and schema flags; never retain Markdown body, raw universe nodes, member
  labels, story labels, or source excerpts.

- Xiaoman daily case-report may reuse active reviewed `creative_profile` snapshots only
  as read-only style memory keyed by stable `person_id`. The read path may use only
  `safe_reply_hints` and `communication_style` fields from
  `profile_version='xiaoman-daily-creative-profile-v1'`; never read or publish snapshot
  `summary`, raw messages, fact text, private profile text, or display-name-guessed
  identities. If this layer fails, keep generating the latest-message daily report.

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

See the
[daily cron runbook](../../docs/operations/xiaoman-daily-case-report-hermes-cron-runbook.md#operating-rules)
for scheduler migration and the
[deploy runner](../../deploy/runner/README.md#operating-rules) for cross-workflow
activation, observation and one-shot constraints.
