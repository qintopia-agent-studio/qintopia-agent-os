# Workflow: Xiaoman Wx-Cli Style Daily Report

`workflows/xiaoman-daily-case-report` generates a wx-cli inspired Xiaoman daily report.
The group-facing deliverable is a PNG/JPEG-style long image, while the richer
digest/roast/public-draft material is retained only in a private review bundle.

## Responsibility

- Read the latest rolling 24 hours of QiWe group messages from
  `qintopia_messages.messages`.
- Group today's discussion into storyline-first chapters, not old "case file" cards.
- Build character sketches by stable `sender_person_id` first, falling back to
  sender/channel identity only when no person id exists.
- Fold long-term `member_facts` into private creative-profile candidates and expose only
  safe counts/approved public labels on the image.
- Render a mobile-friendly long image in the reference project's editorial daily style.
- Write a private `.draft-bundle.json` containing digest, roast draft, public draft,
  quote map, profile candidates, privacy flags, and draft counts.
- Never send, publish, or write to external systems. Upload and group-send remain a
  separate reviewed chain.

## Shape

The generator now mirrors the reference project's daily pipeline shape:

- **digest**: fact-oriented internal daily notes.
- **roast**: storyline-first narrative draft with character sketches and callbacks.
- **public draft**: human-review candidate for public reuse.
- **quote map**: bounded daily quote candidates.
- **profile candidates**: `profiles-roast`-style creative labels with evidence counts.
- **image output**: the default group-facing artifact.

PDF can be produced later as an archive/review format, but it is not the default group
deliverable because group members should not need to open a separate document.

## Running It

### Dry-run preview

```bash
python workflows/xiaoman-daily-case-report/daily_case_report.py \
  --dry-run \
  --render html \
  --output-dir /private/tmp/xiaoman-daily-report-preview
```

The output directory must be private `0700`.

### Production read-through

```bash
export QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_READ_THROUGH_ENABLE=1
export QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_CHAT_ID="<reviewed-qiwe-group-chat-id>"
export QINTOPIA_MESSAGE_STORE_DATABASE_URL="postgresql://..."

python workflows/xiaoman-daily-case-report/daily_case_report.py \
  --group-name "秦托邦的小伙伴（新）"
```

Production read-through requires `--chat-id` or
`QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_CHAT_ID`; do not commit real QiWe group IDs. Without
`--date`, the query window is the latest rolling 24 hours ending at run time.

## Production Boundary

- Production HTML is an intermediate render surface only. `--render html` and
  `--keep-html` are rejected in production because HTML can contain real group content.
- The private draft bundle is `0600` under a `0700` directory and is for human review or
  approved internal archival only.
- Production evidence may retain `draft_counts`, `privacy_flags`, and the fixed
  `public_output_style` booleans. It must not retain rendered Markdown, quote text,
  relationship text, labels from private facts, raw messages, person ids, chat ids, or
  media URIs.
- Database reads use `psycopg` when available, with a reviewed `/usr/bin/psql` fallback
  for production hosts that do not provide Python `psycopg`.
- Rendering stays local. Playwright can render the HTML preview when packaged, but the
  production-safe fallback is a Pillow long-image renderer. Do not download browsers or
  Python packages on the production host.
- No remote fonts, image services, Feishu, QiWe, or network resources are called by this
  script.

## Acceptance Scenarios

- `--dry-run --render html` exits 0, writes retained HTML plus a private
  `.draft-bundle.json`, and returns `public_output_style.image_first_delivery=true`.
- `--dry-run --render png` writes a group-facing image when either Playwright or Pillow
  is available locally; if neither renderer is available, it fails closed.
- Production read-through rejects `--render html` and `--keep-html` before reading the
  database.
- Character sketches remain separated by stable `sender_person_id` even when display
  names collide.
- Private `member_facts` labels never appear directly on the public image; only safe
  labels or private candidate counts may appear.
- The workflow never calls Feishu, QiWe, an image provider, or any external sender.

## Validation

```bash
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover \
  -s workflows/xiaoman-daily-case-report/tests -v
```

```bash
node tools/deploy/check-xiaoman-daily-case-report-character-universe-local.mjs
```
