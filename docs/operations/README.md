# Operations

This directory contains operational evidence, source inventories, release/current
runbooks, and archive-readiness records. Server documents are summarized here as
evidence before they are adopted into canonical architecture, engineering, package, or
deployment docs.

## Documents

- [inventory/README.md](inventory/README.md): M1 migration inventory for local sources,
  server sources, runtime assets, profiles, and services.
- [inventory/m10-hermes-profile-runtime-inventory.md](inventory/m10-hermes-profile-runtime-inventory.md):
  post-M9-F Hermes profile/plugin/script inventory and M10/M11 migration gates.
- [source-document-inventory.md](source-document-inventory.md): read-only inventory of
  server and local documents reviewed during the documentation organization pass.
- [runtime-baseline.md](runtime-baseline.md): current production runtime baseline,
  release/current state, and remaining profile/plugin or archive-retention work.
- [server-directory-plan.md](server-directory-plan.md): target server filesystem shape,
  transition directories, legacy cleanup candidates, and Hermes runtime boundary.
- [release-current-model.md](release-current-model.md): active release directory,
  `current`/`previous` symlink, promotion, rollback, and Hermes mount model.
- [production-deploy-runner.md](production-deploy-runner.md): GitHub Release-triggered
  production deploy requests, COS pull runner, release promotion, and rollback model.
- [production-timer-activation-runbook.md](production-timer-activation-runbook.md):
  signed production timer activation requests for reviewed Erhua/Xiaoman timers through
  the deploy runner.
- [production-runtime-observation-runbook.md](production-runtime-observation-runbook.md):
  signed read-only production observations for QiWe image-send and Xiaoman daily
  case-report state before reviewed activation decisions.
- [production-legacy-cron-retirement-runbook.md](production-legacy-cron-retirement-runbook.md):
  signed production legacy Hermes cron retirement requests before release-managed timer
  activation retries.
- [xiaoman-production-evidence-runbook.md](xiaoman-production-evidence-runbook.md):
  owner-operated Huabaosi canary, QiWe companion verification, real-activity retention,
  and final completion-manifest sequence. The same runbook also includes the reviewed
  one-shot completion finalizer for the last retained-evidence step.
  <p>reviewed one-shot completion finalizer</p>
- [xiaoman-feishu-poster-production-closeout-runbook.md](xiaoman-feishu-poster-production-closeout-runbook.md):
  one-Release database credential rollover, trusted direct configuration and policy,
  direct acceptance, one internal-group canary, and rollback sequence.
- [xiaoman-weekly-minimum-loop-runbook.md](xiaoman-weekly-minimum-loop-runbook.md):
  2026-08-01 Xiaoman three-step weekly operations loop status and action content for the
  Sunday 20:00 plan confirmation.
- [xiaoman-weekly-loop-cutover-runbook.md](xiaoman-weekly-loop-cutover-runbook.md):
  release-managed Saturday recruitment and Sunday plan-confirmation timer activation,
  observation, and rollback for the Xiaoman weekly loop.
- [xiaoman-weekly-preview-cutover-runbook.md](xiaoman-weekly-preview-cutover-runbook.md):
  release-managed Xiaoman weekly preview timer activation, observation, and rollback
  while preserving the human confirmation gate.
- [erhua-morning-brief-production-activation-runbook.md](erhua-morning-brief-production-activation-runbook.md):
  release-managed Erhua morning brief timer activation, observation, and rollback for
  the reviewed 08:10 text artifact workflow.
- [erhua-member-recognition-production-runbook.md](erhua-member-recognition-production-runbook.md):
  release-local production config observation, then release-local QiWe room roster sync,
  identity bootstrap, safe profile refresh, coverage checker, sanitized answer-context
  canary builder, canary checker, and final completion checker for Erhua member
  recognition. Retained canary JSONL keeps irreversible `person_ref` markers only, never
  database `person_id` values.
- Xiaoman activity read-through production config is applied through the release-local
  `deploy/sidecar/scripts/apply-xiaoman-activity-read-through-production-config.py`
  allowlist copier before release-managed Erhua or weekly-preview workers are manually
  exercised in production. Feishu Base mode must be enabled with
  `QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE=1`; otherwise the read-through wrapper
  omits `--use-feishu-base` and no live activity records are read.
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
- [m9-server-cutover-runbook.md](m9-server-cutover-runbook.md): historical cutover
  evidence and reusable runbook for future approved repoints, cleanup windows,
  acceptance, and rollback.
- [../deploy/sidecar/docs/systemd-cutover-plan.md](../../deploy/sidecar/docs/systemd-cutover-plan.md):
  M9.3 monorepo-native sidecar systemd target shape and rollback sequence.

## Checks

- `pnpm agents:check`: validates active Agent package templates and dry-run
  expectations.
- `pnpm artifact:sidecar`: builds the sidecar release artifact layout locally.
- `node tools/deploy/check-xiaoman-production-evidence-chain-local.mjs`: reruns the
  repository-local Xiaoman production evidence chain verification bundle before any
  owner-operated production evidence capture.
- `pnpm deploy:xiaoman-production-evidence:finalize -- --release-please-pr-number ...`:
  builds the final Xiaoman production completion manifest and immediately revalidates it
  against the retained staging and production evidence files.
- `pnpm deploy:erhua-member-recognition:finalize -- --room-sync ...`: runs the reviewed
  Erhua member-recognition completion checker and independently revalidates the retained
  sanitized completion summary.
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
