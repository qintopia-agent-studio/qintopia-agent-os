# Production Sidecar Env Metadata Repair

Date: 2026-08-03

## Summary

A real Xiaoman production canary attempt on `paxon-server` reached the pending
`poster_brief` stage, then failed closed before approving the brief, calling the image
provider, writing Feishu, publishing, calling QiWe, or sending. The Huabaosi one-shot
canary rejected the fixed sidecar environment boundary because the existing production
env file was not root-owned.

The Huabaosi timer was restored through the reviewed activation script after the failed
canary attempt.

## Evidence

- The deployed `release/current` was the reviewed `v0.2.69` release SHA.
- The release root, sidecar directory, Huabaosi binary, and sidecar manifest were
  root-owned with reviewed modes.
- The fixed sidecar env file was a non-symlink regular file with safe non-writable
  group/world bits, but its owner was not root.
- The canary exited with the reviewed release-boundary failure before external side
  effects.

The probes emitted only release identity, modes, owners, booleans, and counts. They did
not emit database URLs, credentials, chat ids, message ids, group ids, record ids,
message text, provider responses, or raw logs.

## Root Cause

The old deploy script normalized env metadata only when creating a missing env file. An
existing production env file could keep legacy ownership across later reviewed release
promotions, while the one-shot Huabaosi production canary correctly requires the fixed
env file to be root-owned before parsing provider, Feishu, release, and database
bindings.

## Resolution

The release systemd installer now validates the existing fixed sidecar env path as a
non-symlink regular file, rejects hard links, group/world write bits, and oversized
files, then normalizes metadata to `root:ubuntu 0640` before rendering and installing
release-local systemd units.

This keeps the repair inside the reviewed deploy/runner path and avoids ad-hoc
production `chown` or `chmod`.

## Validation

```bash
bash -n deploy/runner/install-release-systemd-units.sh
node tools/deploy/test-release-systemd-install.mjs
node tools/deploy/check-deploy-runner.mjs
node tools/deploy/check-deploy-contracts.mjs
```
