#!/usr/bin/env node

import assert from "node:assert/strict";

import {
  positiveXiaomanProductionCompletionClaimLines,
  requiredXiaomanProductionCompletionEvidenceRefs,
  validateXiaomanProductionCompletionClaimBoundary,
} from "./xiaoman-production-claim-boundary.mjs";

assert.deepEqual(
  positiveXiaomanProductionCompletionClaimLines(
    "Xiaoman remains not production-complete until evidence is retained."
  ),
  []
);

assert.deepEqual(
  positiveXiaomanProductionCompletionClaimLines(
    "小满尚未生产完成，不能发布 production-complete 口径。"
  ),
  []
);

assert.deepEqual(
  positiveXiaomanProductionCompletionClaimLines(
    "Xiaoman production-complete workflow is now fully usable in production."
  ),
  ["Xiaoman production-complete workflow is now fully usable in production."]
);

assert.deepEqual(
  positiveXiaomanProductionCompletionClaimLines(
    "Production-complete Xiaoman workflow is now available."
  ),
  ["Production-complete Xiaoman workflow is now available."]
);

assert.deepEqual(
  positiveXiaomanProductionCompletionClaimLines(
    "Xiaoman remains production-complete after the retained evidence bundle passed."
  ),
  ["Xiaoman remains production-complete after the retained evidence bundle passed."]
);

assert.deepEqual(
  positiveXiaomanProductionCompletionClaimLines(
    "Xiaoman production-ready workflow is now available."
  ),
  ["Xiaoman production-ready workflow is now available."]
);

assert.deepEqual(
  positiveXiaomanProductionCompletionClaimLines("小满生产闭环已完成。"),
  ["小满生产闭环已完成。"]
);

const missingEvidenceErrors = validateXiaomanProductionCompletionClaimBoundary({
  pullRequestBody:
    "This PR was generated with [Release Please]\n\nXiaoman production-complete workflow is now available.",
  changelog: "# Changelog\n\n## [1.2.3]\n\n- feat: Xiaoman production complete",
});
assert.equal(missingEvidenceErrors.length, 1);
for (const fragment of requiredXiaomanProductionCompletionEvidenceRefs) {
  assert.match(missingEvidenceErrors[0], new RegExp(escapeRegExp(fragment)));
}

const reversedOrderErrors = validateXiaomanProductionCompletionClaimBoundary({
  pullRequestBody:
    "This PR was generated with [Release Please]\n\nProduction-complete Xiaoman workflow is now available.",
  changelog: "# Changelog\n\n## [1.2.3]\n\n- feat: production-ready Xiaoman workflow",
});
assert.equal(reversedOrderErrors.length, 1);
assert.match(
  reversedOrderErrors[0],
  /Production-complete Xiaoman workflow is now available/
);
assert.match(reversedOrderErrors[0], /production-ready Xiaoman workflow/);

const missingArrivalEvidenceErrors = validateXiaomanProductionCompletionClaimBoundary({
  pullRequestBody: [
    "This PR was generated with [Release Please]",
    "Xiaoman production-complete workflow is now available.",
    "docs/plans/active/xiaoman-production-completion-gate.md",
    "tools/deploy/check-xiaoman-production-completion-evidence.mjs",
    "xiaoman-production-completion-evidence-v1",
    "owner-retained evidence",
  ].join("\n"),
  changelog: "# Changelog\n\n## [1.2.3]\n\n- feat: Xiaoman production complete",
});
assert.equal(missingArrivalEvidenceErrors.length, 1);
assert.match(
  missingArrivalEvidenceErrors[0],
  /check-xiaoman-qiwe-group-arrival-confirmation-evidence\.mjs/
);
assert.match(missingArrivalEvidenceErrors[0], /--qiwe-group-arrival-confirmation/);

assert.deepEqual(
  validateXiaomanProductionCompletionClaimBoundary({
    pullRequestBody: [
      "This PR was generated with [Release Please]",
      "Xiaoman production-complete workflow is now available.",
      ...requiredXiaomanProductionCompletionEvidenceRefs,
    ].join("\n"),
    changelog: "# Changelog\n\n## [1.2.3]\n\n- feat: Xiaoman production complete",
  }),
  []
);

function escapeRegExp(text) {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

console.log("Xiaoman production claim boundary test passed.");
