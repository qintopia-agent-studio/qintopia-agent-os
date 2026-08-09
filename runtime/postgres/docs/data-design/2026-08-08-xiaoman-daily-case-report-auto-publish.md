# Xiaoman Daily Case Report Auto-Publish Capability

Schema version: `2026-08-08.001`  
Migration: `migrations/202608080001_xiaoman_daily_case_report_auto_publish.sql`

## Purpose

Register a Postgres-owned AgentOS capability for Xiaoman daily case-report auto-publish.
The capability binds one durable daily JPEG artifact identity to one automatic QiWe
image-send request without asking for per-day human final confirmation.

The daily renderer remains outside the database. Postgres records only the reviewed
capability contract and the later work item/artifact facts created by the sidecar
command after the JPEG has crossed the reviewed media boundary.

## Contract

`xiaoman.daily_case_report_auto_publish` is provided by Xiaoman, may be called only by
Xiaoman, and may create only `daily_case_report_request` roots that lead to a
`generated_image` artifact plus a `group_message_request`.

The apply path must bind `window_start`, `window_end`, `artifact_uri`, `content_hash`,
`file_md5`, `byte_size`, `width`, `height`, `media_upload_evidence`, and the
runtime-provided target group id. The idempotency key is derived from the report window,
image content identity, and hashed target group, so retrying the same reviewed JPEG
cannot create duplicate sends.

## Boundary

- Postgres writes: approved `generated_image` artifact facts, one automatic
  `group_message_request`, and append-only work item events.
- Local file paths: rejected before publish request creation.
- Media boundary: the automatic publish create command revalidates the upload evidence
  against the configured public media base and allowed hosts before approving the
  artifact or recording send-ready.
- QiWe sends: never called by the daily renderer or capability command; delivery remains
  delegated to the reviewed QiWe image-send adapter.
- Human confirmation: not required per day after production activation, but the release
  timer and runtime configuration remain owner-approved gates.
- Secrets and group ids: runtime-only; no committed target group id or database URL.

Rollback can disable the release-managed daily timer. The additive capability row may
remain; older workers do not call it.
