# Operations

This directory contains operational evidence, source inventories, and future runbook
inputs. Server documents are summarized here as evidence before they are adopted into
canonical architecture, engineering, package, or deployment docs.

## Documents

- [inventory/README.md](inventory/README.md): M1 migration inventory for local sources,
  server sources, runtime assets, profiles, and services.
- [inventory/m10-hermes-profile-runtime-inventory.md](inventory/m10-hermes-profile-runtime-inventory.md):
  post-M9-F Hermes profile/plugin/script inventory and M10/M11 migration gates.
- [source-document-inventory.md](source-document-inventory.md): read-only inventory of
  server and local documents reviewed during the documentation organization pass.
- [runtime-baseline.md](runtime-baseline.md): production runtime baseline and migration
  implications.
- [server-directory-plan.md](server-directory-plan.md): target server filesystem shape,
  transition directories, legacy cleanup candidates, and Hermes runtime boundary.
- [release-current-model.md](release-current-model.md): target release directory,
  `current`/`previous` symlink, promotion, rollback, and Hermes mount model.
- [production-deploy-runner.md](production-deploy-runner.md): GitHub Release-triggered
  production deploy requests, COS pull runner, release promotion, and rollback model.
- [profile-bundles/m10f-profile-template-plan.md](profile-bundles/m10f-profile-template-plan.md):
  M10-F profile template and future `SOUL.md` / `config.yaml` symlink boundary.
- [archive-readiness/m11-legacy-path-readiness.md](archive-readiness/m11-legacy-path-readiness.md):
  M11 read-only archive-ready and decommission-batch evidence for legacy paths.
- [archive-readiness/m12-low-risk-archive.md](archive-readiness/m12-low-risk-archive.md):
  M12 first low-risk legacy archive batch, validation evidence, rollback path, and
  remaining decommission scope.
- [archive-readiness/m12-openclaw-decommission.md](archive-readiness/m12-openclaw-decommission.md):
  M12-B OpenClaw decommission archive, nginx route cleanup, validation, and rollback
  notes.
- [archive-readiness/m12-worktool-xiaoqin-decommission.md](archive-readiness/m12-worktool-xiaoqin-decommission.md):
  M12-C WorkTool and current WorkTool-bound Xiaoqin runtime archive, validation, and
  rollback notes.
- [agent-capability-matrix.md](agent-capability-matrix.md): active Agent package
  capabilities, approval boundaries, and runtime-state exclusions.
- [sidecar-ci-artifacts.md](sidecar-ci-artifacts.md): M9.1 sidecar artifact contract, CI
  build output, checksum verification, and server download requirements.
- [cos-artifact-distribution.md](cos-artifact-distribution.md): Tencent COS bucket,
  credential, upload, and server download runbook for production artifact delivery.
- [m9-server-cutover-runbook.md](m9-server-cutover-runbook.md): final migration runbook
  for monorepo checkout, sidecar service cutover, deprecated runtime cleanup,
  acceptance, and rollback.
- [../deploy/sidecar/docs/systemd-cutover-plan.md](../../deploy/sidecar/docs/systemd-cutover-plan.md):
  M9.3 monorepo-native sidecar systemd target shape and rollback sequence.

## Checks

- `pnpm agents:check`: validates active Agent package templates and dry-run
  expectations.
- `pnpm artifact:sidecar`: builds the sidecar release artifact layout locally.
- `pnpm deploy:postgres:schema:preflight`: runs the read-only Postgres schema gate for
  M9 after production env is loaded.
- `pnpm deploy:systemd:check`: validates the M9.3 sidecar systemd unit renderer without
  touching `/etc/systemd/system`.
- `pnpm deploy:runner:check`: validates the Release-triggered production deploy request
  workflow, runner schemas, server pull-runner scripts, and deploy bundle packaging.
- `pnpm deploy:preflight`: validates non-mutating deployment gates before any server
  cutover.

## Rules

- Do not edit server docs or code directly.
- Convert deployment evidence into runbooks through reviewed git changes.
- Treat server-side exploration as `review-pool` until owner review.
- Do not copy live secrets, `.env` files, generated caches, raw member profile text, or
  private chat logs into this repository.
