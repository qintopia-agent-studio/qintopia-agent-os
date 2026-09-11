#!/usr/bin/env node

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const checker = path.join(repoRoot, "tools/deploy/check-hermes-wecom-parity.mjs");
const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "qintopia-wecom-parity-"));
const baselineHome = path.join(tmpRoot, "baseline");
const candidateHome = path.join(tmpRoot, "candidate");
const secretValues = ["bot-fixture", "secret-fixture", "target-fixture", tmpRoot];
const states = new Map([
  ["default", true],
  ["erhua", false],
  ["guanerye", true],
  ["huabaosi", true],
  ["silaoshi", true],
  ["wenyuange", false],
  ["xiaoman", true],
]);

function writeProfile(home, profile, enabled, parent = "channel") {
  const directory = profile === "default" ? home : path.join(home, "profiles", profile);
  fs.mkdirSync(directory, { recursive: true });
  fs.writeFileSync(
    path.join(directory, "config.yaml"),
    [
      `${parent}:`,
      "  wecom:",
      `    enabled: ${enabled}`,
      "    reconnect_delay: 5",
      "    extra:",
      "      bot_id: bot-fixture",
      "      secret: secret-fixture",
      "",
    ].join("\n")
  );
  fs.writeFileSync(
    path.join(directory, ".env"),
    [
      "WECOM_BOT_ID=bot-fixture",
      "WECOM_SECRET=secret-fixture",
      "WECOM_HOME_CHANNEL=target-fixture",
      "UNRELATED=value",
      "",
    ].join("\n")
  );
}

function createHomes() {
  for (const [profile, enabled] of states) {
    writeProfile(baselineHome, profile, enabled);
    writeProfile(candidateHome, profile, enabled, "platforms");
  }
}

function run() {
  return spawnSync(
    process.execPath,
    [
      checker,
      "--baseline-home",
      baselineHome,
      "--candidate-home",
      candidateHome,
      "--mode",
      "fixture",
    ],
    { cwd: repoRoot, encoding: "utf8", env: { ...process.env } }
  );
}

function output(result) {
  return `${result.stdout}\n${result.stderr}`;
}

function assertSanitized(result) {
  for (const secret of secretValues) {
    assert.equal(
      output(result).includes(secret),
      false,
      `leaked fixture value: ${secret}`
    );
  }
}

try {
  createHomes();
  const ready = run();
  assert.equal(ready.status, 0, output(ready));
  assert.match(ready.stdout, /hermes_wecom_parity=ready/);
  assert.match(ready.stdout, /hermes_wecom_parity_profiles=7/);
  assertSanitized(ready);

  const unrelatedEnv = path.join(candidateHome, ".env");
  fs.appendFileSync(unrelatedEnv, "ANOTHER_UNRELATED=value\n");
  const unrelated = run();
  assert.equal(unrelated.status, 0, output(unrelated));
  assertSanitized(unrelated);

  const secretEnv = path.join(candidateHome, ".env");
  fs.writeFileSync(
    secretEnv,
    fs
      .readFileSync(secretEnv, "utf8")
      .replace("WECOM_SECRET=secret-fixture", "WECOM_SECRET=changed-secret-fixture")
  );
  const envDrift = run();
  assert.notEqual(envDrift.status, 0);
  assert.match(envDrift.stdout, /default:wecom_env_mismatch/);
  assertSanitized(envDrift);
  writeProfile(candidateHome, "default", true, "platforms");

  const configPath = path.join(candidateHome, "profiles", "huabaosi", "config.yaml");
  fs.writeFileSync(
    configPath,
    fs
      .readFileSync(configPath, "utf8")
      .replace("reconnect_delay: 5", "reconnect_delay: 8")
  );
  const configDrift = run();
  assert.notEqual(configDrift.status, 0);
  assert.match(configDrift.stdout, /huabaosi:wecom_config_mismatch/);
  assertSanitized(configDrift);
  writeProfile(candidateHome, "huabaosi", true, "platforms");

  const enabledPath = path.join(candidateHome, "profiles", "xiaoman", "config.yaml");
  fs.writeFileSync(
    enabledPath,
    fs.readFileSync(enabledPath, "utf8").replace("enabled: true", "enabled: false")
  );
  const enabledDrift = run();
  assert.notEqual(enabledDrift.status, 0);
  assert.match(enabledDrift.stdout, /xiaoman:candidate_enabled_mismatch/);
  assertSanitized(enabledDrift);
  writeProfile(candidateHome, "xiaoman", true, "platforms");

  const configTarget = path.join(tmpRoot, "config-target");
  const configLink = path.join(candidateHome, "profiles", "silaoshi", "config.yaml");
  fs.writeFileSync(configTarget, fs.readFileSync(configLink));
  fs.rmSync(configLink);
  fs.symlinkSync(configTarget, configLink);
  const symlinked = run();
  assert.notEqual(symlinked.status, 0);
  assert.match(symlinked.stdout, /silaoshi:candidate_config_symlink/);
  assertSanitized(symlinked);

  const production = spawnSync(
    process.execPath,
    [
      checker,
      "--baseline-home",
      baselineHome,
      "--candidate-home",
      candidateHome,
      "--mode",
      "production",
    ],
    { cwd: repoRoot, encoding: "utf8", env: { ...process.env } }
  );
  assert.equal(production.status, 2);
  assert.match(output(production), /production_baseline_must_be_fixed/);
  assertSanitized(production);
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Hermes WeCom parity fixture test passed.");
