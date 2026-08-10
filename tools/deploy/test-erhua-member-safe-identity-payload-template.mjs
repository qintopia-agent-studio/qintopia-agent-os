#!/usr/bin/env node

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const builder = path.join(
  repoRoot,
  "tools/deploy/build-erhua-member-safe-identity-payload-template.mjs"
);
const checker = path.join(
  repoRoot,
  "tools/deploy/check-erhua-member-safe-identity-payload.mjs"
);
const tmpRoot = fs.mkdtempSync(
  path.join(os.tmpdir(), "erhua-member-safe-identity-template-")
);

try {
  const coveragePath = path.join(tmpRoot, "coverage.json");
  const templatePath = path.join(tmpRoot, "safe-identity-template.json");
  fs.writeFileSync(coveragePath, JSON.stringify(coverage(), null, 2), "utf8");

  let result = spawnSync(
    "node",
    [builder, "--coverage", coveragePath, "--output", templatePath],
    { cwd: repoRoot, encoding: "utf8" }
  );
  assert.equal(result.status, 0, result.stderr);
  const template = JSON.parse(fs.readFileSync(templatePath, "utf8"));
  assert.deepEqual(template, {
    identities: [
      {
        identity_key: "ab2c1a46c0af",
        safe_display_name: "",
        person_key: null,
        reason: "owner reviewed current-room member identity for recognition coverage",
      },
    ],
  });

  result = spawnSync("node", [checker, templatePath], {
    cwd: repoRoot,
    encoding: "utf8",
  });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /must be 2-40 characters/);

  template.identities[0].safe_display_name = "Paxon";
  const filledPath = path.join(tmpRoot, "safe-identity-filled.json");
  fs.writeFileSync(filledPath, JSON.stringify(template, null, 2), "utf8");
  result = spawnSync("node", [checker, filledPath], {
    cwd: repoRoot,
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr);

  const noSamplePath = path.join(tmpRoot, "no-sample.json");
  fs.writeFileSync(
    noSamplePath,
    JSON.stringify({ qiwe_room_potential_member_identities_unlinked_samples: [] }),
    "utf8"
  );
  result = spawnSync("node", [builder, "--coverage", noSamplePath], {
    cwd: repoRoot,
    encoding: "utf8",
  });
  assert.notEqual(result.status, 0);
  assert.match(
    result.stderr,
    /no unlinked current-room potential member identity samples/
  );

  const partialPath = path.join(tmpRoot, "partial-samples.json");
  fs.writeFileSync(
    partialPath,
    JSON.stringify({
      qiwe_room_potential_member_identities_unlinked: 2,
      qiwe_room_potential_member_identities_unlinked_samples: [
        {
          display_name: "[敏感数字]",
          identity_key: "ab2c1a46c0af",
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

console.log("Erhua member safe identity payload template test passed.");

function coverage() {
  return {
    qiwe_room_potential_member_identities_unlinked_samples: [
      {
        display_name: "[敏感数字]",
        identity_key: "ab2c1a46c0af",
        reason: "unsafe-potential-member-unlinked",
      },
      {
        display_name: "[敏感数字]",
        identity_key: "AB2C1A46C0AF",
        reason: "duplicate sample",
      },
    ],
  };
}
