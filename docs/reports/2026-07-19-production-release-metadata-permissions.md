# Production Release Metadata Permissions

Date: 2026-07-19 Asia/Shanghai

## Observed Evidence

After `v0.2.15` was published, a read-only check of the still-current `v0.2.14` release
on `paxon-server` found the sidecar binary mode `0755`, while
`sidecar/artifact-manifest.json` and `sidecar/SHA256SUMS` were mode `0640`.

The unprivileged Huabaosi Feishu production observation could execute the binary but
failed with `PermissionError` while reading `artifact-manifest.json`. It therefore could
not prove that the immutable release used exactly the approved production Cargo
features.

## Root Cause

The COS fetcher normalized executable permissions but retained the COS client's download
mode for manifests, checksums, and archives. Root-owned promotion then left mode `0640`
metadata unreadable to the `ubuntu` runtime user.

These files contain release identity and checksums, not secrets. Restricting them to the
deployment owner did not protect credentials; it prevented the reviewed release-local
verification path from running.

## Resolution

The COS fetcher now installs both artifact manifests and checksum files mode `0444`,
keeps the sidecar binary mode `0755`, and installs packaged sidecar and deploy-bundle
archives mode `0444`. The deploy runner checker requires these exact permission
normalizations so later changes fail CI if they reintroduce the unreadable state.

## Validation

The focused regression and repository checks passed:

- `bash -n deploy/sidecar/scripts/fetch-cos-artifact.sh`;
- `node tools/deploy/test-fetch-cos-artifact-permissions.mjs`;
- `node tools/deploy/check-deploy-contracts.mjs`;
- `node tools/deploy/check-deploy-runner.mjs`; and
- Prettier on every changed source and documentation file.

The fixture executes the production COS fetcher with a fake COS client and verified the
sidecar mode `0755` plus manifest, checksum, and archive modes `0444`. A post-deploy
server observation remains required to prove the next release has the corrected modes.

## Remaining Boundary

This repair does not enable a timer, run a provider, access Postgres, write Feishu, call
QiWe, process a callback, publish, or send externally. It does not mutate an existing
immutable release; production receives the corrected mode only through a new reviewed
Release deployment.

## Follow-up Owner Action

Merge and publish the reviewed fix Release, then rerun the Huabaosi Feishu production
observation from `release/current` before activating its timer.
