# 2026-07-31 Xiaoman Poster Delivery Claim CI Failure

## Scope

PR #334 adds the disabled-by-default Xiaoman asynchronous poster return path. Its first
CI run passed the repository and Rust quality jobs but failed the disposable PostgreSQL
integration job before any external adapter could run.

This is repository remediation only. No production database, Feishu application,
service, timer, image provider, or group-send path was activated.

## Evidence

The failed job reported PostgreSQL error `42703`: the integration fixture tried to set
`completed_at` on `qintopia_agent_os.work_items`, whose canonical schema has no such
column.

Independent review also identified two delivery-claim defects:

- `FOR UPDATE OF notification, item, artifact` attempted to lock `artifact`, the
  nullable side of a `LEFT JOIN`, which PostgreSQL rejects.
- allowlist or artifact-identity rejection returned an error and rolled the claim
  transaction back, leaving the oldest notification pending and repeatedly blocking
  later eligible work.

## Root Cause

The implementation mixed attempt-table completion fields into the work-item fixture and
treated permanent pre-I/O policy rejection as an exceptional rollback. The claim query
also requested a row lock that is not legal for an outer-join nullable relation.

## Resolution

- Lock only the notification and work-item rows needed to establish the claim.
- Convert permanent target or artifact rejection into one transaction that marks the
  notification and work item failed, clears the complete claim tuple, and appends a
  sanitized `conversation_notification_failed` event.
- Do not create a delivery attempt or open any external connection for that rejection.
- Return a reportable rejected outcome so the next worker run can select the next
  eligible notification.
- Remove `work_items.completed_at` from production and integration completion updates.

## Validation

Local validation passed:

- six focused poster-delivery unit tests;
- the default sidecar suite with 452 passing tests;
- the all-feature sidecar suite with 458 passing and 14 intentionally ignored tests;
- compilation of the PostgreSQL integration target;
- warning-denied Clippy with no default features, the Xiaoman Feishu adapter feature,
  and all features;
- `cargo fmt`, `git diff --check`, and `pnpm check:pr:auto`.

The guarded PostgreSQL regression proves that a rejected oldest notification is
terminal, creates no attempt, and does not block the next valid claim. It compiled
locally but could not execute because no disposable `qintopia_test` database was
available on loopback. Replacement PR CI owns that execution before merge.

## Remaining Boundary

The real Feishu adapter remains disabled and requires separate owner-reviewed
credentials, allowlists, callback permissions, release/database bindings, activation,
and end-to-end direct-message acceptance. An approved image still cannot authorize or
create a group send.
