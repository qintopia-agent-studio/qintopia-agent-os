# Welcome source and group projection

Migration: `202609240005_welcome_source_and_group_projection.sql`. Reserved by the
coordinator on 2026-09-24 for the already-authorized local completion slice. Existing
001–004 checksums remain unchanged. See the
[implementation contract](../../../../docs/plans/active/welcome-local-completion.md).

## Source authority and minimal storage

`welcome_source_projections` is an expiring matching projection of an existing source
application, not a person master or proof of identity. It holds tenant/scope, the
current property binding/version, application/revision, trusted identity/content
digests, and bounded name/nickname/phone/arrival hints. The host supplies the exact
allowlisted fields from the fixed source read; the server recomputes both digests
against the existing 004 committed readback before persisting only matching hints. A
model cannot invoke this host operation or supply a digest as authority. Source
invalidation, consent withdrawal, binding changes or revision changes prevent reuse,
even before the cached row expires. The current owner decision requires phone numbers on
new applications, with phone as the main matching hint and names/nicknames as auxiliary
hints. Missing trustworthy PMS phone evidence remains explicit; no guessed match becomes
an identity confirmation. Phone values are never included in general lists, audit or
group text; only masked suffixes may be shown to authorized reviewers.

004 stores irreversible digests, so matching cannot recover these fields without this
small projection. Stay and channel candidates come from existing scoped cases, persons,
aliases and observed channel facts; missing authoritative phone data remains missing.
Candidate scores and unique matches never bind a person. Public `ReviewOpen` and
explicit per-effect `ReviewDecision` remain the only confirmation path.

## Group presentation lifecycle

`welcome_group_presentations` is subordinate to an existing review WorkItem, its
version, configuration version and configured conversation. It stores a rendered
snapshot and bounded candidate-index mapping, the current configuration issuer and its
authority proof (not a human approval), an opaque human-readable reference, and
pending/claimed/delivered/failed/unknown/stale state. Uniqueness on
work/version/configuration prevents duplicate presentation preparation. It is not a
generic queue or a second scheduler. Claim persists before the injected local transport
can run. Claim and callbacks revalidate configuration, source/content, current
configuration authority. Expired claims become unknown; unknown outcomes are never
automatically retried. A later readback may record the original delivery receipt.

A group confirmation references a specific presentation and version, chooses numbered
candidates and explicitly lists identity/channel/content effects. The server parses the
persisted authenticated incoming message; model parameters cannot substitute text,
acting subject, destination or approval. Old versions, wrong groups, revoked authority
and ambiguous unreferenced agreement fail closed. Results use the same review receipt as
UI actions. No production transport, credential loading or external sending is enabled.

## Retention, rollout and rollback

Apply only in this task's isolated local PostgreSQL after 001–004. No existing row is
backfilled into a permission or approved match. Projection updates replace stale hints;
expired hints are unusable. Presentation/audit identities are retained for recovery.
Stop the local host to roll back; do not drop audit records or rewrite migration hashes.
Tests cover scope/source/revision/consent mismatches, ambiguous candidates, stale
approval, configuration changes, same-request recovery and unknown delivery non-replay.
Final UI and genuine provider behavior remain separate acceptance evidence.

## Frozen source host DTO

Use `schema_version: 1`, `agent: "anan"` and `trusted_context`,
`operation=person_foundation_ingress`, `tool=welcome_source_projection`, the independent
host token and fixed Anan gateway. Arguments are exactly
`{binding: UUID, application: UUID, fields: object}`. Binding must equal host
`QINTOPIA_APPLICATION_BINDING`; application is the preceding 004 save result. Fields are
the normalized values from that same trusted read, with their exact mapped key set
preserved. Allowed keys: name, nickname, phone, arrival, nights, room_type, occupation,
interests, consent, status. Name/nickname/phone/consent keys are required; unavailable
cells are null. Values are null, boolean, integer or strings of at most 2048 characters
without NUL. Name/nickname matching hints allow 120 characters; phone/arrival allow 40.
No caller hashes, source revisions or model-selected endpoints.

Recompute identity `{name,nickname,phone}` and full `{fields,valid,consent_active}`
digests with 004's stored validity flags. Current binding, intake source, application
revision and consent must still match. Success returns
`{stored:true,identity_confirmed:false}`. Hints become unusable for matching after 24
hours (no physical deletion is claimed); missing authoritative PMS phone fields remain
explicitly missing. Order primaryGuest is never copied to individual occupants. No new
PMS interface or production send is introduced.

## Final source decision (2026-09-25)

The owner withdrew the intermediate document proposal and returned to required phone
numbers on new applications. Historical records are not backfilled or blocked. No
per-occupant document arrays, document hashes or extra migration are introduced. This
migration uses the existing 004 digest contract. Shared phones and conflicting or
multiple candidates still require explicit confirmation; a phone does not prove
ownership of a WeChat/WeCom account.

Configuration issuer fields in presentations record the authority to present a matter,
not a human approval. Callback authority is separately resolved from the authenticated
message sender. Delivery receipts can settle an already attempted send even if policy
later changes; they cannot authorize a confirmation or another send.
