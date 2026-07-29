# Xiaoman Production Runtime Cutover Remediation

Date: 2026-07-28

## Goal

Restore a deployable production boundary for Xiaoman without replacing Huabaosi's
runtime, adding a combined production binary, adding a CI job, or changing production
outside the reviewed Release flow.

The observable result is one immutable release containing two independently reviewed
sidecar artifacts:

```text
<release>/sidecar/qintopia-message-sidecar
  Huabaosi production runtime

<release>/sidecar-profiles/qiwe-production/qintopia-message-sidecar
  QiWe production runtime
```

All existing internal and Huabaosi units continue to use `sidecar/`. Only the QiWe
image-send preflight/worker, callback processor, and Xiaoman production-evidence export
use the QiWe companion path.

## Confirmed Production Evidence

- Published release and `release/current` both resolve to commit
  `626148b3bf8b01b9e514f67682aaa7a1571c2ec0` (`v0.2.48`).
- Failed deploy run `30353110202` used a release scope and restart-target identity that
  did not match the existing immutable manifest.
- Failed deploy run `30353502354` reached existing-tree verification and rejected an
  unexpected top-level `coscli_output` directory.
- The deploy-runner service currently uses `release/current` as its working directory.
  COSCLI created two root-owned `YYYYMMDD_HHMMSS/process.log` diagnostic trees there.
- Huabaosi generation, Huabaosi Feishu mirror, and QiWe image-send timers remain
  disabled and inactive. Neither failed deploy switched `current` or sent an image.
- The current release has one global `sidecar/` slot. Switching that slot to the QiWe
  artifact would remove Huabaosi image-generation capability.

## Decisions

1. The deploy runner works from `/var/lib/qintopia-agent-os-deploy`, and every
   server-side COSCLI subprocess gets an explicit temporary working directory.
2. A release promotion always fetches and verifies both production artifacts for the
   same reviewed runtime SHA. The Huabaosi artifact stays primary; the QiWe artifact is
   installed as a companion.
3. Existing `coscli_output` is never silently ignored or deleted. The runner accepts
   only the observed bounded COSCLI diagnostic shape, validates it before mutation, and
   moves it to a root-only deploy-state quarantine. Any other extra path still fails.
4. Existing-release dry-run performs the same manifest, inventory, contamination, and
   companion-migration compatibility checks as apply, but does not mutate the existing
   release, quarantine, symlinks, systemd, services, or timers.
5. The Release workflow builds, uploads, prunes, and read-validates both artifacts in
   its existing build and deploy jobs. No new CI job is added.
6. Production callback processing resolves the fixed QiWe companion under
   `release/current`; release-bound path and digest identity come from the immutable
   artifact, not a direct old release path in runtime-local configuration.
7. The legacy owner-triggered rollback workflow stays unavailable until a published
   target contains a reviewed Huabaosi primary, QiWe companion, and dual-runtime deploy
   bundle. It must not expose Huabaosi/QiWe as a global profile choice.

## Implementation Increments

1. Isolate COSCLI working state and make the runner service's state directory its
   working directory.
2. Add strict `coscli_output` validation/quarantine and make dry-run execute complete
   existing-release compatibility checks.
3. Assemble and validate both runtime artifacts, including the legacy same-SHA companion
   installation path without changing the Huabaosi binary.
4. Bind only QiWe systemd units, production observations, activation, callback ingress,
   and production evidence to the companion binary.
5. Extend the existing Release workflow to publish and verify both artifacts.
6. Record the failed-deploy evidence, resolution, rollback boundary, and first rollout
   procedure in an indexed report.

## Acceptance

- A scope or restart-target mismatch fails in dry-run before existing-release mutation.
- Valid observed COSCLI diagnostics pass dry-run, remain in place during dry-run, and
  move to deploy-state quarantine only during a successful apply.
- Arbitrary extra files, malformed diagnostic trees, partial companion trees, changed
  files, symlinks, or unsupported file types fail before mutation.
- A legacy Huabaosi-only release gains the QiWe companion while the Huabaosi binary and
  manifest hashes remain unchanged.
- Rendered Huabaosi/internal units execute `sidecar/`; rendered QiWe units execute
  `sidecar-profiles/qiwe-production/`.
- Enabled QiWe observation, activation, callback processing, and real-activity evidence
  reject the Huabaosi binary and accept only the reviewed companion artifact.
- `pnpm deploy:runner:check`, `pnpm deploy:contracts:check`,
  `pnpm deploy:systemd:check`, `pnpm deploy:release-model:check`, `pnpm test:qiwe`, and
  `pnpm check:pr:auto` pass.

## First Rollout

The first Release is intentionally two-stage because its initial deployment is handled
by the previous runner:

1. Publish the reviewed Release. It promotes the new deploy bundle and both COS
   artifacts but may still use the old runner behavior for that first request.
2. Submit one same-SHA follow-up with the existing release manifest's exact scope and
   restart targets. The new runner then validates/quarantines the known COSCLI output,
   installs the QiWe companion, updates the runner unit, and renders the corrected QiWe
   bindings.
3. Run release-local disabled/enabled observations as appropriate before any external
   timer activation or real callback/send acceptance.

Subsequent published Releases carry both runtimes in their first promotion.

The existing `v0.2.0` rollback candidate predates this layout and is intentionally
rejected before COS access or deploy-request creation. A later Release may become the
first owner-triggered rollback target only after its complete dual-runtime artifact set
and deploy bundle are audited.

## Production Boundary

This repair does not publish a Release, submit a deploy request, edit the server, enable
an external timer, process a callback, call Huabaosi, write Feishu or Postgres, call
QiWe, publish, or send. Those actions remain later owner-reviewed rollout and acceptance
steps.
