#!/usr/bin/env node

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const checker = path.join(
  repoRoot,
  "tools/deploy/check-erhua-member-safe-identity-payload.mjs"
);
const tmpRoot = fs.mkdtempSync(
  path.join(os.tmpdir(), "erhua-member-safe-identity-payload-")
);

try {
  let payload = writeCase("valid.json", validPayload());
  let result = runChecker(payload);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /payload check passed: 1 identities/);

  payload = writeCase(
    "unknown-sensitive-field.json",
    validPayload({ channel_user_id: "secret-user" })
  );
  result = runChecker(payload);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /channel_user_id/);

  payload = writeCase(
    "source-display-name.json",
    validPayload({ source_display_name: "000" })
  );
  result = runChecker(payload);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /source_display_name/);

  payload = writeCase("numeric-name.json", validPayload({ safe_display_name: "000" }));
  result = runChecker(payload);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /numeric-only/);

  payload = writeCase("agent-name.json", validPayload({ safe_display_name: "二花" }));
  result = runChecker(payload);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /system or test display name/);

  payload = writeCase(
    "phone-like-name.json",
    validPayload({ safe_display_name: "Joey17336786728" })
  );
  result = runChecker(payload);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /forbidden sensitive fragment/);

  payload = writeCase("bad-person-key.json", validPayload({ person_key: "not-a-key" }));
  result = runChecker(payload);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /person_key/);

  payload = writeCase("duplicate.json", {
    identities: [validIdentity(), { ...validIdentity(), identity_key: "AB2C1A46C0AF" }],
  });
  result = runChecker(payload);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /duplicates/);

  payload = writeCase("duplicate-safe-name.json", {
    identities: [
      validIdentity(),
      {
        ...validIdentity(),
        identity_key: "cd2c1a46c0af",
        safe_display_name: " paxon ",
      },
    ],
  });
  result = runChecker(payload);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /duplicates another reviewed safe name/);
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Erhua member safe identity payload test passed.");

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
    identities: [
      {
        ...validIdentity(),
        ...overrides,
      },
    ],
  };
}

function validIdentity() {
  return {
    identity_key: "ab2c1a46c0af",
    safe_display_name: "Paxon",
    person_key: null,
    reason: "owner reviewed current-room member identity for recognition coverage",
  };
}
