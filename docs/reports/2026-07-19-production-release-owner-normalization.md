# Production Release Owner Normalization

Date: 2026-07-19 Asia/Shanghai

## Observed Evidence

The production `v0.2.15` release at `ad4d79fa19916afcf1f90c332d661efe2531d201` was
inspected read-only after the Xiaoman and Huabaosi production observations passed. Its
release directory, deploy directory, and release-local profile renderer were owned by
numeric UID `1001`, mapped on the server to an unrelated account, instead of root.

The release-local Xiaoman profile values observation therefore failed closed with:

```text
values_file_path_missing
renderer_path_parent_unexpected_owner
```

The missing values file remains a separate manual prerequisite. The renderer ownership
failure is a release assembly defect and must not be bypassed by weakening the
observation allowlist.

## Root Cause

The root production deploy runner extracted the sidecar and deploy-bundle archives with
plain `tar -xzf`. GNU tar preserves archived numeric ownership when root extracts an
archive, so files created by GitHub runner UID `1001` retained that owner on the
production server. The later `cp -a` release assembly step then propagated the already
incorrect owner into the immutable release tree.

## Resolution

Both production COS archive extraction paths use `tar --no-same-owner`, making extracted
files owned by the root deploy process before release assembly. Preflight and deploy
runner source contracts require the flag on both sidecar and deploy-bundle extraction,
and the COS fetch fixture records both tar invocations.

## Validation

The change must pass:

- `bash -n deploy/sidecar/scripts/fetch-cos-artifact.sh`;
- `node tools/deploy/test-fetch-cos-artifact-permissions.mjs`;
- `node tools/deploy/preflight.mjs --ci`;
- `node tools/deploy/check-deploy-runner.mjs`;
- `node tools/deploy/check-deploy-contracts.mjs`;
- Prettier on changed files; and
- Markdown lint.

The next owner-approved combined Release must additionally prove that the promoted
release directory, deploy directory, renderer, and sidecar are root-owned before the
profile values observation is treated as valid.

## Remaining Boundary

This repair does not publish a Release, deploy to production, change the current
`v0.2.15` tree, create the Xiaoman profile values file, render a live profile, restart
Hermes, enable Feishu mirroring, call QiWe, or send externally.

## Follow-up Owner Action

Keep the pending Release Please PR unmerged until the staging image and QiWe workflow
evidence is complete. Include this repair in that single final Release, then verify the
new root-owned release tree through read-only production observations before any
remaining activation decision.
