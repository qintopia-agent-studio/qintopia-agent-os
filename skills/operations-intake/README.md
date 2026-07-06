# Operations Intake Skill

This package owns controlled Agent OS intake and handoff tools that were previously
implemented inside `skills/qintopia-tools`.

Hermes still loads stable tool names through the `skills/qintopia-tools` registration
shell. Change complaint intake, sales handoff, proposal/demo draft, disclosure
filtering, and conversation summary behavior here, not in the qintopia-tools variants.

## Capability

- create controlled complaint intake action requests for Erhua and default dispatch;
- append complaint details and prepare approved private follow-up action requests;
- capture Xiaoqin-style customer demand as controlled sales, demo, proposal, or
  disclosure-review handoff;
- generate safe proposal and demo drafts that require human review before external send;
- filter external drafts through disclosure filtering for sensitive categories;
- summarize customer conversations into handoff text.

## Tools

- `qintopia_complaint_intake_create`
- `qintopia_complaint_intake_update`
- `qintopia_complaint_followup_send`
- `qintopia_external_product_kb_search`
- `qintopia_public_case_search`
- `qintopia_customer_context_lookup`
- `qintopia_lead_capture`
- `qintopia_proposal_outline_generate`
- `qintopia_demo_script_generate`
- `qintopia_external_disclosure_filter`
- `qintopia_conversation_summary`

## Not Owned Here

- QiWe webhook parsing and actual message sending stay in `skills/qiwe`.
- Public JSONL knowledge search still stays in `skills/qintopia-tools` for now and is
  injected as a read-only callback by the registration shell.
- Hermes Kanban is a legacy runtime surface. This package may prepare or dry-run action
  requests, but new orchestration should move toward Agent OS workflow/runtime packages.
- This package does not own WorkTool. Future Xiaoqin work must use a non-WorkTool
  design.

## Runtime Boundary

- This package may return `qiwe_send_direct_message` action requests for complaint
  private detail collection and approved follow-up.
- The package does not directly send external messages. The QiWe/channel adapter or
  controlled executor must enforce approval state, recipient allowlist, purpose,
  idempotency, and audit before any message is sent.
- Hermes Kanban writes are optional. When Hermes Kanban is unavailable, handlers return
  `dry_run_no_hermes_kanban_runtime`.
- No secrets, `.env`, runtime sessions, raw private chats, or live Hermes state belong
  in this package.
- External customer-facing text must use the returned public-safe message fields, not
  internal notes or action details.

## Validation

```bash
pnpm skills:operations-intake:check
pnpm skills:qintopia-tools:check
pnpm check:light
```
