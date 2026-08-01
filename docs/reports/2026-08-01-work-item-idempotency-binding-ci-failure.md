# 2026-08-01 Work-Item Idempotency Binding CI Failure

## Scope

PR #346 hardens concurrent work-item creation so a reused idempotency key cannot select
an unrelated request. Its replacement CI passed every focused PostgreSQL integration
test but failed the existing Xiaoman downstream apply smoke.

This is repository-only remediation. No production database, Feishu API, image provider,
service, timer, group capability, or external send was used.

## Evidence

GitHub Actions run `30702748580`, job `91376507158`, completed all focused Xiaoman,
Huabaosi, and QiWe PostgreSQL steps successfully. The final
`Xiaoman downstream apply smoke` failed with:

```text
Error: idempotency key is already bound to a different work item request (brief_summary)
```

The smoke intentionally replays one activity request through a second supported starter.
Both paths use the same stable business idempotency key and identity bindings, but may
render a different human-facing summary. The default `dedupe_key` also changes because
its legacy derivation includes that summary.

## Root Cause

The first binding validator treated every stored request field as immutable. That was
stricter than the existing operations contract: `brief_summary` is presentation text,
and `dedupe_key` is a secondary derived value when the request already carries an
explicit stable idempotency key. Neither field identifies capability ownership,
parentage, source provenance, or payload meaning.

## Resolution

- Exclude `brief_summary` and its derived `dedupe_key` from idempotent winner
  validation.
- Continue rejecting drift in capability, work-item type, parent, requester/provider,
  source event, source type, source references, payload, and policy bindings.
- Add a focused regression proving refreshed presentation fields still reuse the stable
  binding.
- Keep both the sequential replay and concurrent `ON CONFLICT` winner paths behind the
  same validator.

## Validation

Local validation passed:

- three focused idempotent-binding unit tests;
- 476 default Rust tests;
- 482 all-feature Rust tests with 20 guarded PostgreSQL tests ignored by design;
- warning-denied Clippy in no-default-feature and all-feature configurations;
- `pnpm check:pr:auto`, including quick, runtime, smoke, and heavy Rust tiers.

The first `pnpm check:pr:auto` run stopped at Prettier for the two newly edited Markdown
files. Formatting those exact files resolved the failure; the replacement run passed.

The local PostgreSQL 16 mirror could not start because two attempts to pull
`pgvector/pgvector:pg16` failed during the Docker Hub TLS handshake. No container was
created. Replacement PR CI must execute the complete disposable PostgreSQL downstream
apply smoke and all focused PostgreSQL integration tests before merge.

## Remaining Boundary

This change does not make arbitrary payload drift idempotent and does not relax poster
conversation, review, delivery, or publication policy. Internal-group delivery remains
disabled and PR 3 has not started.
