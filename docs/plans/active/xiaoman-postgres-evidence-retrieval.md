# Xiaoman Postgres Evidence Retrieval

Updated: 2026-07-13

## Goal

Replace the Xiaoman activity evidence placeholder with source-grounded retrieval from
the existing Postgres message store. The evidence worker remains an internal AgentOS
worker and does not call Wenyuange, embeddings, Feishu, QiWe, or any external adapter.

## Source Contract

For a Xiaoman activity `evidence_request` whose `source_type` is `event_signal`, the
worker must resolve `work_items.source_event_signal_id` to
`qintopia_agent_os.event_signals`. It must not attempt to recover raw identifiers from
hashed `source_refs`. Existing manual and fixture contracts remain separate and do not
pretend to have event-signal provenance.

Retrieval is deliberately narrow:

1. Load messages whose internal UUIDs appear in `event_signals.source_message_ids`,
   while also requiring the signal's platform and chat id.
2. Only when that list is empty, search the same platform and chat within the signal's
   bounded source window. Use local keyword matching against the signal title; do not
   use semantic or embedding search.
3. If neither path returns an authorized message, fail the work item without creating or
   completing an `evidence_summary` artifact.

The generated artifact may contain the accepted event-signal title and summary,
sanitized short message snippets, and internal message UUIDs. It must not contain
platform message ids, raw chat ids, sender ids, unbounded raw chat, tokens, or secrets.

## Runtime Behavior

- Existing non-Xiaoman or fixture evidence requests retain their current internal
  placeholder behavior until they receive a separately reviewed source contract.
- Xiaoman retrieval and artifact persistence occur under the worker's existing Postgres
  transaction and claim.
- Reprocessing keeps the existing `(work_item_id, content_hash)` idempotency boundary.
- Reports expose only retrieval strategy and source counts, not source identifiers or
  message content.

## Production Boundary

- Postgres reads: `event_signals` and allowlisted rows from
  `qintopia_messages.messages`.
- Postgres writes: one internal `evidence_summary`, work-item state, and audit event.
- External calls: none.
- Feishu writes: none.
- QiWe sends: none.
- Image generation and publishing: none.

Rollback is the normal release rollback to the previous immutable sidecar. No schema
migration or production data rewrite is required by this change.

## Validation

```bash
bash -n deploy/sidecar/scripts/operations-control-plane-apply-smoke.sh
node tools/workflows/check-workflows.mjs
node tools/deploy/check-deploy-contracts.mjs
cargo fmt --check --manifest-path runtime/sidecar/Cargo.toml
cargo clippy --manifest-path runtime/sidecar/Cargo.toml --all-targets -- -D warnings
RUST_MIN_STACK=33554432 cargo test --manifest-path runtime/sidecar/Cargo.toml
QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE=1 deploy/sidecar/scripts/operations-control-plane-apply-smoke.sh
sh .husky/pre-commit
git diff --check
```
