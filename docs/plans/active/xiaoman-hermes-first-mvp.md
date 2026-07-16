# Xiaoman Hermes-First MVP

Updated: 2026-07-16

## Goal

Make Xiaoman visibly useful before broadening production infrastructure. Hermes should
read the current activity context, summarize the opportunity, and propose the next
human-reviewed operation. Code should provide thin, hard boundaries for data access and
external effects instead of owning the whole business judgment path.

## First Thin Tool Boundary

The first MVP change makes `qintopia_xiaoman_activity_list_by_date` optionally
read-through for controlled read-only activity queries:

- default behavior remains unchanged and returns the bounded sidecar command;
- `QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE=1` lets read-only, non-dry-run
  operations execute the configured sidecar worker locally;
- the wrapper returns only the sidecar's sanitized report fields, including
  `record_count`, `records`, and `summaries`;
- write operations still return commands and continue to default to dry-run.

This gives Hermes actual activity records when the runtime is explicitly configured,
while keeping Feishu table access inside the reviewed sidecar allowlists.

## MVP Non-Goals

This step does not enable image generation, Feishu mirror writes, QiWe upload/callback,
QiWe production sending, timers, production activation, or automatic publication. It
also does not expose raw Base APIs, raw record ids, secrets, arbitrary SQL, or generic
shell execution.

## Production Boundary

The MVP read-through path is disabled by default. Production use still requires the
existing Xiaoman wrapper enable flag, the Xiaoman profile, a configured sidecar binary,
and either fixture replay or the reviewed Feishu Base read configuration. Read-through
for a query is not evidence that the full production activity-to-group-send workflow is
complete.
