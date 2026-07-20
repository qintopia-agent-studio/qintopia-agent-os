# Xiaoman Text Announcement MVP

Updated: 2026-07-20

## Goal

Ship the smallest Xiaoman community-operations loop that Liu Shan can test without
waiting for poster generation or QiWe production image delivery.

## MVP Scope

- Read or receive sanitized activity records for one local date.
- Prepare a text-only activity announcement draft for operations review.
- Flag missing time, location, owner, and post-event material fields.
- Draft 24/48/72-hour post-event material refill reminders from sanitized records.
- Mark third-miss material follow-up as an operations-lead escalation draft only.
- Skip temporary meal records by default while keeping paid planned activities in the
  scheduling pool.
- Produce an Erhua handoff draft only as text that still requires human confirmation.
- After the text is retained as an approved text announcement artifact, prepare a
  controlled Erhua `group_message_request` command for final confirmation.

## Out Of Scope

- No production timer.
- No Feishu writeback.
- No Huabaosi provider call or poster generation.
- No Erhua command execution.
- No QiWe call, group delivery, publish, or send-ready mutation.
- No text group-message request without an approved text announcement artifact.
- No 24/48/72-hour timer, automatic escalation, omission mutation, or staff writeback.

## Runtime Boundary

The MVP is exposed through `qintopia_xiaoman_activity_announcement_prepare` in the
Xiaoman `qintopia-tools` variant. The tool may compose records already returned by
`qintopia_xiaoman_activity_list_by_date`, or read through only when the existing
read-only Xiaoman activity read-through boundary is explicitly enabled.

The output is safe for operations review, not member chat delivery. A human must confirm
the text before any Erhua handoff or group send is considered.

`qintopia_xiaoman_activity_text_group_message_request_prepare` covers only the next
bounded handoff after that review: it prepares an `operations-create` command for an
`erhua.send_group_message` / `group_message_request` work item from an approved text
announcement artifact. The generated request binds both the artifact id and the approved
text content hash so the message body cannot be swapped after approval. The request
remains `awaiting_publish`; a separate human final confirmation is still required before
queueing, send-ready, QiWe, or external delivery.

For post-event material follow-up, callers may pass `post_event_elapsed_hours` or an
explicit `material_followup_attempt`. The tool can draft the first, second, and third
material reminders, and the third reminder is only a suggested operations-lead
escalation. It does not mark work omissions or update activity records.

## Acceptance

- The tool returns one operations review message, one Erhua handoff draft, missing-field
  reminders, and explicit `external_send_executed=false`.
- The text group-message request wrapper requires `approved_artifact_id`, binds the
  approved text content hash, emits an `operations-create` command only, and retains
  `external_send_executed=false`.
- Temporary meal records are skipped by default.
- Paid planned activities are preserved in the announcement draft.
- Post-event material follow-up returns first, second, and third reminder drafts; the
  third draft is explicitly marked as an operations-lead escalation candidate.
- Unit tests cover the text-only boundary and missing-record behavior.
