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

Replacement CI then executed the new poster-delivery regression successfully, but its
downstream apply smoke exposed a separate query syntax error: the group-send
authorization guard used PostgreSQL keyword `authorization` as a table alias.

Follow-up PR review also found that the shared operations source policy accepted any
non-empty `source_message_ref` for trusted Feishu direct requests. That was weaker than
the intake contract and could persist an unredacted platform identifier in work-item
source references.

The same review cycle found two fail-closed gaps: fact-gate eligibility used
`jsonb_array_length` after a separate type predicate, which PostgreSQL is not required
to evaluate first, and group-send eligibility recognized direct intake only through a
metadata marker rather than also binding the trusted direct source type.

The next review found that the claim-side fact gate still did not bind all of the Rust
authorization provenance checks. A high-priority non-direct work item could therefore
pass SQL selection, fail the Rust authorization check, roll back, and repeatedly block
later valid work.

The first replacement run for that regression failed before exercising the claim query:
the new fixture used integer priorities even though `work_items.priority` accepts only
the reviewed `low`, `normal`, `high`, and `urgent` values.

The following review found that the direct-conversation group-send guard still treated
an unbound `group_send_authorized` event on the poster workflow root as sufficient. No
reviewed writer exists for that event, and reusing a poster-generation root would blur
the required boundary between generation authorization and a later publish instruction.

## Root Cause

The implementation mixed attempt-table completion fields into the work-item fixture and
treated permanent pre-I/O policy rejection as an exceptional rollback. The claim query
also requested a row lock that is not legal for an outer-join nullable relation. The
downstream group-send guard had been covered by repository-local tests without executing
its SQL against PostgreSQL, so the reserved alias reached the disposable database smoke.

## Resolution

- Lock only the notification and work-item rows needed to establish the claim.
- Convert permanent target or artifact rejection into one transaction that marks the
  notification and work item failed, clears the complete claim tuple, and appends a
  sanitized `conversation_notification_failed` event.
- Do not create a delivery attempt or open any external connection for that rejection.
- Return a reportable rejected outcome so the next worker run can select the next
  eligible notification.
- Remove `work_items.completed_at` from production and integration completion updates.
- Rename the downstream authorization-event alias so the existing apply smoke can
  execute the direct-message group-send guard against PostgreSQL.
- Require direct-request and direct-revision `source_message_ref` values to use the
  canonical lowercase `sha256:` plus 64 hexadecimal characters form before persistence.
- Apply the same lowercase canonical hash rule when collaboration authorization and
  poster-notification workers revalidate opaque references downstream.
- Add negative coverage for raw, short, uppercase, and whitespace-padded references.
- Compare fact-gate missing/conflict fields directly with the exact empty JSON array,
  avoiding type-dependent function evaluation on malformed payloads.
- Require a `group_send_authorized` event whenever either the source type or the intake
  metadata identifies a trusted Feishu direct conversation.
- Let the collaboration worker claim only work with no generation authorization or a
  complete trusted-direct authorization whose source, actor, conversation, fact gate,
  and no-group-send provenance match the Rust validation exactly.
- Add a PostgreSQL queue-order regression proving that a higher-priority forged
  authorization cannot starve a lower-priority valid direct request.
- Keep that fixture inside the production schema contract by ordering the forged and
  valid rows with `urgent` and `high`, respectively.
- Exclude direct and direct-revision poster roots unconditionally from the automatic
  group-message starter. A later explicit publish instruction must create a separate,
  target-bound group-message request through the existing operations path.
- Exercise the PostgreSQL smoke with a forged `group_send_authorized` event and prove it
  still creates no group-message child.

## Validation

Local validation passed:

- six focused poster-delivery unit tests;
- the default sidecar suite with 452 passing tests;
- the all-feature sidecar suite with 458 passing and 16 intentionally ignored tests;
- compilation of the PostgreSQL integration target;
- warning-denied Clippy with no default features, the Xiaoman Feishu adapter feature,
  and all features;
- `cargo fmt`, `git diff --check`, and `pnpm check:pr:auto`.

The guarded PostgreSQL regression proves that a rejected oldest notification is
terminal, creates no attempt, and does not block the next valid claim. It compiled
locally but could not execute because no disposable `qintopia_test` database was
available on loopback. Replacement PR CI executed that regression successfully and then
found the downstream reserved-alias failure. A further replacement CI must prove both
the regression and the full downstream apply smoke before merge.

## Remaining Boundary

The real Feishu adapter remains disabled and requires separate owner-reviewed
credentials, allowlists, callback permissions, release/database bindings, activation,
and end-to-end direct-message acceptance. An approved image still cannot authorize or
create a group send.
