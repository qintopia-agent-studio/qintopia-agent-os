# QiWe Callback Shape Evidence Boundary

Date: 2026-07-14

## Current State

`process-qiwe-image-send-callback --dry-run` already reads one callback from bounded
stdin, requires exactly one `cmd=20000` event, validates complete image credentials, and
does not connect to Postgres or open a network connection. Its report previously exposed
only that a callback was received.

The parser accepts the documented `fileAesKey` and `filename` spellings plus the
observed compatibility aliases `fileAeskey` and `fileName`. Deserializing aliases into
one Rust structure erased which spelling the callback actually used, so an owner could
not record the staging credential shape without inspecting sensitive raw input.

## Risk

Raw callbacks contain request correlation, file credentials, filenames, and potentially
unknown provider fields. Logging or storing the callback to learn its shape would cross
the repository's credential boundary. Treating canonical and alias spellings as
interchangeable without reporting the observed shape would also leave the staging
contract unproven.

## Resolution

Inspect the raw `msgData` object before credential deserialization and collapse accepted
field spellings into one of four fixed public schema ids:

```text
fileAesKey+fileId+fileMd5+fileSize+filename
fileAeskey+fileId+fileMd5+fileSize+filename
fileAesKey+fileId+fileMd5+fileSize+fileName
fileAeskey+fileId+fileMd5+fileSize+fileName
```

The callback report may include only the fixed schema id and the number of fields
outside the reviewed canonical and alias set. It must not include unknown field names or
values. If canonical and alias spellings for the same credential appear together,
parsing fails closed because the callback shape is ambiguous.

## Validation

Rust unit tests cover all four accepted schema ids, ambiguous canonical-plus-alias
inputs, additional-field counting, and serialized-report leakage checks.

The focused nextest selection passed `4/4`. The final default-feature suite passed
`337/337`; its first restricted-sandbox run passed 330 tests and failed only the six
existing fake-server tests that could not bind loopback, then the exact command passed
with loopback permission. The final all-feature suite passed `334/334`, with eight
guarded PostgreSQL integration tests skipped by design.

Warning-denied Clippy passed for both `--no-default-features` and `--all-features`.
Pre-commit formatting, Markdown, workflow, deploy, and Xiaoman readiness checks passed,
as did anti-drift policy, secret, runtime-contract, CI-contract, and Cargo advisory,
ban, and source checks. Cargo deny reported only the existing duplicate-version warnings
and no advisory, ban, or source failure.

A local default-build CLI dry-run reported the fixed
`fileAeskey+fileId+fileMd5+fileSize+fileName` schema id and one additional field. Its
JSON report excluded the callback request id, all credential values, filename value, MD5
value, unknown field name, and unknown value.

## Production Boundary

This change does not capture a real callback and does not prove QiWe staging behavior.
It adds no listener, service, timer, database migration, runtime enablement, or
deployment configuration. The dry-run path remains bounded local stdin parsing with no
database or network access. No provider, media service, Feishu endpoint, QiWe endpoint,
or production Postgres instance is contacted.

## Follow-Up Owner Action

In an owner-approved isolated staging session, feed one callback directly to the bounded
processor and retain only its fixed schema id and additional-field count as evidence.
Review the exact image bytes and the send success response separately before deciding
whether a guarded staging smoke may execute.
