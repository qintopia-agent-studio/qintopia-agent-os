# Restricted communication contact lock

Date: 2026-09-29. Owner: Agent OS / Person Foundation. Risk: high (identity and
concurrent routing).

The restricted management UI can read identity tables but cannot take PostgreSQL row
locks on them. Existing `management_ui_lock_identity_candidate` locks a source link and
Gateway, but not its Person. Existing `management_ui_lock_actor` locks the Person only
when the source namespace equals the tenant login namespace, so it cannot cover a
separate QiWe or WeCom Gateway namespace. Removing the lock would allow a contact to
change while a choice is saved.

Migration `202609290002` adds one fixed `SECURITY DEFINER` function accepting tenant,
scope, source link and Person UUIDs. It uses fully qualified tables and a fixed
`pg_catalog, pg_temp` search path, locks all matching current source, Gateway, Person
and scope rows, and returns only whether the exact combination exists. It accepts no
table names, SQL, free-form predicates or destination. The caller first takes the
existing Gateway registry `SHARE` lock in the same transaction to prevent a new
ambiguous namespace owner. The subsequent ordinary query still checks source and Gateway
versions and all selection rules. Registered shared work accounts use the existing
`management_ui_lock_business_account` function.

The migration revokes `PUBLIC EXECUTE` immediately and does not create roles, grant
identity-table writes, or prepare production ACLs. Before enabling the management UI,
the deployment owner must review the dedicated `NOLOGIN` function owner and grant the UI
role only this function's `EXECUTE`, following the existing management-lock preflight.
Production role changes and deployment need separate authorization. No CI, external
send, PMS fact or existing identity link is modified.

The group candidate and saved choice must also follow current `read_business`
authorization, role, duty, collaboration and autonomous audience decisions. Staff-group
platform does not limit which independently observed staff contact channel may be
selected; a contact choice alone never grants private-message or cross-channel send
authority. The host must recheck any actual target channel before delivery.

Validate with an isolated PostgreSQL database and a nonempty restricted UI role: current
and revoked grants, role/duty/agent shutdown, cross-tenant/scope/namespace denial,
Gateway insertion and source/person changes during save, and function ACL, owner and
search-path checks. Rollback disables the UI route while preserving the additive
function and saved choices; a stale choice yields no destination.
