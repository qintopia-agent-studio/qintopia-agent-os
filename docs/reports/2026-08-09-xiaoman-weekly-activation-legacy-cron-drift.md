# Xiaoman Weekly Activation Legacy Cron Drift

Date: 2026-08-09

## Evidence

Release `v0.2.85` deployed successfully to production at
`0b665bf7745dae02fe8e6a2c252b4dc01d066055`. The reviewed `Activate Production Timers`
workflow then failed closed before enabling the Xiaoman weekly loop timer because
`xiaoman-legacy-cron-observation-smoke.sh` found one runtime cron declaration in the
live Xiaoman Hermes cron file.

A release-local retirement attempt also failed closed because the live cron file hash no
longer matched the hash baked into `retire-xiaoman-legacy-cron-production.sh`.

After `v0.2.86` deployed the reviewed hash update and signed retirement workflow, the
server-side retirement request failed closed again because the deploy-runner unit kept
`ProtectHome=read-only` and did not grant `ReadWritePaths` to the fixed Xiaoman cron
directory. The retirement script attempted to create the reviewed backup and received
`EROFS` before replacing the live cron file.

Read-only production metadata observed after the failed activation:

```json
{
  "sha256": "41347af48cbb62010be3f530f0fa7d4dfa28f0e661f4fd48fbc0a5467b484c08",
  "decl_count": 1,
  "mode": "0o600",
  "size": 4422
}
```

The cron content was not copied into git or logs.

## Root Cause

The Xiaoman legacy Hermes cron file drifted after the retirement script was reviewed.
The script correctly refused to retire an unreviewed hash, and the activation workflow
correctly refused to enable a replacement systemd timer while the legacy cron still had
a runnable declaration.

The follow-up retirement request also showed that the deployed runner sandbox allowed
Erhua profile writes but not the fixed Xiaoman cron directory, so the reviewed
retirement script could not create its backup or replacement file.

## Resolution

Update the retirement script's expected previous SHA-256 to the newly observed
production hash:

`41347af48cbb62010be3f530f0fa7d4dfa28f0e661f4fd48fbc0a5467b484c08`

Update the deploy-runner service to retain `ProtectHome=read-only` while adding only
`/home/ubuntu/.hermes/profiles/xiaoman/cron` to `ReadWritePaths`. Do not grant whole
Xiaoman profile write access.

After the patched release is deployed, run the release-local Xiaoman legacy cron
retirement script and re-run the weekly timer activation workflow for:

```text
xiaoman-weekly-recruitment,xiaoman-weekly-plan-confirmation,xiaoman-weekly-preview
```

## Validation

- `node tools/deploy/test-xiaoman-legacy-cron-retirement.mjs`
- `node tools/deploy/check-deploy-contracts.mjs`
- `node tools/deploy/check-deploy-runner.mjs`

## Remaining Boundary

Retirement still requires the explicit owner approval value and still accepts no cron
path override. It creates a backup, replaces the live cron with retired metadata, and
does not execute external calls or print cron content.
