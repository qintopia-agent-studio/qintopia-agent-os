#!/usr/bin/env node

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const testDirectory = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(testDirectory, "../..");
const checker = path.join(repoRoot, "tools/deploy/check-hermes-wecom-readiness.mjs");
const tmpRoot = fs.mkdtempSync(
  path.join(os.tmpdir(), "qintopia-hermes-wecom-readiness-")
);
const stagingHome = path.join(tmpRoot, "hermes");
const markerPath = path.join(tmpRoot, "source-marker");
const secretValues = [
  "fixture-bot-value",
  "fixture-secret-value",
  "fixture-target-value",
  "config-sensitive-fixture",
  tmpRoot,
  stagingHome,
  markerPath,
];

const expectedStates = new Map([
  ["default", true],
  ["erhua", false],
  ["guanerye", true],
  ["huabaosi", true],
  ["silaoshi", true],
  ["wenyuange", false],
  ["xiaoman", true],
]);

function writeProfile(profile, enabled) {
  const profileDirectory = path.join(stagingHome, "profiles", profile);
  fs.mkdirSync(profileDirectory, { recursive: true });
  fs.writeFileSync(
    path.join(profileDirectory, "config.yaml"),
    [
      "profile_fixture: config-sensitive-fixture",
      "platforms:",
      "  wecom:",
      `    enabled: ${enabled}`,
      "    extra:",
      "      fixture_note: config-sensitive-fixture",
      "",
    ].join("\n"),
    "utf8"
  );
  fs.writeFileSync(
    path.join(profileDirectory, ".env"),
    [
      "WECOM_BOT_ID=fixture-bot-value",
      "WECOM_SECRET=fixture-secret-value",
      "WECOM_HOME_CHANNEL=fixture-target-value",
      `QINTOPIA_SOURCE_TEST="$(touch ${markerPath})"`,
      "",
    ].join("\n"),
    "utf8"
  );
}

function createFixture() {
  fs.mkdirSync(path.join(stagingHome, "profiles"), { recursive: true });
  for (const [profile, enabled] of expectedStates) {
    writeProfile(profile, enabled);
  }
}

function snapshotTree(root) {
  const entries = [];
  const visit = (current) => {
    for (const name of fs.readdirSync(current).sort()) {
      const filePath = path.join(current, name);
      const relative = path.relative(root, filePath);
      const metadata = fs.lstatSync(filePath);
      entries.push({
        relative,
        mode: metadata.mode,
        type: metadata.isDirectory()
          ? "directory"
          : metadata.isSymbolicLink()
            ? "symlink"
            : "file",
        content: metadata.isFile() ? fs.readFileSync(filePath, "utf8") : null,
      });
      if (metadata.isDirectory()) {
        visit(filePath);
      }
    }
  };
  visit(root);
  return entries;
}

function runChecker(extraArguments = [], options = {}) {
  return spawnSync(
    process.execPath,
    [checker, "--hermes-home", stagingHome, "--mode", "staging", ...extraArguments],
    {
      cwd: repoRoot,
      env: { ...process.env, ...options.env },
      encoding: "utf8",
    }
  );
}

function combinedOutput(result) {
  return `${result.stdout}\n${result.stderr}`;
}

function assertSanitized(result) {
  const output = combinedOutput(result);
  for (const secret of secretValues) {
    assert.equal(
      output.includes(secret),
      false,
      `output leaked fixture value: ${secret}`
    );
  }
  assert.equal(fs.existsSync(markerPath), false, "checker sourced a fixture env file");
}

function assertReady(result) {
  assert.equal(
    result.status,
    0,
    `expected readiness to pass:\n${combinedOutput(result)}`
  );
  assert.match(result.stdout, /hermes_wecom_readiness=ready/);
  assert.match(result.stdout, /hermes_wecom_profiles=7/);
  for (const [profile, enabled] of expectedStates) {
    assert.match(
      result.stdout,
      new RegExp(
        `hermes_wecom_profile=${profile}[^\\n]*expected_enabled=${enabled}[^\\n]*actual_enabled=${enabled}`
      )
    );
  }
  assertSanitized(result);
}

