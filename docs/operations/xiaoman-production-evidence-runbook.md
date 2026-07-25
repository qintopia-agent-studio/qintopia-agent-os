# Xiaoman Production Evidence Runbook

Updated: 2026-07-24

This runbook is the owner-operated sequence for retaining the final Xiaoman production
evidence bundle after reviewed code has already shipped. It does not publish a Release,
does not auto-merge a Release Please PR, does not edit production servers outside the
reviewed deploy path, and does not replace the completion gate in
[`../plans/active/xiaoman-production-completion-gate.md`](../plans/active/xiaoman-production-completion-gate.md).

Use it only after the reviewed release is published and production deployment is an
explicit owner decision.

## Boundary

- Huabaosi first-record canary runs from the ordinary production artifact with
  `runtime_artifact_profile=huabaosi-production`.
- Real Xiaoman/QiWe delivery evidence runs from the separately deployed production
  artifact with `runtime_artifact_profile=qiwe-production`.
- The production database URL SHA-256 may stay the same across both phases.
- The Huabaosi production sidecar SHA-256 and QiWe production sidecar SHA-256 must be
  retained separately. Treating them as the same production binary is invalid.
- Retain only sanitized stdout/evidence files that pass the reviewed checkers below.
- Do not copy database URLs, tokens, callback payloads, request ids, group ids, message
  ids, provider responses, raw logs, or raw shell transcripts into the retained bundle.

## Required Inputs

Prepare these owner-reviewed facts before starting:

- published production release tag and commit SHA;
- Release Please PR number and exact merged head SHA for that published release;
- QiWe production enablement PR number and exact merged head SHA included in the same
  published release commit;
- production database URL SHA-256;
- Huabaosi production sidecar SHA-256 for the release/current binary used by the canary;
- QiWe production sidecar SHA-256 for the release/current binary used by the real
  activity evidence exporter;
- one approved pending Huabaosi `poster_brief` artifact UUID for the canary;
- one completed real Xiaoman production workflow root UUID after the QiWe delivery has
  visibly arrived in the intended group;
- one sanitized human confirmation file for the QiWe group arrival.

Before running any external production evidence step, rerun the repository-local
verification bundle from the reviewed commit:

```bash
node tools/deploy/check-xiaoman-production-evidence-chain-local.mjs
```

Do not continue to production evidence capture if this local check fails.

## 1. Huabaosi Production Canary

Run the one-shot Huabaosi production canary from the reviewed immutable release:

```bash
QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_ENABLE=1 \
QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_APPROVAL=approved-production-image-generation-canary \
QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_BRIEF_ARTIFACT_ID=<pending-poster-brief-uuid> \
QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_DATABASE_URL_SHA256=<approved-production-database-url-sha256> \
QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_RELEASE_SHA=<published-production-release-sha> \
QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_SIDECAR_SHA256=<approved-huabaosi-production-sidecar-sha256> \
deploy/sidecar/scripts/huabaosi-image-generation-production-canary-smoke.sh \
  > huabaosi-production-canary-output.txt

node tools/deploy/check-huabaosi-image-production-canary-evidence.mjs \
  huabaosi-production-canary-output.txt
```

Retain only the sanitized `huabaosi-production-canary-output.txt` after the checker
passes, then copy the reviewed fields into
[`../reports/templates/huabaosi-image-production-canary-evidence.md`](../reports/templates/huabaosi-image-production-canary-evidence.md).

## 2. QiWe Production Artifact Follow-Up Deploy

If the current production `release/current` still points at the ordinary Huabaosi
artifact, first publish the reviewed independent QiWe artifact to COS and dispatch the
owner-approved follow-up deploy with `runtime_artifact_profile=qiwe-production`.

Keep the deploy request/result evidence that proves the QiWe production artifact was the
active release-local binary before capturing the real activity evidence.

## 3. Real Xiaoman Production Activity Evidence

After one real Xiaoman activity has completed through QiWe delivery, export the
sanitized release-local evidence from the reviewed QiWe production binary:

```bash
QINTOPIA_XIAOMAN_REAL_ACTIVITY_PRODUCTION_SIDECAR_SHA256=<approved-qiwe-production-sidecar-sha256> \
QINTOPIA_XIAOMAN_REAL_ACTIVITY_PRODUCTION_DATABASE_URL_SHA256=<approved-production-database-url-sha256> \
qintopia-message-sidecar xiaoman-real-activity-production-evidence \
  --workflow-root-id <completed-xiaoman-activity-root-uuid> \
  > production-evidence-output.txt

node tools/deploy/check-xiaoman-real-activity-production-evidence.mjs \
  production-evidence-output.txt
```

