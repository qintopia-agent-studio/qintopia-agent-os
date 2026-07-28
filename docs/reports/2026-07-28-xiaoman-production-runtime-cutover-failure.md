# Xiaoman Production Runtime Cutover Failure

Date: 2026-07-28

## Summary

Two production deployment attempts for the `v0.2.48` commit
`626148b3bf8b01b9e514f67682aaa7a1571c2ec0` failed closed before switching
`release/current`. The first request did not match the existing immutable release scope
and restart-target identity. The second reached release-tree comparison and rejected
root-owned COSCLI diagnostic output created inside the release directory.

No failed attempt enabled Huabaosi image generation, Huabaosi Feishu mirroring, or QiWe
image sending. The associated timers remained disabled and inactive, and no image or
message was sent by these attempts.

## Root Causes

1. The deploy-runner service used `release/current` as its working directory. COSCLI
   created `YYYYMMDD_HHMMSS/process.log` diagnostic trees there, contaminating an
   otherwise immutable release.
2. The release model exposed one global `sidecar/` slot while Huabaosi and QiWe require
   different compile-time feature sets. Replacing that slot with the QiWe artifact would
   remove Huabaosi image-generation capability.
3. The previous same-SHA recovery model treated Huabaosi-to-QiWe as a global profile
   switch. That model cannot keep both capabilities available and made runtime identity
   dependent on deployment order.
4. The Hermes callback bridge accepted runtime-local processor path and digest values
   that could remain pinned to an old direct release after `release/current` moved.

## Corrected Release Model

Every production release carries two independently reviewed binaries for the same
commit:

```text
<release>/sidecar/qintopia-message-sidecar
  huabaosi-production-adapter
  huabaosi-feishu-mirror-adapter

<release>/sidecar-profiles/qiwe-production/qintopia-message-sidecar
  qiwe-production-adapter
  huabaosi-feishu-mirror-adapter
```

Internal and Huabaosi units use the primary `sidecar/` binary. Only QiWe send
preflight/worker units, the Hermes image callback processor, and Xiaoman real-activity
evidence use the companion path. A combined production binary remains forbidden.

## Remediation

- Run COSCLI subprocesses from private temporary working directories.
- Give the root deploy runner `/var/lib/qintopia-agent-os-deploy` as its systemd state
  and working directory.
- Accept only the observed bounded `coscli_output` shape during existing-release repair;
  dry-run validates it without mutation, while apply moves it to a root-only quarantine.
  Any other extra release path fails.
- Fetch, checksum, inventory, and promote both production artifacts for one runtime SHA
  without changing the Huabaosi binary during legacy same-SHA companion repair.
- Render QiWe units against `sidecar-profiles/qiwe-production/` and all other units
  against `sidecar/`.
- Derive the production callback processor path and SHA-256 from the immutable QiWe
  companion manifest and `SHA256SUMS`; ignore stale runtime-local processor identity
  values and bind child release SHA values to resolved `release/current`.
- Build, upload, prune, and read-validate both runtime artifacts inside the existing
  production workflow jobs. No CI job is added.

## Validation Boundary

Repository tests cover release contamination, dry-run non-mutation, partial companion
rejection, Huabaosi hash preservation, systemd path separation, QiWe observation and
activation, callback identity derivation, and Xiaoman evidence binary identity.

The required final local gates are:

```bash
pnpm deploy:runner:check
pnpm deploy:contracts:check
pnpm deploy:systemd:check
pnpm deploy:release-model:check
pnpm test:qiwe
pnpm check:pr:auto
```

These checks do not publish a Release, submit a deploy request, edit production,
activate timers, process callbacks, write Postgres or Feishu, or call QiWe.

## First Rollout

The first reviewed Release remains a two-stage rollout because its initial request is
handled by the previous runner:

1. The owner publishes the reviewed Release. The workflow publishes both runtime
   artifacts and the deploy bundle for the same commit.
2. The owner submits one same-SHA follow-up using the existing release manifest's exact
   scope and restart targets. The new runner quarantines the known diagnostic shape,
   installs the companion artifact and updated runner units, and renders corrected QiWe
   bindings.
3. Operators run release-local disabled observations before any explicit external
   activation. Enabled observations and one real activity acceptance follow only after
   the persistent owner gates are reviewed.

## Rollback

Before external activation, rollback is to leave Huabaosi, Feishu mirror, QiWe send, and
callback enablement disabled. After activation, use the existing dedicated timer and
callback rollback scripts first. A failed promotion must preserve the previous `current`
target and restore any quarantined diagnostic directory or partially installed companion
path before exiting.

The owner-triggered `Rollback Production` workflow has no verified dual-runtime target
for this first rollout. Its historical `v0.2.0` candidate predates the companion layout
and is rejected before COS access or deploy-request creation. It may be re-enabled only
after a later published target's Huabaosi artifact, QiWe companion, and deploy bundle
are audited together.
