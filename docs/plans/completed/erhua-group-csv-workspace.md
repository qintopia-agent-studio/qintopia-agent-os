# Erhua Group CSV Workspace

Status: implementation complete  
Owner: Qintopia  
Updated: 2026-08-14

## Goal

Give Erhua a narrow, conversation-driven CSV capability for QiWe group chats. Erhua can
create a dataset, define typed fields, append rows, list datasets, and run bounded
queries or aggregates without receiving the native Hermes `file`, `terminal`, or
`code_execution` toolsets.

The first use case is group bookkeeping, but the capability is intentionally generic:
future group persistence needs may create a new CSV and schema through conversation
instead of requiring a product-specific code path.

## Scope

- Enable all Erhua QiWe group chats; direct chats remain rejected.
- Trust only task-local `HERMES_SESSION_*` values for platform, group, actor, and source
  message identity. Tool arguments never accept a group id or filesystem path.
- Store one isolated workspace under a full SHA-256 scope derived from `qiwe` and the
  current group id.
- Support `text`, `decimal`, `integer`, `boolean`, `date`, `datetime`, and `enum`
  business fields.
- Keep catalogs and rows append-only. Schema evolution creates a new immutable version
  that adds optional fields; it cannot remove or change existing fields.
- Merge all versions when querying a logical dataset. Undeclared values are retained in
  `_extra_json` until a later schema version promotes them to formal fields.
- Provide a `ledger` preset with generated signed deltas, reversal events, checksum
  verification, and post-write balance recomputation.
- Create one internal recovery snapshot after the first successful mutation on each
  Asia/Shanghai day and retain the latest 30 dates.

Out of scope for v1: direct-chat spaces, overwrite, delete, rename, move, arbitrary file
access, file sending, cross-group queries, joins, formulas, administrator reports,
Postgres, or real external sends.

## Public Tool Contract

The Erhua `qintopia` toolset registers four stable wrappers. Their implementation is
owned by `skills/erhua-csv`; `skills/qintopia-tools/variants/erhua` performs only Hermes
registration and delegation.

- `qintopia_erhua_csv_list`: list current-group datasets, or describe one `csv_id`.
- `qintopia_erhua_csv_create`: create a `custom` or `ledger` dataset; `version_of`
  creates the next version by adding optional fields only.
- `qintopia_erhua_csv_append`: append one typed row. System fields and ledger-generated
  fields are rejected when caller-supplied. `reverses_event_id` creates a single inverse
  ledger event.
- `qintopia_erhua_csv_query`: paginate up to 100 rows, apply at most five equality
  filters, and optionally calculate `count` and an exact Decimal `sum`.

Tool responses may show dataset metadata, business data, `_event_id`, `_created_at`, and
`_actor_name`. They must not expose raw actor ids, source message ids, group ids, scope
hashes, or internal paths.

## Storage And Integrity

```text
~/.hermes/profiles/erhua/data/csv/v1/groups/<sha256("qiwe\\0" + chat_id)>/
├── catalog.jsonl
├── datasets/<csv-id>/schemas/v1.json
├── datasets/<csv-id>/rows/v1.csv
└── locks/
```

Directories use mode `0700`; files use `0600`. Dataset ids are UUIDs. The engine uses
the Python standard CSV and JSON encoders, `flock`, `O_APPEND`, `O_NOFOLLOW`, and
`fsync`. It refuses symlinks, path escapes, malformed rows, partial tails, invalid
headers, or checksum drift. After every append it rereads the appended row and verifies
its checksum before returning success.

Each row carries:

```text
_event_id, _created_at, _actor_user_id, _actor_name,
_source_message_id, _row_sha256, <business fields>, _extra_json
```

The canonical row checksum covers the durable row contents except `_row_sha256`.
Idempotency covers current group, source message, logical CSV, schema version, and the
canonical caller row. Replaying the same mutation returns the original event rather than
appending a duplicate.

The primary append and its checksum verification are the commit boundary. If the
post-commit daily recovery snapshot fails, the tool returns the committed result with an
explicit snapshot warning instead of incorrectly reporting that no write occurred.

## Ledger Rules

The ledger preset defines:

```text
occurred_at, account, currency, direction, amount,
amount_delta, category, note, reverses_event_id
```

`account` defaults to `cash`, `currency` to `CNY`; `direction` is `income` or `expense`;
`amount` must be a positive Decimal. The engine generates `amount_delta` as positive
income or negative expense. A reversal references one existing event in the same logical
ledger, copies its account/currency/amount, reverses its direction, and is allowed once.
Every successful ledger append recomputes the account/currency balance across every
schema version using Decimal arithmetic.

## Limits And Safety

- 20 logical datasets per group, 5 versions per dataset, 32 business fields per schema.
- 16 KiB encoded row, 10 MiB row file, 100 MiB total group workspace.
- V1 does not impose a per-member operation rate limit. Capacity limits and serialized
  group writes remain in place; add rate limiting later only from observed abuse or
  performance evidence.
- 100 returned query rows and 5 equality filters.
- Decimal input text and fixed-point expansion are each limited to 1024 characters or
  digits before formatting, preventing oversized exponents from exhausting resources.
- Formula-like text beginning with `=`, `+`, `-`, `@`, tab, or carriage return is
  rejected. Negative values are accepted only by numeric field types.
- CSV data is group-visible and must not store passwords, tokens, government identity
  numbers, or similarly sensitive data.

## Release And Rollback

The package, Erhua registration wrapper, QiWe group prompt, registry metadata, deploy
bundle, and `hermes-erhua` restart routing ship together through a reviewed feature PR
and formal Release. Production continues to resolve plugins and delegated skills from
`release/current`; no live SOUL, profile config, or server source file is edited.

Rollback points `release/current` to the previous Release and restarts
`hermes-gateway-erhua.service`. Existing CSV data and internal snapshots remain in the
profile data directory. The previous release simply does not expose the new tools.

## Validation

```bash
pnpm skills:erhua-csv:check
pnpm skills:qintopia-tools:check
pnpm test:qiwe
pnpm registry:check
pnpm policy:check
pnpm deploy:contracts:check
pnpm artifact:deploy-bundle
pnpm check:pr:auto
```

Focused tests cover typed schemas, versions, `_extra_json`, idempotency, limits, formula
injection, cross-group isolation, direct-chat rejection, symlink/path attacks,
concurrent append, malformed or tampered files, restart reads, ledger precision and
reversals, post-write balance verification, and daily snapshot retention.

Post-release smoke is read-only apart from lazy directory creation: verify tool
discovery, profile data permissions, and an empty current-group list. The first real
group bookkeeping row is the business acceptance event; rollout must not create an
undeletable synthetic ledger entry.
