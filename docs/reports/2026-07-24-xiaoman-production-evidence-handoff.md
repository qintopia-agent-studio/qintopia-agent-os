# 2026-07-24 Xiaoman Production Evidence Handoff

## Purpose

This note is the handoff package for the reviewed Xiaoman production evidence chain work
as of Friday, July 24, 2026. It is intended for the next owner-operated executor of the
production evidence path.

The reviewed repository work is substantially in place. The remaining work is primarily
production execution and evidence retention, not a large remaining repository
implementation gap.

## What Is Ready In The Repository

The current branch/worktree includes the reviewed repository-side implementation for:

- the independent QiWe production enablement chain;
- the Huabaosi first-record production image canary evidence chain;
- the retained real-activity production evidence checker;
- the QiWe group-arrival confirmation checker;
- the final completion manifest builder and full completion-evidence checker; and
- the one-shot completion finalizer:
  `tools/deploy/finalize-xiaoman-production-completion-evidence.mjs`.

Repository-local verification entrypoint:

```bash
node tools/deploy/check-xiaoman-production-evidence-chain-local.mjs
```

## What Still Is Not Done

The remaining work is external and owner-operated:

1. Run the real Huabaosi production canary and retain sanitized evidence.
2. If `release/current` still points to the ordinary Huabaosi artifact, perform the
   reviewed follow-up deploy for `runtime_artifact_profile=qiwe-production`.
3. Run one real Xiaoman production activity and export sanitized real-activity evidence.
4. Retain the human QiWe group-arrival confirmation for that same activity.
5. Build and validate the final completion manifest from the retained staging and
   production evidence bundle.

These steps are required before any `production-complete` claim.

## What To Hand Over

Give the next executor these reviewed inputs:

- the code branch or PR containing this repository work;
- the reviewed runbook:
  [`../operations/xiaoman-production-evidence-runbook.md`](../operations/xiaoman-production-evidence-runbook.md);
- the completion gate:
  [`../plans/active/xiaoman-production-completion-gate.md`](../plans/active/xiaoman-production-completion-gate.md);
- the latest local verification report:
  [`2026-07-24-xiaoman-production-evidence-chain-local-verification.md`](2026-07-24-xiaoman-production-evidence-chain-local-verification.md);
- the visual production test map:
  [`2026-07-24-xiaoman-production-test-map.html`](2026-07-24-xiaoman-production-test-map.html);
- the ready-to-use PR body:
  [`2026-07-24-xiaoman-production-evidence-pr-body.md`](2026-07-24-xiaoman-production-evidence-pr-body.md);
- the PR title/reviewer notes:
  [`2026-07-24-xiaoman-production-evidence-pr-notes.md`](2026-07-24-xiaoman-production-evidence-pr-notes.md).

If this repository work is not yet merged, the handoff must include the review/merge
path for the branch before the production evidence sequence is treated as canonical.

## Execution Order

1. Re-run the repository-local verification bundle from the reviewed commit:

   ```bash
   node tools/deploy/check-xiaoman-production-evidence-chain-local.mjs
   ```

2. Follow the production evidence runbook for:
   - the Huabaosi first-record canary;
   - the QiWe production follow-up deploy if needed;
   - the real Xiaoman/QiWe production evidence export; and
   - the QiWe group-arrival human confirmation.

3. Finalize the retained evidence bundle:

   ```bash
   pnpm deploy:xiaoman-production-evidence:finalize -- \
     --release-please-pr-number <release-please-pr-number> \
     --release-please-head-sha <release-please-head-sha> \
     --release-tag <published-release-tag> \
     --released-commit-sha <published-release-commit-sha> \
     --qiwe-production-enablement-pr-number <qiwe-production-enablement-pr-number> \
     --qiwe-production-enablement-head-sha <qiwe-production-enablement-head-sha> \
     --staging-runtime-readiness <staging-runtime-readiness-output.txt> \
     --huabaosi-staging <huabaosi-staging-output.txt> \
     --qiwe-staging <qiwe-staging-output.txt> \
     --huabaosi-production-canary <huabaosi-production-canary-output.txt> \
     --production-real-activity <production-evidence-output.txt> \
     --qiwe-group-arrival-confirmation <qiwe-group-arrival-confirmation-output.txt> \
     --output <completed-xiaoman-production-completion-evidence.json>
   ```

## Important Boundary

- Repository readiness is not the same thing as production completion.
- Huabaosi canary evidence must retain `runtime_artifact_profile=huabaosi-production`.
- Real Xiaoman/QiWe production evidence must retain
  `runtime_artifact_profile=qiwe-production`.
- The Huabaosi production sidecar SHA-256 and QiWe production sidecar SHA-256 must be
  retained separately.
- Do not copy secrets, database URLs, raw provider responses, callback payloads, raw
  logs, raw shell transcripts, or raw group ids into the retained bundle or the handoff
  note.

## Current Status Summary

As of Friday, July 24, 2026:

- the repository-side code path is effectively in place;
- the repository-local verification bundle is green; and
- the blocking gap is the owner-operated production evidence capture, not a large
  remaining code implementation gap.
