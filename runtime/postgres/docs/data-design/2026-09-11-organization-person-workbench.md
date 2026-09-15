# Organization and person workbench A

Version: `2026-09-11.001`. Owner: Agent OS / sidecar. Additive, synthetic-local
verification only.

The migration references the preceding short decision in
[organization-workbench-a-increment.md](../../../../docs/plans/active/organization-workbench-a-increment.md).
It does not change previously applied migration checksums or seed live identities,
appointments or grants.

| Table                   | Meaning and authority                                                                                                                               |
| ----------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| collaboration_positions | Role definition + scope + organizational parent. A vacancy is valid. Parent links are responsibility structure, never authorization inheritance.    |
| collaboration_ledger    | Tenant-local metadata referencing existing Person UUIDs, registered Agent keys or conversation UUIDs. This is not another person identity database. |
| collaboration_audiences | One connection's groups, explicit people, dynamic residence selector, public reception, reply and proactive decisions, reviewer and visibility.     |

An organization position represents a role in a particular scope; existing appointments
keep their role/scope keys. Previously used position dimensions cannot be rewritten.
Changing occupant replaces the connection transactionally; ending an appointment revokes
its whole responsibility immediately. Other scopes and duties remain independent.

Person lookup uses existing confirmed names and aliases for discovery, never
identification by name alone. New persons have distinct existing `persons.id` values and
pending ledger status; registration creates no confirmed channel link or appointment.
The UI does not accept a typed identity ID as an authentication claim. Agent
registration does not create a Hermes runtime. Known repository keys and unconnected
business registrations are separate from actual runtime readiness. Group registration
selects an existing active accessible conversation; raw platform identifiers are not
displayed or copied into the ledger. Scope binding is a separate authorized command.

Commands use the existing locked tenant configuration version, idempotency key and audit
transaction. Position/ledger/contact changes include before/after snapshots in the
existing command history. Rejected previews roll back. Retiring a ledger or position
ends affected active connections in the same transaction. Groups lose current bindings.
Restoring definitions cannot reopen old appointments, grants, audiences, bindings or
WorkItems. Only explicit unused drafts are deletable; existing role/duty/scope
definitions retain their history when retired.

Dynamic current/past/all selectors persist the requested boundary but never invent PMS
membership. The contact configuration evaluator refuses unresolved dynamic targets.
Public reception can admit an unregistered incoming consultation with `public_only`
visibility; it cannot authorize proactive contact or private information disclosure.
Proactive contact additionally needs the existing publish authorization. Contact
settings retain their originating management grant and recheck its current chain;
confirmation reviewers are rechecked, not assumed permanently valid. All evaluator
responses are observations of current configuration, not execution tickets. C must check
actual identity, purpose, consent, business facts and data visibility again at
execution.

Local sessions resolve a confirmed identity from a server-selected link and revalidate
its version; browser JSON cannot switch actor. Synthetic tenant/source guards remain
separate from live state. Real login provider, real channel coverage, runtime
consumption and sending remain outside this local A delivery.

Rollback: stop the local listener; retain additive tables and command history. Do not
delete production data or reverse previous migration checksums. Tests only use synthetic
tenants in the designated loopback `qintopia_test` instance.
