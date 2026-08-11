# Xiaoman Activity Skill

This package owns the Agent-facing Xiaoman activity tools that were previously buried
inside `skills/qintopia-tools`.

Hermes still loads stable tool names through the Xiaoman `qintopia-tools` profile
variant until profile bundle repoint is reviewed. This package provides the dedicated
capability boundary first and uses a legacy bridge to preserve byte-for-byte behavior
while the implementation is moved out of the broad registration shell.

## Capability

- read sanitized Xiaoman activity records by date or record reference;
- prepare text announcements, weekly preview drafts, and weekly poster workflow intake;
- prepare reviewed text group-message requests without sending;
- draft promotion review material and material summaries;
- prepare bounded activity field-update, status, gap, phase, and handoff commands.

## Tools

- `qintopia_xiaoman_activity_record_get`
- `qintopia_xiaoman_activity_list_by_date`
- `qintopia_xiaoman_activity_plan_table_probe`
- `qintopia_xiaoman_activity_announcement_prepare`
- `qintopia_xiaoman_activity_text_group_message_request_prepare`
- `qintopia_xiaoman_weekly_poster_workflow_prepare`
- `qintopia_xiaoman_public_reply_rewrite`
- `qintopia_xiaoman_activity_status_update`
- `qintopia_xiaoman_activity_gap_update`
- `qintopia_xiaoman_activity_phase_update`
- `qintopia_xiaoman_activity_feishu_field_update`
- `qintopia_xiaoman_activity_handoff_create`
- `qintopia_xiaoman_activity_promotion_review_draft`
- `qintopia_xiaoman_activity_material_summary`

## Runtime Boundary

- The skill may call release-managed read-through and worker command boundaries.
- Feishu writes are limited to the reviewed field-update worker command and never expose
  a generic write primitive.
- Text/image sending is not performed here. The skill may prepare approved work-item
  payloads; QiWe delivery remains behind the separate send-ready and adapter chains.
- No `.env`, profile sessions, raw Feishu/QiWe payloads, raw private chats, or live
  Hermes runtime state belong in this package.

## Legacy Bridge

The current implementation is loaded from
`skills/qintopia-tools/variants/xiaoman/__init__.py` through a narrow bridge. This
avoids two active implementations while keeping a dedicated package target for future
edits.

Do not add new Xiaoman activity behavior to `qintopia-tools`; route it here, then move
the legacy implementation behind this package in small, tested slices.

## Validation

```bash
pnpm skills:xiaoman-activity:check
pnpm skills:qintopia-tools:check
pnpm registry:check
```
