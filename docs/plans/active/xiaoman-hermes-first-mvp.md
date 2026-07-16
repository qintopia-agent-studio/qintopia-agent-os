# Xiaoman Hermes-First MVP

Updated: 2026-07-16

## Goal

Make Xiaoman visibly useful before broadening production infrastructure. Hermes should
read the current activity context, summarize the opportunity, and propose the next
human-reviewed operation. Code should provide thin, hard boundaries for data access and
external effects instead of owning the whole business judgment path.

## First Thin Tool Boundary

The first MVP change keeps the runtime thin while making the boss-visible loop usable:

1. `qintopia_xiaoman_activity_list_by_date` can optionally read through for controlled
   read-only activity queries:

- default behavior remains unchanged and returns the bounded sidecar command;
- `QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE=1` lets read-only, non-dry-run
  operations execute the configured sidecar worker locally;
- the wrapper returns only the sidecar's sanitized report fields, including
  `record_count`, `records`, and `summaries`;
- write operations still return commands and continue to default to dry-run.

1. `qintopia_xiaoman_activity_promotion_brief_generate` turns already-read sanitized
   records into a human-reviewable activity summary, promotion judgment, group-message
   draft, and poster brief:

- it does not read Feishu, write Postgres, call Huabaosi, publish, or send;
- promotable records return a dry-run `qintopia_xiaoman_activity_handoff_create` payload
  as the next controlled action;
- incomplete records stop at human review with missing fields instead of inventing
  activity facts.

This gives Hermes actual activity records when the runtime is explicitly configured,
then gives Xiaoman a small deterministic planning step before any external effect.
Feishu table access remains inside the reviewed sidecar allowlists.

## MVP Non-Goals

This step does not enable image generation, Feishu mirror writes, QiWe upload/callback,
QiWe production sending, timers, production activation, or automatic publication. It
also does not expose raw Base APIs, raw record ids, secrets, arbitrary SQL, or generic
shell execution.

## Production Boundary

The MVP read-through path is disabled by default. Production use still requires the
existing Xiaoman wrapper enable flag, the Xiaoman profile, a configured sidecar binary,
and either fixture replay or the reviewed Feishu Base read configuration. The brief
generation step can only use sanitized records already returned by the read tools.
Read-through plus brief generation is not evidence that the full production
activity-to-group-send workflow is complete.
