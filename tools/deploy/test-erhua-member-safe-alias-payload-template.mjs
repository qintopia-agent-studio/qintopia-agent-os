#!/usr/bin/env node

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const builder = path.join(
  repoRoot,
  "tools/deploy/build-erhua-member-safe-alias-payload-template.mjs"
);
const checker = path.join(
  repoRoot,
  "tools/deploy/check-erhua-member-safe-alias-payload.mjs"
);
const tmpRoot = fs.mkdtempSync(
  path.join(os.tmpdir(), "erhua-member-safe-alias-template-")
);

try {
  const coveragePath = path.join(tmpRoot, "coverage.json");
  const templatePath = path.join(tmpRoot, "safe-alias-template.json");
  fs.writeFileSync(coveragePath, JSON.stringify(coverage(), null, 2), "utf8");

  let result = spawnSync(
    "node",
    [builder, "--coverage", coveragePath, "--output", templatePath],
    { cwd: repoRoot, encoding: "utf8" }
  );
  assert.equal(result.status, 0, result.stderr);
  const template = JSON.parse(fs.readFileSync(templatePath, "utf8"));
  assert.deepEqual(template, {
    aliases: [
      {
        person_key: "fc2c1a46c0af",
        alias: "",
        source_display_name: "000",
        reason: "owner reviewed safe member name for answer-context canary coverage",
      },
    ],
  });

  result = spawnSync("node", [checker, templatePath], {
    cwd: repoRoot,
    encoding: "utf8",
  });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /must be 2-40 characters/);

  template.aliases[0].alias = "小白君";
  const filledPath = path.join(tmpRoot, "safe-alias-filled.json");
  fs.writeFileSync(filledPath, JSON.stringify(template, null, 2), "utf8");
  result = spawnSync("node", [checker, filledPath], {
    cwd: repoRoot,
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr);

  const noSamplePath = path.join(tmpRoot, "no-sample.json");
  fs.writeFileSync(
    noSamplePath,
    JSON.stringify({ linked_people_without_answer_context_canary_spec_samples: [] }),
    "utf8"
  );
  result = spawnSync("node", [builder, "--coverage", noSamplePath], {
    cwd: repoRoot,
    encoding: "utf8",
  });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /no missing safe alias samples/);

  const partialPath = path.join(tmpRoot, "partial-samples.json");
  fs.writeFileSync(
    partialPath,
    JSON.stringify({
      linked_people_without_answer_context_canary_spec: 2,
      linked_people_without_answer_context_canary_spec_samples: [
        {
          display_name: "000",
          person_key: "fc2c1a46c0af",
        },
      ],
    }),
    "utf8"
  );
  result = spawnSync("node", [builder, "--coverage", partialPath], {
    cwd: repoRoot,
    encoding: "utf8",
  });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /would cover 1\/2/);
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Erhua member safe alias payload template test passed.");

function coverage() {
  return {
    linked_people_without_answer_context_canary_spec_samples: [
      {
        display_name: "000",
        person_key: "fc2c1a46c0af",
        reason: "missing_safe_answer_context_canary_name",
      },
      {
        display_name: "000",
        person_key: "FC2C1A46C0AF",
        reason: "duplicate sample",
      },
    ],
  };
}
