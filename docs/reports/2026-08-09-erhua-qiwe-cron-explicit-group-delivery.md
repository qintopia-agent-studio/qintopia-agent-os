# Erhua QiWe Cron Explicit Group Delivery

Date: 2026-08-09

## Current State

Erhua production accepted several user-created cron jobs intended to deliver into a QiWe
group. The jobs executed, but the group did not receive the scheduled messages.

## Evidence

Read-only production journal inspection of `hermes-gateway-erhua.service` showed four
recent cron executions with the same delivery failure shape:

- The stored job origin had a `thread_id`.
- The resolved delivery target lost that `thread_id`.
- The delivery target was an explicit `qiwe:<id>` value.
- The live adapter then attempted direct-recipient delivery and failed contact lookup:
  `QiWe direct recipient was not found in external contacts`.

The live Erhua cron queue was empty after the one-shot jobs completed, so the failure
was not a stuck pending queue.

## Root Cause

Direct Erhua replies and scheduled explicit QiWe delivery do not use the same preserved
context. Direct group replies carry group metadata into the QiWe adapter. A scheduled
job with a bare `deliver=qiwe:<id>` can reach the adapter without `conversation_type` or
`chat_type`, so the adapter cannot distinguish an explicit group room id from a direct
recipient id and correctly applies the direct-recipient contact guard.

## Resolution

`skills/qiwe/adapter.py` now proves group semantics before direct contact guard when
metadata is absent.

First, explicitly configured QiWe group targets are treated as group sends. The
configured group set is intentionally narrow:

- `QIWE_HOME_GROUP`
- `QIWE_PASSIVE_ALLOWED_GROUPS`
- `QIWE_PASSIVE_ACK_ALLOWED_GROUPS`
- `QIWE_ACTIVITY_REMINDER_ALLOWED_GROUPS`
- keys from `QIWE_HUMAN_HANDOFF_GROUPS_JSON`

Second, untyped bare `qiwe:<id>` targets are checked against QiWe room detail before
direct-recipient contact guard. If `/room/batchGetRoomDetail` confirms the id is a room,
the adapter uses the group send shape. If the room check fails or returns no room, the
existing direct-recipient contact guard still runs.

## Validation

Ran:

```bash
cd skills/qiwe
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest tests/test_parser.py -v
```

Result: 163 tests passed.

## Remaining Boundary

This is a QiWe adapter-side guardrail. A broader Hermes cron runtime fix may still be
needed so explicit QiWe group delivery carries typed delivery metadata end to end
instead of relying on adapter-side room proof.
