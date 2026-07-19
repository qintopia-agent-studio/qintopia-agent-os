# Same-SHA Release Metadata Repair

Date: 2026-07-19 Asia/Shanghai

## Observed Evidence

The owner published `v0.2.16` at `c353a767f725f80a575bf60bc886f2da257d84f2`. The
release-triggered deployment and an exact-manifest same-SHA follow-up both succeeded,
but the promoted release still had archive-derived metadata from the previous runner:

- the release root, deploy tree, and sidecar binary were owned by the unrelated server
  account mapped from GitHub runner UID `1001`;
- sidecar `artifact-manifest.json`, `SHA256SUMS`, and packaged archive were mode `0640`
  and root-owned; and
- the unprivileged release-local checksum verification failed with `Permission denied`.

## Root Cause

The archive extraction fixes in `v0.2.16` correctly normalize freshly fetched staging
artifacts. The initial promotion was processed by the previous runner, so its existing
release tree retained the old metadata. The new runner's same-SHA path fetched and
verified fresh artifacts but discarded them after checking only manifest identity. It
did not reconcile the already existing release's owner or modes.

## Resolution

The existing-release path now requires all of the following before metadata mutation:

- exact release, runtime, deploy-bundle, commit, scope, and restart-target identity;
- complete path, type, symlink-target, and regular-file content equality against the
  freshly fetched verified release tree, excluding only request-specific manifest
  content;
- successful sidecar and deploy-bundle `SHA256SUMS` verification; and
- a non-symlink existing release directory.

After those gates pass, the runner makes the release tree root-owned and copies modes
from the freshly assembled tree. Any content or path drift fails before `chown` or
`chmod`.

## Validation

- `bash -n deploy/runner/promote-release.sh`;
- `node tools/deploy/test-promote-existing-release-metadata.mjs`;
- `node tools/deploy/check-deploy-runner.mjs`;
- `node tools/deploy/check-deploy-contracts.mjs`;
- `node tools/deploy/preflight.mjs --ci`;
- formatting, Markdown lint, and `git diff --check`.

## Remaining Boundary

This PR does not hot-edit the current release, publish a Release, trigger deployment,
enable Huabaosi or Feishu timers, create staging configuration, call a provider, write
Feishu, call QiWe, or send externally. Production acceptance requires a later
owner-approved Release assembled by the corrected runner. A same-SHA follow-up may be
used only with the exact immutable manifest identity and must preserve the distinct
rollback pointer.

## Next Owner Action

Merge and release this runner repair, then verify the new release tree is root-owned and
its packaged evidence is world-readable before running release-local Huabaosi and
staging-values observations.
