#!/usr/bin/env node

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const checker = path.join(
  repoRoot,
  "tools/deploy/check-erhua-member-safe-alias-payload.mjs"
);
const tmpRoot = fs.mkdtempSync(
  path.join(os.tmpdir(), "erhua-member-safe-alias-payload-")
);

try {
  let payload = writeCase("valid.json", validPayload());
  let result = runChecker(payload);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /payload check passed: 1 aliases/);

  payload = writeCase(
    "unknown-sensitive-field.json",
    validPayload({ chat_id: "group-secret" })
  );
  result = runChecker(payload);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /chat_id/);

  payload = writeCase("numeric-alias.json", validPayload({ alias: "000" }));
  result = runChecker(payload);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /numeric-only/);

  payload = writeCase("agent-alias.json", validPayload({ alias: "二花" }));
  result = runChecker(payload);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /system or test display name/);

  payload = writeCase(
    "phone-like-alias.json",
    validPayload({ alias: "Joey17336786728" })
  );
  result = runChecker(payload);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /forbidden sensitive fragment/);

  payload = writeCase("unknown-field.json", validPayload({ unexpected: "field" }));
  result = runChecker(payload);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /unknown field/);

  payload = writeCase("duplicate.json", {
    aliases: [
      validAlias(),
      { ...validAlias(), person_key: "FC2C1A46C0AF", alias: " 小白君 " },
    ],
  });
  result = runChecker(payload);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /duplicates/);

  payload = writeCase("duplicate-casefold-alias.json", {
    aliases: [
      { ...validAlias(), alias: "Paxon" },
      { ...validAlias(), person_key: "ab2c1a46c0af", alias: " paxon " },
    ],
  });
  result = runChecker(payload);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /duplicates another reviewed alias/);
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Erhua member safe alias payload test passed.");

function runChecker(payloadPath) {
  return spawnSync("node", [checker, payloadPath], {
    cwd: repoRoot,
    encoding: "utf8",
  });
}

function writeCase(name, content) {
  const payloadPath = path.join(tmpRoot, name);
  fs.writeFileSync(payloadPath, JSON.stringify(content, null, 2), "utf8");
  return payloadPath;
}

function validPayload(overrides = {}) {
  return {
    aliases: [
      {
        ...validAlias(),
        ...overrides,
      },
    ],
  };
}

function validAlias() {
  return {
    person_key: "fc2c1a46c0af",
    alias: "小白君",
    source_display_name: "000",
    reason: "owner reviewed safe member name for answer-context canary coverage",
  };
}
