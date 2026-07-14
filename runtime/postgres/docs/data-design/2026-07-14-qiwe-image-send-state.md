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
  callback arriving after that TTL atomically closes the old attempt, stores only its
  payload hash, and requeues the work item so a new attempt gets a new request
  correlation. If the callback never arrives, the next claim scan performs the same
  expiration and requeue without inventing a callback hash.

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
facts. The attempt also snapshots the approved final JPEG's canonical MD5 and positive
byte size so the callback can be matched to the exact reviewed file. These values are
computed from Huabaosi's final JPEG before review; they are not copied from the
callback. The raw request id is held only long enough to hash the upload acceptance. The
callback's `fileAesKey`, `fileId`, raw payload, filename, URL, and unknown fields are
never inserted into Postgres, work-item events, logs, or reports. Its `fileMd5` and
`fileSize` are compared in memory with the snapshotted artifact identity before the send
gate opens.

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

Read-only preview uses the same artifact and current target-group/media-host allowlist
validator as the claim transaction. It may omit locking and mutation, but it must not
weaken policy and report a work item that apply would reject.

QiWe target group ids are opaque, case-sensitive values. The configured group allowlist
must match the complete id exactly; lowercase normalization is not permitted at the
external-send boundary.

The external worker writes a unique claim token to the work item. Recording upload
acceptance and claiming a callback require that same unexpired token and recheck the
approved artifact, target group hash, final-confirmation/send-ready evidence, and final
JPEG filename, MD5, and byte size before crossing the next boundary. A mismatched
callback leaves the attempt in `awaiting_callback` and sends nothing. Finalizing a send
locks the work item and attempt and still requires the exact token that opened the send
gate.

The unexpired-token requirement applies through the callback's transition to `sending`.
After that transition commits, an external request may outlive the short send TTL.
Recording `sent`, `failed`, or `ambiguous` still requires the exact locked attempt,
processing work item, artifact, and claim token, but does not reject that terminal write
solely because wall-clock TTL elapsed. This preserves ownership while ensuring the
external outcome converges.

The timeout scan applies only to `awaiting_callback`. It must never automatically expire
or retry `sending`, because the external send may already have occurred and requires a
terminal response or `ambiguous` human reconciliation.

Once `/msg/sendImage` may have left the process, a non-2xx response or a business
response without explicit success is also `ambiguous`. It cannot be recorded as a
definite rejection or `external_send_executed=false` unless a separately reviewed,
documented failure-code allowlist proves that QiWe did not send the image.

An upload-worker crash before request correlation is persisted leaves no attempt row.
The next claim transaction may requeue that expired worker claim only when no attempt
uses the same claim token. If an HTTP upload call returns a known rejection or an
unknown outcome before correlation can be stored, the worker fails the work item with a
fixed sanitized code; a later callback cannot open a send gate without a persisted
request-id hash.

A `sending` attempt whose short claim TTL expires is never requeued. The next guarded
claim scan atomically records `ambiguous`, clears the processing claim, and requires
human reconciliation. This covers callback-worker crashes after the send gate without
risking an automatic duplicate send.

## Executable Adapter Boundary

`run-qiwe-image-send-worker` performs one guarded asynchronous upload and
`process-qiwe-image-send-callback` reads one bounded callback from stdin. Both remain
disabled and unscheduled in production. They use a shared bounded Rust HTTP client and
persist only through the state transitions in this document. Raw callback credentials,
request ids, target group ids, media URLs, response bodies, and provider message ids are
excluded from reports and are zeroized from owned in-memory buffers on drop.

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
