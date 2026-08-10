#!/usr/bin/env node

import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpRoot = fs.mkdtempSync(
  path.join("/tmp", "erhua-member-recognition-config-observation-")
);
const sourceScript = path.join(
  repoRoot,
  "deploy/sidecar/scripts/erhua-member-recognition-production-config-observation-smoke.sh"
);

try {
  const python = spawnSync("which", ["python3"], { encoding: "utf8" }).stdout.trim();
  if (!python) {
    throw new Error("python3 is required for the observation fixture");
  }

  const script = path.join(
    tmpRoot,
    "erhua-member-recognition-production-config-observation-smoke.sh"
  );
  const envFile = path.join(tmpRoot, "message-sidecar.env");
  fs.writeFileSync(
    script,
    fs.readFileSync(sourceScript, "utf8").replaceAll("/usr/bin/python3", python),
    "utf8"
  );
  fs.chmodSync(script, 0o755);

  const writeEnv = (content, mode = 0o640) => {
    fs.rmSync(envFile, { force: true });
    fs.writeFileSync(envFile, `${content.trim()}\n`, "utf8");
    fs.chmodSync(envFile, mode);
  };
  const run = (extraEnv = {}) =>
    spawnSync(script, [], {
      cwd: tmpRoot,
      encoding: "utf8",
      env: {
        ...process.env,
        QINTOPIA_ERHUA_MEMBER_RECOGNITION_CONFIG_OBSERVATION_ENABLE: "1",
        QINTOPIA_ERHUA_MEMBER_RECOGNITION_CONFIG_OBSERVATION_TEST_MODE: "1",
        QINTOPIA_ERHUA_MEMBER_RECOGNITION_CONFIG_OBSERVATION_TEST_ROOT: tmpRoot,
        QINTOPIA_ERHUA_MEMBER_RECOGNITION_CONFIG_OBSERVATION_ENV_FILE: envFile,
        ...extraEnv,
      },
    });

  writeEnv(`
QINTOPIA_SIDECAR_DATABASE_URL=postgres://user:p$a$s@127.0.0.1:5432/qintopia
QINTOPIA_PROFILE_TARGET_CHAT_IDS='room_reviewed_1'
QINTOPIA_ERHUA_MEMBER_RECOGNITION_CANARY_CHAT_ID='room_reviewed_1'
QINTOPIA_ERHUA_MEMBER_RECOGNITION_CANARY_SENDER_ID='sender_reviewed_1'
`);
  let result = run();
  assert.equal(result.status, 0, result.stderr);
  let report = observation(result.stdout);
  assert.equal(report.success, true);
  assert.equal(report.action_status, "ready_for_member_recognition_runbook");
  assert.equal(report.safe_for_chat, true);
  assert.equal(report.database_url_count, 1);
  assert.equal(report.profile_target_count, 1);
  assert.equal(report.profile_target_matches_canary_chat, true);
  assert.equal(report.canary_sender_differs_from_chat, true);
  assert.match(report.scope_fingerprint, /^sha256:[0-9a-f]{64}$/);
  assertNoSecretOutput(result, ["room_reviewed_1", "sender_reviewed_1", "p$a$s"]);

  writeEnv(`
QINTOPIA_SIDECAR_DATABASE_URL=postgres://user:pass@127.0.0.1:5432/qintopia
QINTOPIA_PROFILE_TARGET_CHAT_IDS='room_reviewed_1'
QINTOPIA_ERHUA_MEMBER_RECOGNITION_CANARY_CHAT_ID='room_reviewed_2'
QINTOPIA_ERHUA_MEMBER_RECOGNITION_CANARY_SENDER_ID='sender_reviewed_1'
`);
  result = run();
  assert.notEqual(result.status, 0);
  report = observation(result.stdout);
  assert.equal(report.success, false);
  assert.equal(report.profile_target_matches_canary_chat, false);
  assert.match(report.limitations.join(","), /profile_target_canary_chat_mismatch/);
  assertNoSecretOutput(result, [
    "room_reviewed_1",
    "room_reviewed_2",
    "sender_reviewed_1",
  ]);

  writeEnv(`
QINTOPIA_SIDECAR_DATABASE_URL=postgres://user:pass@127.0.0.1:5432/qintopia
QINTOPIA_PROFILE_TARGET_CHAT_IDS='room_reviewed_1'
QINTOPIA_ERHUA_MEMBER_RECOGNITION_CANARY_CHAT_ID='room_reviewed_1'
QINTOPIA_ERHUA_MEMBER_RECOGNITION_CANARY_SENDER_ID='room_reviewed_1'
`);
  result = run();
  assert.notEqual(result.status, 0);
  report = observation(result.stdout);
  assert.equal(report.canary_sender_differs_from_chat, false);
  assert.match(report.limitations.join(","), /canary_sender_equals_chat/);
  assertNoSecretOutput(result, ["room_reviewed_1"]);

  const target = path.join(tmpRoot, "message-sidecar-target.env");
  fs.writeFileSync(
    target,
    [
      "QINTOPIA_SIDECAR_DATABASE_URL=postgres://user:pass@127.0.0.1:5432/qintopia",
      "QINTOPIA_PROFILE_TARGET_CHAT_IDS='room_reviewed_1'",
      "QINTOPIA_ERHUA_MEMBER_RECOGNITION_CANARY_CHAT_ID='room_reviewed_1'",
      "QINTOPIA_ERHUA_MEMBER_RECOGNITION_CANARY_SENDER_ID='sender_reviewed_1'",
    ].join("\n") + "\n",
    "utf8"
  );
  fs.chmodSync(target, 0o640);
  fs.rmSync(envFile, { force: true });
  fs.symlinkSync(target, envFile);
  result = run();
  assert.notEqual(result.status, 0);
  report = observation(result.stdout);
  assert.equal(report.env_file_secure, false);
  assert.match(report.limitations.join(","), /env_file_not_regular/);
  assertNoSecretOutput(result, ["room_reviewed_1", "sender_reviewed_1"]);

  const skipped = spawnSync(script, [], {
    cwd: tmpRoot,
    encoding: "utf8",
    env: { ...process.env },
  });
  assert.equal(skipped.status, 0);
  assert.match(skipped.stderr, /observation skipped/);

  console.log("Erhua member recognition production config observation test passed.");
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

function observation(stdout) {
  const prefix = "erhua_member_recognition_production_config_observation=";
  const line = stdout.split(/\r?\n/).find((item) => item.startsWith(prefix));
  assert.ok(line, `missing observation line in stdout=${stdout}`);
  return JSON.parse(line.slice(prefix.length));
}

function assertNoSecretOutput(result, fragments) {
  const combined = `${result.stdout}\n${result.stderr}`;
  for (const fragment of fragments) {
    assert.equal(combined.includes(fragment), false, `leaked ${fragment}`);
  }
}
