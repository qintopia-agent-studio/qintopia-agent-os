# Xiaoman v0.2.7 Production Preflight Record

Date: 2026-07-14

## Scope

The read-only Xiaoman aggregate production preflight was run after GitHub Release
`v0.2.7` deployed commit `9ab54cd938d08188b3ab980c7b84f8737da26e5b` through Deploy
Production run `29299942402`.

## Findings

| Finding                                                                                                                   | Classification                     | Evidence                                                                                                                                                                 | Resolution                                                                                                                             |
| ------------------------------------------------------------------------------------------------------------------------- | ---------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------- |
| The image-generation starter service and timer both remained `LoadState=not-found` after the release-triggered deploy.    | Release-runner activation boundary | The deploy request included `qintopia-system-services`, but the first promotion containing the installer allowlist change was processed by the previous `v0.2.6` runner. | Completed by owner-approved same-SHA workflow run `29302981402`; the timer is now loaded, enabled, active, and waiting.                |
| The downstream observation rejected an empty evidence queue because the empty report did not contain the word `external`. | State-dependent smoke contract     | The report was otherwise `success=true`, `dry_run=true`, `apply_requested=false`, `action_status=no_claimable_evidence_request`, with zero artifact ids and previews.    | Require the external-boundary marker only for preview reports; require empty artifact lists for explicit no-claimable reports.         |
| The send-request starter observation rejected `no_eligible_approved_generated_images`.                                    | Stale smoke contract               | The timer checks completed before the JSON assertion; the current worker correctly gates intake on approved `generated_image` artifacts.                                 | Replace the obsolete visual-artifact empty status in the observation allowlist and cover both valid statuses with a fake-sidecar test. |

The same-SHA follow-up must reuse the original
`sidecar-runtime,deploy-bundle,hermes-plugins` release scope and
`qintopia-system-services,hermes-erhua,hermes-xiaoman,hermes-huabaosi` restart targets.
The promoter rejects an existing release when either field differs from its immutable
manifest.

## Follow-Up Production Evidence

- Owner-approved same-SHA Deploy Production run `29302981402` completed successfully.
- `current` still resolves to release SHA `9ab54cd938d08188b3ab980c7b84f8737da26e5b`.
- The image-generation starter timer reports `LoadState=loaded`, `ActiveState=active`,
  `SubState=waiting`, and `UnitFileState=enabled`.
- Its service runs the immutable release binary with
  `run-xiaoman-activity-image-generation-starter-worker --once --apply`; the independent
  read-only observation passed.
- Huabaosi production observation passed with generation disabled, no provider service
  or timer installed, and a zero-artifact dry-run.
- The aggregate v0.2.7 preflight still stops at the downstream empty-queue text
  assertion. This does not change the verified timer/provider boundaries, but the record
  remains Hold until the corrected smoke is released and rerun.

## Production Evidence

- `current` resolved to the expected `v0.2.7` release SHA.
- Signal, promotion starter, evidence, visual, and group send-ready timer observations
  passed.
- Huabaosi production observation passed with generation disabled, no provider service
  or timer installed, and `no_claimable_image_request` from `--once --dry-run`.
- Fixed-field worker checks reported no eligible signals, activity parents, evidence,
  visual, approved poster briefs, image-generation requests, or approved generated
  images.
- No apply smoke, final confirmation, send-ready execution, deploy command, Feishu
  write, QiWe call, provider/media request, or external publish was run during the
  preflight and diagnosis.

## Validation And Remaining Boundary

The preflight record remains Hold until the corrected observation contract is released
and rerun. The timer installation requirement is complete. The fixes are validated
locally with a fake sidecar/systemd test that covers empty and preview queue states and
rejects preview output without an external-boundary marker.

The repository pnpm shim could not verify the `pnpm@10.29.2` registry signature during
validation. No bypass flag was used. The fixed repository-local entrypoints
`node tools/deploy/check-deploy-contracts.mjs` and
`node tools/deploy/check-deploy-runner.mjs` were run directly and passed.

This work does not enable Huabaosi, Feishu, QiWe, provider/media endpoints, final
confirmation, send-ready execution, or external publishing.