Retain only the sanitized `production-evidence-output.txt` after the checker passes, and
copy reviewed fields into
[`../reports/templates/xiaoman-real-activity-production-evidence.md`](../reports/templates/xiaoman-real-activity-production-evidence.md).

## 4. QiWe Group Arrival Confirmation

After a human operator visually confirms the intended QiWe group received the image
message, validate the confirmation record against the sanitized production evidence:

```bash
node tools/deploy/check-xiaoman-qiwe-group-arrival-confirmation-evidence.mjs \
  production-evidence-output.txt \
  qiwe-group-arrival-confirmation-output.txt
```

Retain only the sanitized confirmation file and copy reviewed fields into
[`../reports/templates/xiaoman-qiwe-group-arrival-confirmation-evidence.md`](../reports/templates/xiaoman-qiwe-group-arrival-confirmation-evidence.md).

## 5. Completion Manifest

After the Huabaosi canary, real activity evidence, and group-arrival confirmation have
all passed their reviewed checkers, build the final non-secret completion manifest:

```bash
node tools/deploy/build-xiaoman-production-completion-manifest.mjs \
  --release-please-pr-number <release-please-pr-number> \
  --release-please-head-sha <release-please-head-sha> \
  --release-tag <published-release-tag> \
  --released-commit-sha <published-release-commit-sha> \
  --qiwe-production-enablement-pr-number <qiwe-production-enablement-pr-number> \
  --qiwe-production-enablement-head-sha <qiwe-production-enablement-head-sha> \
  --huabaosi-production-canary huabaosi-production-canary-output.txt \
  --production-real-activity production-evidence-output.txt \
  --qiwe-group-arrival-confirmation qiwe-group-arrival-confirmation-output.txt \
  --output completed-xiaoman-production-completion-evidence.json

node tools/deploy/check-xiaoman-production-completion-evidence.mjs \
  --manifest completed-xiaoman-production-completion-evidence.json \
  --staging-runtime-readiness <staging-runtime-readiness-output.txt> \
  --huabaosi-staging <huabaosi-staging-output.txt> \
  --qiwe-staging <qiwe-staging-output.txt> \
  --huabaosi-production-canary huabaosi-production-canary-output.txt \
  --production-real-activity production-evidence-output.txt \
  --qiwe-group-arrival-confirmation qiwe-group-arrival-confirmation-output.txt
```

To reduce handoff mistakes during the owner-operated final step, you may run the
reviewed one-shot helper instead of typing the builder and checker separately:

```bash
node tools/deploy/finalize-xiaoman-production-completion-evidence.mjs \
  --release-please-pr-number <release-please-pr-number> \
  --release-please-head-sha <release-please-head-sha> \
  --release-tag <published-release-tag> \
  --released-commit-sha <published-release-commit-sha> \
  --qiwe-production-enablement-pr-number <qiwe-production-enablement-pr-number> \
  --qiwe-production-enablement-head-sha <qiwe-production-enablement-head-sha> \
  --staging-runtime-readiness <staging-runtime-readiness-output.txt> \
  --huabaosi-staging <huabaosi-staging-output.txt> \
  --qiwe-staging <qiwe-staging-output.txt> \
  --huabaosi-production-canary huabaosi-production-canary-output.txt \
  --production-real-activity production-evidence-output.txt \
  --qiwe-group-arrival-confirmation qiwe-group-arrival-confirmation-output.txt \
  --output completed-xiaoman-production-completion-evidence.json
```

Retain the final manifest only after the completion checker passes, then use
[`../reports/templates/xiaoman-production-completion-evidence.json`](../reports/templates/xiaoman-production-completion-evidence.json)
as the non-secret preserved record.

## Hold Conditions

Stop and do not continue to the next step if any of these occur:

- the published Release tag does not point to current `origin/master`;
- the Huabaosi canary checker fails;
- the real activity exporter fails to prove `runtime_artifact_profile=qiwe-production`;
- the retained evidence suggests Huabaosi and QiWe used the same binary by assumption
  rather than by reviewed artifact identity;
- the group-arrival confirmation checker fails;
- the completion checker fails; or
- any retained output contains raw secrets or fields outside the reviewed templates.

## Completion Standard

This runbook completes only when all retained files above have passed their reviewed
checkers and the final completion manifest proves:

- Huabaosi canary evidence retained `runtime_artifact_profile=huabaosi-production`;
- real Xiaoman/QiWe evidence retained `runtime_artifact_profile=qiwe-production`;
- both artifacts are bound to the same published production release commit;
- both production sidecar SHA-256 values are retained separately; and
- the final checker passes against the full sanitized evidence bundle.
