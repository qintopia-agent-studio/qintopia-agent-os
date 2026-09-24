# Welcome subject confirmation

Migration: `202609240003_welcome_subject_confirmation.sql`. Owner: person collaboration
/ resident welcome. Local only; no production activation.

## Decision and scope

The
[audited owner decision](../../../../docs/reports/2026-09-24-welcome-identity-and-review-audit.md)
requires Anan to present candidate identity links and welcome content in one persistent
matter, shared by the configured operations group and authenticated UI. A configured
work account may confirm within that matter; it is not a Person or a PMS operator.

This additive slice follows PMS baseline `201b961` and does not depend on migration 002.
Existing personal Actor, verify, gateway_actor and PMS business execution remain
unchanged. Existing identity/review actions are used; no welcome permission category or
production send path is introduced.

## Persistence

- `qintopia_identity.work_accounts`: verified, observed channel account identity,
  namespace and gateway version; no fabricated Person. Changing or disabling the source
  invalidates later use.
- `welcome_review_settings`: tenant/scope and existing operations conversation,
  versioned configuration and source management authority.
- `welcome_review_subject_grants`: explicit person/work_account subject and existing
  identity/review effects, expiry and source management grants. Replacing settings
  revokes previous grants; historical decisions retain their original subjects.
- `welcome_review_items`: existing WorkItem and case/application references, candidate
  selection and concrete artifact versions, current state and optimistic version.
- `welcome_review_receipts`: command hash, exact subject/version, separately recorded
  identity and content effects. Receipts and WorkItem events are committed atomically.
- `source_identity_links.confirmed_by_work_account`: explicit alternative confirmation
  attribution. Confirmed links still require a real Person and evidence. Personal
  verification APIs retain their existing stronger personal-confirmation requirement.

Operations use the existing tenant transaction lock, then revalidate subject, source
configuration, authorization, case/application/artifact versions. Ambiguous names are
candidates only; incomplete links stay pending. An explicit combined decision first
establishes the requested identity segments, then checks the exact content. A failed
content check rolls the whole command back. Missing identity never blocks opening an
identity-confirmation matter. Rejection, correction and revocation retain audit history.

Approved operations content is not a building publication approval. The existing
building wording, current membership, consent and publish checks continue separately. No
PMS or Feishu source fact is changed. No group message is actually sent by this slice.

## Validation and recovery

Use a newly created disposable loopback `qintopia_test` and inspect its applied
migration versions before applying 003; preserve old databases and checksums. Run actual
PostgreSQL transactions for personal compatibility, work-account
scope/expiry/revocation, replay, concurrency, partial links, combined confirmation
rollback and changed content. Exercise HTTP and trusted group entry points against the
same WorkItem. Run Rust formatting, compiler/Clippy and affected identity, welcome and
PMS regressions, then the existing PR checks. Tests do not establish real channel
delivery or production readiness.

Disable the local entry point to stop processing. Retain records and receipts; do not
reverse the migration by deleting identity evidence or replaying external effects.
