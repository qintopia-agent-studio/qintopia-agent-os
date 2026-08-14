# Erhua Group CSV Workspace

This package owns Erhua's narrow, append-only CSV persistence capability for QiWe
groups. It lets Erhua create typed datasets from conversation, append auditable rows,
query or aggregate current-group data, evolve schemas by adding optional fields, and use
a built-in exact-Decimal ledger preset.

## Boundary

- Scope is derived only from Hermes task-local `HERMES_SESSION_*` context.
- Only QiWe group sessions are accepted. Direct chats and missing context fail closed.
- Tool inputs never accept group ids, user ids, message ids, filenames, or paths.
- Raw Hermes `file`, `terminal`, and `code_execution` toolsets remain disabled.
- Data is append-only. There is no overwrite, delete, rename, move, or arbitrary file
  primitive.
- No network request, external send, Postgres write, or secret access occurs.

The production root is fixed at `~/.hermes/profiles/erhua/data/csv/v1`. Recovery
snapshots live in the separate, tool-inaccessible
`~/.hermes/profiles/erhua/data/csv-internal/v1` subtree. Group data is isolated by
`sha256("qiwe\\0" + chat_id)` and internal UUID dataset ids.

The public tool registration stays in `skills/qintopia-tools/variants/erhua`; this
package owns validation and storage behavior. See
`docs/plans/completed/erhua-group-csv-workspace.md` for the complete contract.

Decimal inputs and their fixed-point expansion are each capped at 1024 characters or
digits before formatting. This keeps exact-Decimal fields and ledger arithmetic bounded
against exponent-expansion resource exhaustion.

Group CSVs are visible to all members of that group. They must not be used for
passwords, tokens, government identity numbers, or similarly sensitive data.

## Validation

```bash
pnpm skills:erhua-csv:check
pnpm skills:qintopia-tools:check
pnpm test:qiwe
pnpm registry:check
pnpm policy:check
```

## Rollback

Roll back `release/current` to the previous reviewed Release and restart
`hermes-gateway-erhua.service`. The previous release stops exposing the tools; CSV data
and recovery snapshots remain untouched for a later forward rollout.
