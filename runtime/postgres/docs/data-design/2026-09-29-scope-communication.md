# Scope communication and host recovery

Owner: Agent OS Person/WorkItem service. Risk: high (identity and external routing).

## Purpose and boundary

Anan's operations group and contact accounts must be selected from current, observed,
scope-bound identities. Welcome content review keeps its existing
`welcome_review_settings`; a general scope communication choice does not grant review,
PMS operation, or external publication authority. No production send is enabled by this
change. PMS remains authoritative for stays and orders.

## Storage and contract

Migration `202609290001` adds `collaboration_scopes.communication_config jsonb NOT NULL`
with an empty-object default. An empty object means no general operations destination.
The existing `collaboration_scopes.version` advances on each saved choice. The tenant
configuration command retains its own expected version and operation id.

The stored object contains only a selected active scope group binding and its version,
the bound conversation reference and platform, plus selected Person or registered work
account references with their observed source-link, gateway and version snapshots. A
display name, raw `chat_id`, sender id, arbitrary destination, permission text, or
credential is never accepted from the browser as routing authority. A disconnected
binding reactivated under the same id must have a new version and cannot restore an old
choice. Source and gateway version changes also invalidate that choice. Saved labels are
display hints; a rename does not invalidate otherwise current identity evidence.

`scope_communication_candidates` lists bounded, searchable group, contact, unregistered
observed work-account and registered-account choices only after the current
scope-management grant. Each category has its own stable cursor bound to tenant, scope,
category and search text; each page rechecks authorization. An account's active grant
summary is capped at 256 entries and explicitly marked incomplete if more exist.
`validate_scope_communication_in` checks the selected group, active audience, channel
and contact links in the same transaction as the existing business configuration
command. The host-only `scope_communication_current` obtains the route from the
currently bound property and gateway, repeats these checks, and returns no route when
they fail. The delivery adapter must still check the action-specific audience and
authorization immediately before a send. Neither the model nor a recovery-list argument
supplies a tenant, property, scope, person, or destination.

`pms_workitem_recovery` uses the existing host token and environment-bound gateway and
property. `list(after?,limit<=100)` is read-only and cursor-bounded; `detail(work_item)`
returns the current WorkItem, contact requirement, route-version snapshot and original
send state. It never claims work or creates a send. An unknown send outcome retains its
original key. An external receipt can only be reconciled through a proven transport
readback for that same key; the OS cannot infer delivery from a lost acknowledgement.

The payment event listener acknowledges only after inbox and WorkItem persistence. Its
best-effort immediate wake follows that commit. Startup recovery queries the same
durable work and offers the existing wake hint again; a failed wake cannot lose the item
or replay the business send.

## Validation and rollback

Use a dedicated disposable local PostgreSQL instance, never an existing acceptance or
production database. Cover active and revoked bindings, changed versions, ambiguous
identities, scope isolation, read-only pagination, wake failure, restart and unknown
effects. Run focused package checks before `pnpm check:pr:auto`.

Rollback disables the host and wake gates first. Preserve the additive column, retained
WorkItems and receipts; do not erase identity or delivery evidence. An older binary
ignores the new column. Missing or stale settings continue to block general sends.
