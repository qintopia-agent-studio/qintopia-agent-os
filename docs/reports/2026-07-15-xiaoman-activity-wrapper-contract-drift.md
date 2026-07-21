# Xiaoman Activity Wrapper Contract Drift

Date: 2026-07-15

Status: resolved in this PR; production deployment remains a later Release decision

## Observed Evidence

The `v0.2.10` release-managed Xiaoman plugin exposed `status_update` and `gap_update`
with the legacy Feishu activity shape. It generated `record_id` and `table_role` fields,
while the Rust `xiaoman-activity` command required internal `event_signal_id` and
caller-supplied `mutation_id` UUIDs and explicitly rejected Feishu identifiers.

The generated wrapper command therefore failed Rust payload validation before database
access. Existing Rust tests covered the correct event-signal mutation contract, but the
Xiaoman plugin suite did not exercise any activity wrapper and the package checker did
not require the activity tools to be registered.

## Root Cause

The audited Postgres mutation boundary replaced the earlier planned Feishu write shape
in Rust without updating the independently packaged Hermes tool schemas and handlers.
The two layers had separate tests with no assertion over their shared payload fields.

## Resolution

- Replace the wrapper write schemas with the UUID-only AgentOS mutation contract.
- Remove legacy `record_id`, `table_role`, `status_note`, and `missing_fields` write
  inputs.
- Preserve caller-provided mutation UUIDs in the sidecar payload so exact retries remain
  idempotent.
- Reject overlong gap summaries before command construction instead of silently
  truncating them.
- Add focused wrapper tests for schema fields, sidecar payloads, dry-run/apply modes,
  missing mutation ids, and gap length.
- Extend the existing Xiaoman acceptance smoke to pass wrapper-generated status and gap
  payloads through the Rust command validator in the same run.
- Require all six Xiaoman activity tools in package registration checks and declare them
  in the plugin manifest.

## Validation

- `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s skills/qintopia-tools/variants/xiaoman/tests -p 'test_*.py'`:
  28 passed.
- `RUST_MIN_STACK=33554432 cargo test --manifest-path runtime/sidecar/Cargo.toml`: 371
  passed.
- `deploy/sidecar/scripts/xiaoman-activity-acceptance-smoke.sh`: passed, including
  Python-wrapper payload validation by the Rust dry-run command.
- `node tools/skills/check-qintopia-tools.mjs` and
  `node tools/workflows/check-workflows.mjs`: passed.
- `node tools/policy/check-anti-drift.mjs`, `node tools/security/check-secrets.mjs`,
  `node tools/deploy/check-deploy-contracts.mjs`, and
  `node tools/deploy/preflight.mjs --ci`: passed.
- Repository-local Markdown and Prettier checks: 208 Markdown files with zero errors;
  formatting passed.
- `git diff --check`: passed.

The local pnpm shim rejected registry signature verification before running
`skills:qintopia-tools:check`, `workflows:check`, `lint:md`, and `format:check`. No
signature bypass was used. Each fixed repository-local Node or installed CLI entrypoint
was run directly and passed.

## Remaining Boundary

This repair does not add activity lifecycle phases, Feishu writes, new handoff mappings,
image generation, QiWe sends, external publication, systemd changes, or production
configuration. Deployment must use a later owner-published Release; the live server must
not be hot-edited.
