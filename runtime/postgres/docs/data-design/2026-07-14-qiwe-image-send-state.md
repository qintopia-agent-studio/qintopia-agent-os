# QiWe Image Send State

Date: 2026-07-14

## Purpose

Add a durable AgentOS state machine between a reviewed Xiaoman `group_message_request`
and the QiWe asynchronous image upload callback. The state is owned by Postgres and must
remain useful across worker restarts without persisting QiWe file credentials or raw
callback payloads.

## State Model

Each external upload attempt is represented by one
`qintopia_agent_os.qiwe_image_send_attempts` row:

```text
awaiting_callback
  -> sending
  -> sent
  -> failed | ambiguous

awaiting_callback -> expired
```

- `awaiting_callback` means QiWe accepted one asynchronous URL upload and AgentOS
  durably recorded its hashed request id.
- `sending` is committed before `/msg/sendImage` is called. It is an at-most-once gate,
  not proof that QiWe received the send request.
- `sent` requires an explicit successful QiWe response and completes the AgentOS work
  item.
- `failed` is a known terminal denial or provider failure before a successful send.
- `ambiguous` means AgentOS cannot prove whether QiWe sent the image. It requires human
  reconciliation, records `external_send_executed=null` with outcome `unknown`, and must
  never be retried automatically.
- `expired` means the callback did not arrive while the original claim was current. A
  later reviewed retry creates a new attempt and request correlation.

Only one active attempt and one successful attempt may exist for a work item. Attempt
numbers remain unique so failed/expired history is retained.

## Correlation And Credential Boundary

The table stores only canonical `sha256:<64 lowercase hex>` values for:

- QiWe upload `requestId`;
- callback payload bytes;
- target group id;
- generated-image URI; and
- the final QiWe message identifier.

The approved artifact UUID and canonical image content hash remain internal AgentOS
facts. The raw request id is held only long enough to hash the upload acceptance. The
callback's `fileAesKey`, `fileId`, `fileMd5`, filename, URL, and any unknown fields are
never inserted into Postgres, work-item events, logs, or reports.

A dedicated callback handler may parse complete credentials in memory and use them for
one `/msg/sendImage` request after the database transition to `sending` commits. If the
process stops after that commit, the attempt is `ambiguous`; credentials are not
retained for a retry. Retrying an ambiguous attempt could duplicate an external send and
is forbidden.

## Claim And Immutability Checks

Starting an attempt requires all of the following in one reviewed flow:

- a queued `xiaoman -> erhua` `group_message_request`;
- `human_final_confirmation` with a recorded confirmed decision;
- an existing `group_message_send_ready_recorded` event;
- an approved `generated_image` with immutable JPEG URI and content hash; and
- no active or successful QiWe image-send attempt.

QiWe target group ids are opaque, case-sensitive values. The configured group allowlist
must match the complete id exactly; lowercase normalization is not permitted at the
external-send boundary.

The external worker writes a unique claim token to the work item. Recording upload
acceptance, claiming a callback, and finalizing the send each lock the work item and
attempt, then require the same unexpired token. They also recheck the approved artifact,
target group hash, and final-confirmation/send-ready evidence before crossing the next
boundary.

## Idempotency

- `request_id_sha256` is globally unique.
- `callback_payload_sha256` is globally unique when present.
- duplicate delivery of the same callback returns the existing attempt state and never
  opens a second send gate.
- a different callback for a request already in `sending` or `sent` is rejected.
- one partial unique index permits only one `awaiting_callback` or `sending` attempt per
  work item.
- one partial unique index permits only one `sent` attempt per work item.

## Production Boundary

This migration is additive. It does not call QiWe, persist callback credentials, enable
an adapter, install a timer, change Feishu, or send a message. Rollback keeps the table
unused and the QiWe image-send enable flag at `0`; historical attempt rows must be
retained for audit rather than deleted.
