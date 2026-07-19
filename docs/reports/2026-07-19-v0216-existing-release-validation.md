# v0.2.16 Existing Release Validation Gap

Date: 2026-07-19 Asia/Shanghai

## Observed Evidence

At publication, the owner-published `v0.2.16` Release and `origin/master` both pointed
to `c353a767f725f80a575bf60bc886f2da257d84f2`. Release-triggered Deploy Production run
`29674135332` completed with `dry_run=false`, server status `succeeded`, previous SHA
`ad4d79fa19916afcf1f90c332d661efe2531d201`, and no rollback.

The first server-side assembly was handled by the previous `v0.2.15` runner. A read-only
acceptance check then found:

```text
release owner=lighthouse:ubuntu mode=0755
deploy owner=lighthouse:ubuntu mode=0755
sidecar binary owner=lighthouse:ubuntu mode=0755
artifact-manifest.json owner=root:root mode=0640
SHA256SUMS owner=root:root mode=0640
```

The promoted `v0.2.16` runner contained both reviewed `tar --no-same-owner` extraction
calls and the `0444` metadata normalization. Owner-approved same-SHA follow-up run
`29674328754` completed with `dry_run=false`, server status `succeeded`, and no
rollback, but the release tree remained unchanged. The follow-up also changed `previous`
from `v0.2.15` to the same SHA as `current`.

The Huabaosi production image-generation timer remained `disabled` and `inactive`.

## Root Cause

`promote-release.sh` assembles a fresh staging tree before checking whether the
immutable release directory already exists. For an existing release it validates only
selected manifest fields, deletes the fresh staging tree, and reports success while
retaining the old release tree. It does not validate existing ownership or modes.

The same-SHA path also writes the current target to `previous` before repointing
`current`, even when both targets are identical. A successful result therefore hid the
unreadable metadata and removed the distinct rollback pointer.

## Resolution

- Validate every newly assembled tree before promotion.
- Require every entry to be owned by the effective deploy-runner UID.
- Reject non-symlink entries that are group- or world-writable.
- Require directories to remain group/world readable and traversable, and reject special
  file types.
- Require the sidecar binary to be a regular file with mode `0755`.
- Require packaged manifests, checksum files, and archives to be regular files with mode
  `0444`.
- For an existing same-SHA release, prove exact identity, complete tree content
  equality, and both packaged checksums before repairing owner/modes, then run the same
  strict tree validation. Reject content or path drift before metadata mutation.
- Preserve `previous` when an idempotent same-SHA request already targets `current`.

The repair must include executable promotion fixtures covering metadata repair only
after content proof, content-drift rejection, strict new-tree validation, valid same-SHA
reuse without rollback-pointer drift, and successful new release promotion. Fixture
directory modes must be explicit so validation does not depend on the process `umask`.

## Remaining Boundary

Do not chmod, chown, replace, or delete the deployed `v0.2.16` release directory. A new
owner-published Release with a distinct SHA must be assembled by the current runner and
pass unprivileged release-local observation before Huabaosi image generation is
reactivated.

This repair does not call the image provider, write Feishu, create or approve an image,
publish a message, call QiWe, or send externally.
