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

After presentation fields were removed from the stable binding, replacement run
`30703712897`, job `91379071557`, again passed every focused PostgreSQL test and then
failed the same downstream smoke with:

```text
Error: idempotency key is already bound to a different work item request (source_refs)
```

A traced local PostgreSQL 16 replay localized the second failure to the direct
`xiaoman-activity signal-ingest` immediately after the activity signal worker had
created the same `in_event` root. The worker used the persisted event signal's hashed
chat and source-message references. The direct apply payload carried only the event
signal ID, so its locally rendered source references omitted that persisted provenance.

After source provenance was canonicalized, replacement run `30705025875`, job
`91382559912`, passed the first eight focused PostgreSQL tests and then exposed the
first-valid group revision race:

```text
reviewer revision resolves: idempotency key is already bound to a different work item request (source_refs)
```

Two authorized reviewers intentionally compete on one source image. The artifact-scoped
key selects the first valid instruction, while each contender necessarily has a
different authenticated message, actor, instruction, and derived prompt hash.

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

The replacement failure exposed a separate boundary error. The direct apply path already
read the activity phase from Postgres, but still derived `source_refs` from the caller
payload. That made two supported starters disagree about the same persisted event signal
even though `source_event_signal_id` and the business payload were identical. Unlike the
presentation fields, `source_refs` are provenance and must remain strictly bound.

The third failure was a different idempotency contract, not another missing canonical
field. Poster revision uses an artifact-scoped first-writer key as its concurrency
boundary. Treating every contender as an exact replay rejected the loser instead of
returning the already committed winner, even though the persisted winner remained the
only instruction eligible to create the next image.

## Resolution

- Exclude `brief_summary` and its derived `dedupe_key` from idempotent winner
  validation.
- Continue rejecting drift in capability, work-item type, parent, requester/provider,
  source event, source type, source references, payload, and policy bindings.
- Add a focused regression proving refreshed presentation fields still reuse the stable
  binding.
- Keep both the sequential replay and concurrent `ON CONFLICT` winner paths behind the
  same validator.
- On direct signal apply, load phase, chat, source-message IDs, signal type, and signal
  date from the referenced AgentOS event signal and rebuild the same sanitized
  `source_refs` used by the background worker.
- Keep exact `source_refs` validation in place so a reused key cannot cross source
  provenance.
- Recognize first-valid poster revision contenders only when both stored and incoming
  requests are self-consistent and bind the same source image, approved brief, route,
  capability, safety flags, and artifact-derived idempotency key. Only the authenticated
  message, actor, instruction, and their derived hashes may differ; the stored winner
  remains authoritative.
- Keep every other work-item idempotency path on exact source-reference and payload
  matching, and add negative regressions for source-image and publication-safety drift.

## Validation

Local validation passed:

- five focused idempotent-binding unit tests;
- 479 default Rust tests;
- 485 all-feature Rust tests with 20 guarded PostgreSQL tests ignored by design;
- warning-denied Clippy in no-default-feature and all-feature configurations;
- `pnpm check:pr:auto`, including quick, runtime, smoke, and heavy Rust tiers.

One bare local `cargo test` attempt omitted the repository-required 32 MiB test-thread
stack and aborted in the known large async worker test. The prescribed
`RUST_MIN_STACK=33554432` default and all-feature commands passed; production runtime
configuration was not changed.

The first `pnpm check:pr:auto` run stopped at Prettier for the two newly edited Markdown
files. Formatting those exact files resolved the failure; the replacement run passed.

The latest PR Reviewer Guide separately found that poster callback `--dry-run` could
write a rejection or duplicate audit before reaching its apply gate. The review finding
was accepted. Target mismatch, participant authorization, runtime allowlist, duplicate,
and conflict audits are now conditional on apply mode, while validation still fails
closed. The protected PostgreSQL callback scenario now proves that valid and rejected
dry-runs leave review state, review actions, and mutation-audit counts unchanged. Real
callback apply continues to retain the rejection and duplicate audit trail.

The local PostgreSQL 16 mirror could not start because two attempts to pull
`pgvector/pgvector:pg16` failed during the Docker Hub TLS handshake and a third stalled
before Docker assembled a usable image. No pgvector container was created. Replacement
PR CI must execute the complete disposable PostgreSQL downstream apply smoke and all
focused PostgreSQL integration tests before merge.

For localization only, a cached plain PostgreSQL 16 image ran all repository migrations
in a disposable database with the unrelated vector columns replaced by text. The traced
downstream smoke reproduced the exact `source_refs` failure and command boundary. This
same full downstream smoke passed after direct apply rebuilt its source references from
the persisted event signal. This diagnostic database is not a replacement for the
repository-supported pgvector CI job; the final fix still requires that full job to
pass.

## Remaining Boundary

This change does not make arbitrary payload drift idempotent and does not relax poster
conversation, review, delivery, or publication policy. Internal-group delivery remains
disabled and PR 3 has not started.
