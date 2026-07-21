#!/usr/bin/env node

import assert from "node:assert/strict";

import {
  extractPatchEntries,
  patchEntriesMatchAllowedPaths,
} from "./hermes-patch-paths.mjs";

const allowedPaths = ["gateway/platforms/wecom.py", "tests/gateway/test_wecom.py"];
const validPatch = [
  "diff --git a/gateway/platforms/wecom.py b/gateway/platforms/wecom.py",
  "diff --git a/tests/gateway/test_wecom.py b/tests/gateway/test_wecom.py",
].join("\n");

assert.equal(
  patchEntriesMatchAllowedPaths(extractPatchEntries(validPatch), allowedPaths),
  true
);
assert.equal(
  patchEntriesMatchAllowedPaths(
    extractPatchEntries(
      validPatch.replace(
        "b/tests/gateway/test_wecom.py",
        "b/gateway/platforms/webhook.py"
      )
    ),
    allowedPaths
  ),
  false
);
assert.equal(
  patchEntriesMatchAllowedPaths(
    extractPatchEntries(
      validPatch.replace(
        "a/tests/gateway/test_wecom.py",
        "a/gateway/platforms/webhook.py"
      )
    ),
    allowedPaths
  ),
  false
);

console.log("Hermes patch path tests passed.");
