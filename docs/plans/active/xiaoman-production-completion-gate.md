# Xiaoman Production Completion Gate

Updated: 2026-07-20

## Goal

Prevent an infrastructure-only Release from being treated as a usable Xiaoman production
completion. A Release may still ship reviewed staging, deploy, or activation tooling,
but it must not be described as "Xiaoman production complete" until the real external
workflow has passed the gates below.

## Current Boundary

The AgentOS-only Xiaoman orchestration path is production schedulable through internal
send-ready state. It is not a complete business workflow until the reviewed production
evidence path proves Huabaosi real image generation, human approval, QiWe final
delivery, and one real end-to-end activity.

The reviewed code gates for route binding, Huabaosi canary evidence, QiWe activation
hash binding, QiWe group-arrival confirmation, release-claim protection, send-ready
evidence binding, and final completion-evidence binding are merged in `#226` through
`#233`. They are still code readiness only until a manual Release Please decision,
immutable production deployment, owner activation, and owner-retained real-activity
evidence prove the full workflow.

## Release Classification

Classify each Xiaoman-adjacent Release before merging its Release Please PR:

- `infrastructure`: release, deploy, staging, provisioning, smoke, or guardrail changes
  that do not make the full production workflow usable.
- `activation-ready`: production artifacts and activation scripts are present, but the
  external runtime remains disabled until owner-provided configuration and evidence
  pass.
- `production-complete`: all gates in [Completion Gates](#completion-gates) are
  satisfied and the Release notes identify the retained evidence.

Only `production-complete` may be described as Xiaoman fully usable in production.

## Completion Gates

All gates are required before a Release is called Xiaoman production complete:

1. Release Please validation passes on the exact release PR head, including the manual
   CI dispatch required for bot-authored release PRs, and the completion manifest binds
   the published production Release commit to the deployed production evidence.
2. The staging sidecar artifact is built and provisioned under the fixed immutable
   staging release root with reviewed release SHA, sidecar SHA-256, and rollback owner.
3. Huabaosi staging generation produces one reviewed final JPEG and retains only the
   sanitized evidence allowed by the staging template.
4. QiWe staging preflight, upload, and callback/send phases pass against one isolated
   send-ready work item and one trusted memory-only callback stream.
5. The cross-flow evidence checker proves the Huabaosi final JPEG `content_hash` equals
   the QiWe `artifact_content_hash`.
6. A separate QiWe production enablement PR adds reviewed listener/service/timer,
   observation, rollback, exact allowlists, and production feature boundaries, and the
   final manifest records that the reviewed enablement was included in the same
   production Release commit used for the real-activity evidence.
7. Huabaosi production generation and Feishu mirror activation pass release-local
   observation, explicit activation, and first-record canary evidence retained as a
   separate sanitized output.
8. One real Xiaoman activity is observed from signal intake through image generation,
   human approval, send-ready, QiWe group-send arrival, and sanitized production
   evidence retention. The QiWe group arrival must also have a separate sanitized human
   confirmation record bound to the same send-ready work item, generated-image artifact,
   and `artifact_content_hash`.
9. The final production completion checker passes against the owner-retained evidence
   bundle:

   ```bash
   node tools/deploy/check-xiaoman-production-completion-evidence.mjs \
     --manifest <completed-xiaoman-production-completion-evidence.json> \
     --staging-runtime-readiness <staging-runtime-readiness-output.txt> \
     --huabaosi-staging <huabaosi-staging-output.txt> \
     --qiwe-staging <qiwe-staging-output.txt> \
     --huabaosi-production-canary <huabaosi-production-canary-output.txt> \
     --production-real-activity <production-real-activity-output.txt> \
     --qiwe-group-arrival-confirmation <qiwe-group-arrival-confirmation-output.txt>
   ```

## Non-Completion Cases

These are useful but not completion:

- local fake-server, fake-sidecar, disposable PostgreSQL, or CI-only validation;
- staging artifact builder or staging provisioner changes without a server-side
  owner-approved staging run;
- Huabaosi production units installed but timer disabled;
- Feishu mirror units installed but timer disabled or first-record evidence missing;
- QiWe staging evidence without a reviewed production enablement PR;
- a Release that deploys internal timers but still leaves QiWe delivery disabled;
- Xiaoman text announcement MVP output, an approved `text_announcement`, or a prepared
  text `group_message_request` without the image/send-ready/QiWe arrival evidence
  bundle;
- passing PR CI or PR-Agent review without the owner-retained production evidence bundle
  above.

## Next Work

1. Let Release Please prepare the next release PR from current `master`; run manual
   Release Please validation on the exact release PR head before any release decision.
2. Publish and deploy only an owner-approved immutable production release whose release
   SHA, sidecar SHA-256, and database URL SHA-256 are retained for the evidence
   commands.
3. Use the staging artifact builder and provisioner to prepare the fixed staging
   runtime, then retain the staging runtime readiness output.
4. Run Huabaosi and QiWe staging evidence smokes, then the cross-flow checker.
5. Confirm QiWe production enablement and Huabaosi production activation evidence are
   merged, deployed, and owner-approved.
6. After the Huabaosi one-shot production canary creates one pending Feishu-backed JPEG,
   retain its sanitized output and validate it before using it as first-record evidence:

   ```bash
   node tools/deploy/check-huabaosi-image-production-canary-evidence.mjs \
     <production-canary-output.txt>
   ```

7. Process one real production Xiaoman activity and retain sanitized real-activity
   evidence with the release-local exporter:

   ```bash
   QINTOPIA_XIAOMAN_REAL_ACTIVITY_PRODUCTION_SIDECAR_SHA256=<approved-production-sidecar-sha256> \
   QINTOPIA_XIAOMAN_REAL_ACTIVITY_PRODUCTION_DATABASE_URL_SHA256=<approved-production-database-url-sha256> \
   qintopia-message-sidecar xiaoman-real-activity-production-evidence \
     --workflow-root-id <completed-xiaoman-activity-root-uuid> > production-evidence-output.txt
   node tools/deploy/check-xiaoman-real-activity-production-evidence.mjs <production-evidence-output.txt>
   node tools/deploy/check-xiaoman-qiwe-group-arrival-confirmation-evidence.mjs \
     <production-evidence-output.txt> \
     <qiwe-group-arrival-confirmation-output.txt>
   ```

8. Build the non-secret completion manifest from the retained Huabaosi canary,
   real-activity, QiWe group-arrival evidence, and GitHub PR state, then run the full
   checker before changing any Release classification to `production-complete`. The
   builder must run where `gh` can verify the Release Please PR, QiWe production
   enablement PR, and published release commit facts:

   ```bash
   node tools/deploy/build-xiaoman-production-completion-manifest.mjs \
     --release-please-pr-number <release-please-pr-number> \
     --release-please-head-sha <release-please-head-sha> \
     --release-tag <published-release-tag> \
     --released-commit-sha <published-release-commit-sha> \
     --qiwe-production-enablement-pr-number <qiwe-production-enablement-pr-number> \
     --qiwe-production-enablement-head-sha <qiwe-production-enablement-head-sha> \
     --huabaosi-production-canary <production-canary-output.txt> \
     --production-real-activity <production-evidence-output.txt> \
     --qiwe-group-arrival-confirmation <qiwe-group-arrival-confirmation-output.txt> \
     --output <completed-xiaoman-production-completion-evidence.json>
   ```

## Production Boundary

This plan changes release decision criteria only. It does not publish a Release, deploy
to production, install or enable timers, write Postgres or Feishu, call providers or
QiWe, process callbacks, or send externally.
