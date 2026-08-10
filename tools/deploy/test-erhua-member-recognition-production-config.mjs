#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpRoot = fs.mkdtempSync(
  path.join(os.tmpdir(), "erhua-member-recognition-config-")
);
const sourceScript = path.join(
  repoRoot,
  "deploy/sidecar/scripts/apply-erhua-member-recognition-production-config.sh"
);

try {
  const python = spawnSync("which", ["python3"], { encoding: "utf8" }).stdout.trim();
  if (!python) {
    throw new Error("python3 is required for the config fixture");
  }

  const script = path.join(
    tmpRoot,
    "apply-erhua-member-recognition-production-config.sh"
  );
  const envFile = path.join(tmpRoot, "message-sidecar.env");
  fs.writeFileSync(
    script,
    fs
      .readFileSync(sourceScript, "utf8")
      .replaceAll("/usr/bin/python3", python)
      .replaceAll("/etc/qintopia/message-sidecar.env", envFile),
    "utf8"
  );
  fs.chmodSync(script, 0o755);

  const writeEnv = (content) => {
    fs.writeFileSync(envFile, `${content.trim()}\n`, "utf8");
    fs.chmodSync(envFile, 0o640);
  };
  const run = (extraEnv = {}) =>
    spawnSync(script, ["--apply"], {
      cwd: tmpRoot,
      encoding: "utf8",
      env: { ...process.env, ...extraEnv },
    });

  writeEnv(`
QINTOPIA_SIDECAR_DATABASE_URL=postgres://user:pass@127.0.0.1:5432/qintopia
QINTOPIA_PROFILE_TARGET_CHAT_IDS=room_reviewed_1
`);
  const missingApproval = run({
    QINTOPIA_ERHUA_MEMBER_RECOGNITION_CONFIG_CANARY_SENDER_ID: "sender_reviewed_1",
  });
  assertFailed(missingApproval, /explicit owner approval/);

  const missingSender = run({
    QINTOPIA_ERHUA_MEMBER_RECOGNITION_PRODUCTION_CONFIG:
      "approved-production-erhua-member-recognition-config",
  });
  assertFailed(missingSender, /CONFIG_CANARY_SENDER_ID is required/);

  const senderEqualsChat = run({
    QINTOPIA_ERHUA_MEMBER_RECOGNITION_PRODUCTION_CONFIG:
      "approved-production-erhua-member-recognition-config",
    QINTOPIA_ERHUA_MEMBER_RECOGNITION_CONFIG_CANARY_SENDER_ID: "room_reviewed_1",
  });
  assertFailed(senderEqualsChat, /must differ from the reviewed group id/);

  const approved = run({
    QINTOPIA_ERHUA_MEMBER_RECOGNITION_PRODUCTION_CONFIG:
      "approved-production-erhua-member-recognition-config",
    QINTOPIA_ERHUA_MEMBER_RECOGNITION_CONFIG_CANARY_SENDER_ID: "sender_reviewed_1",
  });
  assertPassed(approved);
  if (
    approved.stdout.includes("room_reviewed_1") ||
    approved.stdout.includes("sender_reviewed_1")
  ) {
    throw new Error("config apply output leaked reviewed ids");
  }
  const applied = fs.readFileSync(envFile, "utf8");
  for (const fragment of [
    "QINTOPIA_PROFILE_TARGET_CHAT_IDS='room_reviewed_1'",
    "QINTOPIA_ERHUA_MEMBER_RECOGNITION_CANARY_CHAT_ID='room_reviewed_1'",
    "QINTOPIA_ERHUA_MEMBER_RECOGNITION_CANARY_SENDER_ID='sender_reviewed_1'",
  ]) {
    if (!applied.includes(fragment)) {
      throw new Error(`applied env is missing ${fragment}`);
    }
  }

  writeEnv(`
QINTOPIA_SIDECAR_DATABASE_URL=postgres://user:pass@127.0.0.1:5432/qintopia
QINTOPIA_PROFILE_TARGET_CHAT_IDS=room_a,room_b
`);
  const multipleTargets = run({
    QINTOPIA_ERHUA_MEMBER_RECOGNITION_PRODUCTION_CONFIG:
      "approved-production-erhua-member-recognition-config",
    QINTOPIA_ERHUA_MEMBER_RECOGNITION_CONFIG_CANARY_SENDER_ID: "sender_reviewed_1",
  });
  assertFailed(multipleTargets, /exactly one reviewed group/);

  const explicitChat = run({
    QINTOPIA_ERHUA_MEMBER_RECOGNITION_PRODUCTION_CONFIG:
      "approved-production-erhua-member-recognition-config",
    QINTOPIA_ERHUA_MEMBER_RECOGNITION_CONFIG_CHAT_ID: "room_reviewed_2",
    QINTOPIA_ERHUA_MEMBER_RECOGNITION_CONFIG_CANARY_SENDER_ID: "sender_reviewed_2",
  });
  assertPassed(explicitChat);
  const explicitApplied = fs.readFileSync(envFile, "utf8");
  if (
    !explicitApplied.includes(
      "QINTOPIA_ERHUA_MEMBER_RECOGNITION_CANARY_CHAT_ID='room_reviewed_2'"
    )
  ) {
    throw new Error("explicit chat id was not applied");
  }

  const symlinkTarget = path.join(tmpRoot, "message-sidecar-target.env");
  fs.writeFileSync(
    symlinkTarget,
    [
      "QINTOPIA_SIDECAR_DATABASE_URL=postgres://user:pass@127.0.0.1:5432/qintopia",
      "QINTOPIA_PROFILE_TARGET_CHAT_IDS=room_reviewed_1",
    ].join("\n") + "\n",
    "utf8"
  );
  fs.chmodSync(symlinkTarget, 0o640);
  fs.rmSync(envFile, { force: true });
  fs.symlinkSync(symlinkTarget, envFile);
  const symlinkEnv = run({
    QINTOPIA_ERHUA_MEMBER_RECOGNITION_PRODUCTION_CONFIG:
      "approved-production-erhua-member-recognition-config",
    QINTOPIA_ERHUA_MEMBER_RECOGNITION_CONFIG_CANARY_SENDER_ID: "sender_reviewed_1",
  });
  assertFailed(symlinkEnv, /regular non-symlink file/);

  console.log("Erhua member recognition production config test passed.");
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

function assertPassed(result) {
  if (result.status !== 0) {
    throw new Error(
      `expected command to pass, got ${result.status}\nstdout=${result.stdout}\nstderr=${result.stderr}`
    );
  }
}

function assertFailed(result, pattern) {
  if (result.status === 0 || !pattern.test(`${result.stdout}\n${result.stderr}`)) {
    throw new Error(
      `expected command to fail with ${pattern}, got ${result.status}\nstdout=${result.stdout}\nstderr=${result.stderr}`
    );
  }
}
