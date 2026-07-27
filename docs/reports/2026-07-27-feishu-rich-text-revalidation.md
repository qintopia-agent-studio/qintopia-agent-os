# Feishu Rich-Text Revalidation Compatibility

Date: 2026-07-27

## Finding

The staging Huabaosi generated-image flow successfully created a pending JPEG and stored
it in the Feishu artifact table. Authenticated pre-approval revalidation then failed
with `record_identity_mismatch`.

The live Feishu Base response represents text columns as a single-element rich-text
array containing a `text` value and a `type` marker. Numeric columns remain numeric and
the final JPEG column remains a single attachment array. The revalidation reader only
accepted scalar JSON strings for text columns, so valid records were rejected before
attachment download.

## Scope

The compatibility fix accepts both the existing scalar string shape and exactly one
rich-text item with a string `text` value. Empty, multi-item, or malformed rich-text
values remain invalid. Numeric identity checks, attachment checks, byte readback, hash
validation, review policy, and external-send boundaries are unchanged.

## Validation

- `RUST_MIN_STACK=33554432 cargo test --features huabaosi-production-adapter`: passed.
- `node tools/deploy/check-xiaoman-production-evidence-chain-local.mjs`: passed.
- The live staging revalidation remains pending deployment of the compatibility fix.

## Boundary

This record contains no credentials, Base/table/record identifiers, attachment tokens,
URLs, raw provider responses, or image bytes. The fix does not approve artifacts,
publish, send QiWe messages, enable timers, or modify production configuration.
