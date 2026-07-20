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
