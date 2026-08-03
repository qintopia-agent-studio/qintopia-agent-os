# Xiaoman Release Boundary Contract Mismatch

Date: 2026-08-03

## Summary

The `v0.2.66` Release deploy succeeded, but the Xiaoman direct configuration preview
failed before any persistent runtime configuration or service mutation. The deploy
runner promoted a trusted `root:root`, non-group/world-writable Release using directory
mode `0755`, while the new Xiaoman configuration and conversation-policy entrypoints
required directory mode `0555`.

No Feishu call, database write, configuration apply, service reload, or activation was
performed while diagnosing this failure. The preview created or reused only its
root-only transient lock under `/run`.

## Evidence

- GitHub Actions run `30776782711` reported a successful real deployment of Release SHA
  `bb986fc0c4089fa662870b18e84486d0e2b54e42` with no rollback.
- Production `release/current` resolved to that SHA and the Xiaoman plugin resolved to
  the immutable Release plugin.
- The Release root, `sidecar/`, and deploy directories were `root:root 0755`.
- The no-apply configuration entrypoint returned a redacted validation failure before
  external calls, database writes, or service changes.
- Source inspection proved the deploy runner accepts owner-writable, root-owned paths,
  while the Xiaoman entrypoints rejected the owner write bit without checking owner
  identity.

The probes emitted only Release identity, modes, owners, booleans, and counts. They did
not emit database addresses, credentials, chat or user ids, message text, or raw logs.

## Root Cause

The promotion and activation tests modeled two different definitions of an immutable
Release. Promotion treats the privileged deploy owner as trusted and rejects group/world
writes. The Xiaoman tests used mode `0555` as a proxy for trust, even though a non-root
owner can restore its own write bit. No test passed an actual promoted tree through both
protected Xiaoman path validators.

## Resolution

- Require the Release root, `sidecar/`, and sidecar binary to be owned by the effective
  privileged process.
- Continue rejecting symlink escapes, unsupported path types, and group/world writes.
- Accept the deploy runner's root-owned `0755` directory mode so same-SHA metadata
  repair remains possible.
- Run the real promotion fixture through both Xiaoman protected path validators.
- Document one shared Release integrity contract for promotion, configuration, policy,
  activation, and rollback code.

## Validation

Run the narrow checks first:

```bash
python3 tools/deploy/test_xiaoman_feishu_poster_production_config_apply.py
python3 tools/deploy/test_xiaoman_conversation_policy_production_apply.py
node tools/deploy/test-promote-release-tree.mjs
pnpm deploy:contracts:check
pnpm deploy:runner:check
```

Then run `pnpm check:pr:auto`. Production still requires a new reviewed Release,
database credential rollover, direct configuration, a private policy, same-SHA reload,
no-network preflight, explicit direct activation, and one new real Feishu private
request.
