# 2026-07-24 Xiaoman Production Evidence Chain Local Verification

## Scope

This report captures the local repository verification completed on Friday, July 24,
2026 for the reviewed Xiaoman production evidence chain work:

- independent QiWe production enablement chain; and
- Huabaosi first-record production image canary evidence chain.

It is repository-local verification only. It does not prove that production evidence has
been captured, that a QiWe production follow-up deploy has run, or that Xiaoman is
production-complete.

Convenience rerun command:

- `node tools/deploy/check-xiaoman-production-evidence-chain-local.mjs`

## Local Verification Result

The current workspace passed the repository-local checks needed to support the reviewed
production evidence chain:

- `node tools/deploy/check-deploy-contracts.mjs`
- `node tools/deploy/check-deploy-runner.mjs`
- `node tools/deploy/check-xiaoman-production-evidence-chain-local.mjs`
- `node tools/deploy/test-sidecar-artifact-build-boundary.mjs`
- `node tools/deploy/test-build-sidecar-artifact.mjs`
- `node tools/deploy/test-build-qiwe-production-sidecar-artifact.mjs`
- `node tools/deploy/test-fetch-cos-artifact-permissions.mjs`
- `node tools/deploy/test-fetch-staging-sidecar-artifact.mjs`
- `node tools/deploy/check-xiaoman-preflight-readiness.mjs`
- `node tools/deploy/test-xiaoman-legacy-cron-observation.mjs`
- `node tools/deploy/test-staging-runtime-prerequisite-observation.mjs`
- `node tools/deploy/test-staging-runtime-values-observation.mjs`
- `node tools/deploy/test-staging-runtime-env-render.mjs`
- `node tools/deploy/test-staging-runtime-readiness-evidence.mjs`
- `node tools/deploy/test-huabaosi-image-staging-readiness.mjs`
- `node tools/deploy/test-huabaosi-image-staging-smoke.mjs`
- `node tools/deploy/test-qiwe-image-staging-readiness.mjs`
- `node tools/deploy/test-qiwe-image-staging-smoke.mjs`
- `node tools/deploy/test-qiwe-image-production-observation.mjs`
- `node tools/deploy/test-qiwe-image-production-activation.mjs`
- `node tools/deploy/test-qiwe-image-callback-bridge-production-observation.mjs`
- `node tools/deploy/test-qiwe-image-callback-bridge-production-activation.mjs`
- `node tools/deploy/test-huabaosi-image-production-canary.mjs`
- `node tools/deploy/test-huabaosi-image-production-canary-evidence.mjs`
- `node tools/deploy/test-xiaoman-image-send-staging-evidence.mjs`
- `node tools/deploy/test-xiaoman-real-activity-production-evidence.mjs`
- `node tools/deploy/test-xiaoman-qiwe-group-arrival-confirmation-evidence.mjs`
- `node tools/deploy/test-xiaoman-production-completion-manifest-builder.mjs`
- `node tools/deploy/test-xiaoman-production-completion-evidence.mjs`
- `node tools/deploy/test-finalize-xiaoman-production-completion-evidence.mjs`
- `node tools/agents/pr-doctor.mjs docs/reports/2026-07-24-xiaoman-production-evidence-pr-body.md`
- `cargo test --manifest-path runtime/sidecar/Cargo.toml xiaoman_real_activity_evidence`

## Notable Fix During Verification

The local verification uncovered one real repository gap:

- `tools/deploy/test-xiaoman-qiwe-group-arrival-confirmation-evidence.mjs` still emitted
  production evidence fixtures without the reviewed
  `runtime_artifact_profile=qiwe-production` fact.

The fixture was updated so the test now matches the current production evidence
contract, and `tools/deploy/check-deploy-contracts.mjs` now guards that expectation.

The local verification also re-confirmed the release-local artifact-permission and
metadata boundary: reviewed Huabaosi and `qiwe-production` sidecar artifacts keep
`artifact-manifest.json`, `SHA256SUMS`, and packaged archives mode `0444`, preserve the
sidecar binary at `0755`, reject mixed QiWe features in the Huabaosi production
artifact, and accept the reviewed independent `qiwe-production` artifact.

The same rerun now also verifies the reviewed one-shot completion helper:
`tools/deploy/finalize-xiaoman-production-completion-evidence.mjs`. That helper reduces
manual handoff risk during the last owner-operated step by chaining manifest build and
full completion-evidence validation from the retained staging and production files.

## pnpm Verification Note

`pnpm deploy:contracts:check` did not run directly in this environment because the local
`pnpm@10.29.2` registry signature could not be verified on July 24, 2026. In line with
repository rules, the verification did not set `pmOnFail=ignore`. Instead, the exact
repository-owned Node entrypoints from `package.json` were executed directly.

This is a local tooling verification issue, not evidence that the reviewed deploy or
evidence-chain code is failing.

## Current Repository Conclusion

As of July 24, 2026, the repository-local implementation for the reviewed Xiaoman
production evidence chain is effectively in place:

- Huabaosi production canary evidence is bound to the ordinary
  `runtime_artifact_profile=huabaosi-production` artifact.
- real Xiaoman/QiWe production evidence is bound to the separate
  `runtime_artifact_profile=qiwe-production` artifact.
- the final completion manifest and checker preserve the two production sidecar SHA-256
  values separately while binding both artifacts to the same released commit SHA.

The remaining work is no longer a large repository implementation gap. It is the
owner-operated capture of real production evidence.

## Remaining External Evidence Gates

The following still require real external execution and retained sanitized outputs:

1. Run the real Huabaosi first-record production canary and retain the sanitized
   evidence output.
2. If `release/current` still points at the ordinary Huabaosi artifact, publish and
   deploy the reviewed `qiwe-production` artifact through the owner-approved follow-up
   production deploy path.
3. Run one real Xiaoman activity through the reviewed QiWe production path and export
   the sanitized real-activity production evidence.
4. Retain the human QiWe group-arrival confirmation evidence for that same activity.
5. Build and validate the final Xiaoman production completion manifest against the full
   sanitized evidence bundle, either with the separate builder/checker pair or the
   reviewed one-shot finalizer.

Follow the reviewed runbook: `docs/operations/xiaoman-production-evidence-runbook.md`.
