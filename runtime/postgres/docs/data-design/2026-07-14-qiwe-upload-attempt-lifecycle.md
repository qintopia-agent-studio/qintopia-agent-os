# QiWe Upload Attempt Lifecycle

Date: 2026-07-14

## Problem

The original upload worker created `qiwe_image_send_attempts` only after QiWe returned a
successful asynchronous upload response. A process or database failure after QiWe
accepted the request but before the request-id hash committed left only a work-item
claim. Expiring that claim back to `queued` could issue a duplicate upload and leave an
older callback without durable correlation.

## State Change

The upload lifecycle now starts inside the work-item claim transaction:

```text
uploading
  -> awaiting_callback
  -> failed | ambiguous

awaiting_callback -> sending -> sent | failed | ambiguous
awaiting_callback -> expired
```

`uploading` contains the immutable artifact, target, attempt number, and unique claim
token hashes before any socket can open. Its `request_id_sha256` is null because QiWe
has not returned a correlation id yet. A successful response atomically changes the same
row to `awaiting_callback` and stores the canonical request-id hash.

Known pre-acceptance rejection moves the attempt to `failed`. Any transport uncertainty,
state-persistence uncertainty, worker crash, or expired `uploading` claim moves it to
`ambiguous`, fails the work item, and records `automatic_retry_allowed=false`. AgentOS
must not retry an upload when it cannot prove that the previous request stayed local.

## Compatibility

The migration makes `request_id_sha256` nullable only for states that may not have a
provider correlation. `awaiting_callback`, `sending`, `sent`, and `expired` continue to
require a canonical request-id hash. The one-active-attempt index now includes
`uploading`, so concurrent or recovered workers cannot create another active request.

A stale legacy claim with no attempt row is also terminalized as an unknown upload. The
legacy state cannot prove whether old code crossed the external boundary, so requeueing
would be unsafe.

## Data Boundary

No raw request id, group id, media URI, callback credential, response body, or provider
message id is added. Audit events contain only AgentOS UUIDs, canonical hashes, fixed
status values, and booleans. This migration does not compile or enable the staging
adapter, contact QiWe, install a timer, write Feishu, or send a message.

## Rollback

Keep the additive column/constraint shape and disable the staging adapter. Do not change
an `ambiguous` upload back to `queued`; reconciliation requires an owner decision
because the external upload may already exist.
