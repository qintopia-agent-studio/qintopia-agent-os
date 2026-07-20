# Xiaoman Text Announcement MVP

Updated: 2026-07-20

## Goal

Ship the smallest Xiaoman community-operations loop that Liu Shan can test without
waiting for poster generation or QiWe production image delivery.

## MVP Scope

- Read or receive sanitized activity records for one local date.
- Prepare a text-only activity announcement draft for operations review.
- Flag missing time, location, owner, and post-event material fields.
- Skip temporary meal records by default while keeping paid planned activities in the
  scheduling pool.
- Produce an Erhua handoff draft only as text that still requires human confirmation.

## Out Of Scope

- No production timer.
- No Feishu writeback.
- No Huabaosi provider call or poster generation.
- No Erhua command execution.
- No QiWe call, group delivery, publish, or send-ready mutation.
- No 24/48/72-hour escalation automation in this MVP.

## Runtime Boundary

The MVP is exposed through `qintopia_xiaoman_activity_announcement_prepare` in the
Xiaoman `qintopia-tools` variant. The tool may compose records already returned by
`qintopia_xiaoman_activity_list_by_date`, or read through only when the existing
read-only Xiaoman activity read-through boundary is explicitly enabled.

The output is safe for operations review, not member chat delivery. A human must confirm
the text before any Erhua handoff or group send is considered.

## Acceptance

- The tool returns one operations review message, one Erhua handoff draft, missing-field
  reminders, and explicit `external_send_executed=false`.
- Temporary meal records are skipped by default.
- Paid planned activities are preserved in the announcement draft.
- Unit tests cover the text-only boundary and missing-record behavior.
