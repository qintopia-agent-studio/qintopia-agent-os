# Staging Sidecar Provision Umask

Date: 2026-07-19 Asia/Shanghai

## Observed Evidence

The combined Huabaosi/QiWe staging artifact for
`54df4347cc7a7c643f8033427385985e8fa575b2` built successfully in GitHub Actions run
`29669502031`. The production `v0.2.15` staging provisioner downloaded it through the
server-local GitHub App and passed all manifest and checksum checks.

The first operator attempt then stopped before installation because the newly created
release directory was mode `0775`. The `ubuntu` operator has ambient `umask 0002`, while
the provisioner used `mkdir -p` and immediately rejected group-writable path components.
The failed attempt left one empty release directory and no sidecar files.

An earlier root invocation was also rejected at `/home/ubuntu`. That rejection is
correct: staging provisioning is an unprivileged operator workflow and must not weaken
owner checks to let root traverse a user-owned mutable home boundary.

## Root Cause

The provisioner correctly rejected group-writable staging paths but created its own
release root and release directory with modes derived from ambient `umask`. It also
tracked cleanup only after creating `sidecar/`, so a failure immediately after release
directory creation left the empty directory behind.

## Resolution

The provisioner now creates the release root, release directory, and sidecar directory
with explicit mode `0755`, then preserves the existing final immutable modes. It tracks
which root and release directories were created by the current attempt and removes only
those empty directories on failure. Existing release paths remain no-clobber.

The regression fixture runs the provisioner under `umask 0002` and requires the staging
release root to remain mode `0755`.

The first replacement CI run exposed a stale preflight source assertion that still
required the previous `mkdir "$sidecar_dir"` form. The deploy contract checker already
required explicit `0755` creation and bounded cleanup for all three directories. The
preflight assertion now enforces the same fragments, preventing those two CI contracts
from drifting apart again.

## Validation

The focused validation passed:

- `bash -n deploy/sidecar/scripts/fetch-staging-sidecar-artifact.sh`;
- `node tools/deploy/test-fetch-staging-sidecar-artifact.mjs`;
- `node tools/deploy/check-deploy-contracts.mjs`;
- `node tools/deploy/preflight.mjs --ci`;
- Prettier on every changed source and documentation file; and
- Markdown lint on the changed documentation.

The server provision was then resumed without changing production `release/current`. The
verified empty failed release directory was removed, and the existing `v0.2.15` operator
script was run under explicit `umask 022` against the same reviewed artifact. It
completed with:

```text
run_id=29669502031
release_sha=54df4347cc7a7c643f8033427385985e8fa575b2
sidecar_sha256=52513b117a0f7ce83ce531c661f27337ac961d78fd68893c3972bfeb3362fef2
release_dir_mode=0555
sidecar_dir_mode=0555
sidecar_binary_mode=0555
artifact_manifest_mode=0444
```

The release-local prerequisite observation accepted the release root and sidecar hash.
Its only remaining limitation was `env_file_path_missing`.

## Remaining Boundary

This repair does not publish a Release, change production `release/current`, create a
staging env or database, run the staging sidecar, call a provider, write Feishu, call
QiWe, enable a production sender, or send externally.

## Follow-up Owner Action

Merge the reviewed fix so future operator runs do not require an explicit umask
workaround. Provision the separate owner-reviewed staging database and values file
before rendering `/etc/qintopia/message-sidecar-staging.env`.
