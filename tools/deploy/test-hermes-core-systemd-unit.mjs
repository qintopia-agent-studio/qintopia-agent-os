#!/usr/bin/env node

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

const repoRoot = process.cwd();
const renderer = path.join(
  repoRoot,
  "tools/deploy/render-hermes-core-systemd-unit.mjs"
);
const profiles = [
  ["default", "hermes-gateway.service"],
  ["erhua", "hermes-gateway-erhua.service"],
  ["guanerye", "hermes-gateway-guanerye.service"],
  ["huabaosi", "hermes-gateway-huabaosi.service"],
  ["silaoshi", "hermes-gateway-silaoshi.service"],
  ["wenyuange", "hermes-gateway-wenyuange.service"],
  ["xiaoman", "hermes-gateway-xiaoman.service"],
];

for (const [profile, service] of profiles) {
  const result = spawnSync(process.execPath, [renderer, profile], {
    cwd: repoRoot,
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(
    result.stdout,
    `[Service]\nExecStart=\nExecStart=/usr/local/libexec/qintopia-hermes-core-launcher --profile ${profile} gateway run --replace\nEnvironment=HERMES_HOME=/home/ubuntu/.hermes\n`
  );
  assert.match(service, /^hermes-gateway/);
}

const invalid = spawnSync(process.execPath, [renderer, "unknown"], {
  cwd: repoRoot,
  encoding: "utf8",
});
assert.notEqual(invalid.status, 0);
assert.match(invalid.stderr, /hermes_core_systemd_unit_error=profile_registry_invalid/);
assert.equal(
  fs.existsSync(path.join(repoRoot, "runtime/hermes/profile-registry.yaml")),
  true
);

console.log("Hermes core systemd unit renderer test passed.");
