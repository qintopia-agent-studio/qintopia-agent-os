# Erhua Morning Brief Card Publish Capability

Schema version: `2026-08-19.001`  
Migration: `migrations/202608190001_erhua_morning_brief_card_publish_capability.sql`

## Purpose

Register the Postgres-owned AgentOS capability `erhua.morning_brief_card_publish`. The
capability backs the `operations-erhua-morning-brief-card-publish-create` sidecar
command: it binds one rendered Erhua morning-brief card JPEG to one automatic QiWe
image-send work item without per-day human final confirmation.

This capability row was shipped with the card-send feature (#648) but never seeded into
Postgres, so the first work item creation using it failed with a
`work_items_capability_key_fkey` foreign-key violation and the worker degraded the card
delivery back to the text brief. This migration is the additive, idempotent seed that
closes that gap; it does not change any existing row.

## Contract

`erhua.morning_brief_card_publish` is provided by Erhua, may be called only by Xiaoman,
and may create only `morning_brief_card_request` roots that lead to a `generated_image`
artifact plus a `group_message_request` (delivered through the reviewed
`erhua.send_group_message` capability and the QiWe image-send adapter).

The apply path must bind `brief_date`, `artifact_uri`, `content_hash`, `file_md5`,
`byte_size`, `width`, `height`, `media_upload_evidence`, and the runtime-provided target
group id. `message_text` is a short caption capped at 500 characters — the full brief
lives inside the rendered card image, not in the work item text.

## Boundary

- Postgres writes: one completed `morning_brief_card_request` source work item, one
  approved `generated_image` artifact, one automatic `group_message_request`, and
  append-only work item events, all in a single transaction.
- Local file paths: rejected before publish request creation; the JPEG must have crossed
  the reviewed media-upload boundary first.
- Media boundary: the command revalidates the upload evidence (workflow type, content
  hash, file MD5, byte size, mime type, dimensions) against the reviewed Feishu primary
  storage evidence before approving the artifact.
- QiWe sends: never called by the worker or capability command; delivery remains
  delegated to the reviewed QiWe image-send adapter.
- Human confirmation: not required per day after production activation; the Hermes cron
  (08:10) and runtime configuration remain owner-approved gates.
- Secrets and group ids: runtime-only; no committed target group id or database URL.
