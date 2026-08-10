# Erhua Member Safe Alias - Paxon

Date: 2026-08-10 Type: reviewed production payload Owner: qiaopengjun Risk level: low

## Scope

This package records the owner-reviewed safe alias for one Erhua member-recognition
coverage gap. The retained production coverage sample identified a linked person with
the sanitized key `fc2c1a46c0af` whose observed display name was numeric-only: `000`.
Numeric-only names are intentionally excluded from answer-context canary evidence, so
this member needs a reviewed human-readable alias before full member recognition
coverage can be claimed.

Owner confirmation in the Codex task: use `paxon`.

## Payload

The deployable payload is:

- `safe-alias.json`

It contains only:

- the sanitized `person_key`
- the owner-reviewed alias
- the sanitized source display name
- the fixed review reason

It does not include QiWe user ids, `chat_id`, `sender_id`, raw messages, raw profile
text, tokens, or database URLs.

## Production Boundary

This directory does not write production state by itself. Apply must happen only after
the reviewed release that contains the safe-alias command is deployed, using the
production runbook path:

```bash
node tools/deploy/check-erhua-member-safe-alias-payload.mjs \
  docs/operations/review-pool/hermes/2026-08-10-erhua-member-safe-alias-paxon/safe-alias.json

qintopia-message-sidecar erhua-member-safe-alias \
  --payload-file docs/operations/review-pool/hermes/2026-08-10-erhua-member-safe-alias-paxon/safe-alias.json

QINTOPIA_ERHUA_MEMBER_SAFE_ALIAS_APPROVAL=approved-production-erhua-member-safe-alias \
  qintopia-message-sidecar erhua-member-safe-alias \
    --payload-file docs/operations/review-pool/hermes/2026-08-10-erhua-member-safe-alias-paxon/safe-alias.json \
    --apply
```

After apply, rerun identity bootstrap, member profile generation, all-member
answer-context canaries, and the completion checker.

## Validation

```bash
node tools/deploy/check-erhua-member-safe-alias-payload.mjs \
  docs/operations/review-pool/hermes/2026-08-10-erhua-member-safe-alias-paxon/safe-alias.json
```

## Rollback

No runtime state is touched by this review-pool package. If the production alias is
applied incorrectly later, remove or supersede it only through a separate owner-reviewed
database change.