try {
  createFixture();

  const before = snapshotTree(stagingHome);
  const ready = runChecker();
  assertReady(ready);
  assert.deepEqual(
    snapshotTree(stagingHome),
    before,
    "readiness checker modified staging"
  );

  fs.rmSync(path.join(stagingHome, "profiles", "erhua", ".env"));
  fs.rmSync(path.join(stagingHome, "profiles", "wenyuange", ".env"));
  assertReady(runChecker());

  const driftConfig = path.join(stagingHome, "profiles", "guanerye", "config.yaml");
  fs.writeFileSync(
    driftConfig,
    fs.readFileSync(driftConfig, "utf8").replace("enabled: true", "enabled: false")
  );
  const drift = runChecker();
  assert.notEqual(drift.status, 0);
  assert.match(drift.stdout, /hermes_wecom_error=guanerye:wecom_enabled_mismatch/);
  assertSanitized(drift);
  writeProfile("guanerye", true);

  const malformedConfig = path.join(stagingHome, "profiles", "huabaosi", "config.yaml");
  fs.writeFileSync(malformedConfig, "platforms: [\n", "utf8");
  const malformed = runChecker();
  assert.notEqual(malformed.status, 0);
  assert.match(malformed.stdout, /hermes_wecom_error=huabaosi:config_unparseable/);
  assertSanitized(malformed);
  writeProfile("huabaosi", true);

  const secretEnv = path.join(stagingHome, "profiles", "default", ".env");
  fs.writeFileSync(
    secretEnv,
    "WECOM_BOT_ID=fixture-bot-value\nQINTOPIA_SOURCE_TEST=fixture-secret-value\n",
    "utf8"
  );
  const missingKey = runChecker();
  assert.notEqual(missingKey.status, 0);
  assert.match(
    missingKey.stdout,
    /hermes_wecom_error=default:required_key_missing:WECOM_SECRET/
  );
  assertSanitized(missingKey);
  writeProfile("default", true);

  const duplicateEnv = path.join(stagingHome, "profiles", "silaoshi", ".env");
  fs.appendFileSync(duplicateEnv, "WECOM_SECRET=another-fixture-secret\n", "utf8");
  const duplicate = runChecker();
  assert.notEqual(duplicate.status, 0);
  assert.match(duplicate.stdout, /hermes_wecom_error=silaoshi:env_duplicate_key/);
  assertSanitized(duplicate);
  writeProfile("silaoshi", true);

  const targetConfig = path.join(stagingHome, "profiles", "xiaoman", "config.yaml");
  const targetLink = path.join(tmpRoot, "target-config.yaml");
  fs.writeFileSync(targetLink, fs.readFileSync(targetConfig, "utf8"), "utf8");
  fs.rmSync(targetConfig);
  fs.symlinkSync(targetLink, targetConfig);
  const symlinkedConfig = runChecker();
  assert.notEqual(symlinkedConfig.status, 0);
  assert.match(symlinkedConfig.stdout, /hermes_wecom_error=xiaoman:config_symlink/);
  assertSanitized(symlinkedConfig);
  fs.rmSync(targetConfig);
  writeProfile("xiaoman", true);

  const targetEnv = path.join(stagingHome, "profiles", "xiaoman", ".env");
  const targetEnvLink = path.join(tmpRoot, "target-env");
  fs.writeFileSync(targetEnvLink, fs.readFileSync(targetEnv, "utf8"), "utf8");
  fs.rmSync(targetEnv);
  fs.symlinkSync(targetEnvLink, targetEnv);
  const symlinkedEnv = runChecker();
  assert.notEqual(symlinkedEnv.status, 0);
  assert.match(symlinkedEnv.stdout, /hermes_wecom_error=xiaoman:env_symlink/);
  assertSanitized(symlinkedEnv);
  fs.rmSync(targetEnv);
  writeProfile("xiaoman", true);

  const defaultProfile = path.join(stagingHome, "profiles", "default");
  const defaultProfileTarget = path.join(tmpRoot, "default-profile-target");
  fs.renameSync(defaultProfile, defaultProfileTarget);
  fs.symlinkSync(defaultProfileTarget, defaultProfile);
  const symlinkedProfile = runChecker();
  assert.notEqual(symlinkedProfile.status, 0);
  assert.match(
    symlinkedProfile.stdout,
    /hermes_wecom_error=default:profile_dir_symlink/
  );
  assertSanitized(symlinkedProfile);
  fs.rmSync(defaultProfile);
  fs.renameSync(defaultProfileTarget, defaultProfile);

  const profilesDirectory = path.join(stagingHome, "profiles");
  const profilesTarget = path.join(tmpRoot, "profiles-target");
  fs.renameSync(profilesDirectory, profilesTarget);
  fs.symlinkSync(profilesTarget, profilesDirectory);
  const symlinkedProfilesDirectory = runChecker();
  assert.notEqual(symlinkedProfilesDirectory.status, 0);
  assert.match(
    symlinkedProfilesDirectory.stdout,
    /hermes_wecom_error=profiles_dir_symlink/
  );
  assertSanitized(symlinkedProfilesDirectory);
  fs.rmSync(profilesDirectory);
  fs.renameSync(profilesTarget, profilesDirectory);

  const relativeHome = spawnSync(
    process.execPath,
    [checker, "--hermes-home", "relative-fixture", "--mode", "staging"],
    { cwd: repoRoot, encoding: "utf8", env: { ...process.env } }
  );
  assert.equal(relativeHome.status, 2);
  assert.match(combinedOutput(relativeHome), /hermes_wecom_error=invalid_invocation/);
  assertSanitized(relativeHome);

  const productionMode = spawnSync(
    process.execPath,
    [checker, "--hermes-home", stagingHome, "--mode", "production"],
    { cwd: repoRoot, encoding: "utf8", env: { ...process.env } }
  );
  assert.equal(productionMode.status, 2);
  assert.match(
    combinedOutput(productionMode),
    /hermes_wecom_error=production_home_must_be_fixed/
  );
  assertSanitized(productionMode);
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Hermes WeCom readiness fixture test passed.");
