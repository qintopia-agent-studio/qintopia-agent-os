#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpParent = fs.existsSync("/private/tmp") ? "/private/tmp" : "/tmp";
const tmpRoot = fs.mkdtempSync(path.join(tmpParent, "qintopia-xiaoman-cron-"));
const script = path.join(
  repoRoot,
  "deploy/sidecar/scripts/xiaoman-legacy-cron-observation-smoke.sh"
);

const writeCron = (profileDir, value) => {
  const cronFile = path.join(profileDir, "cron", "jobs.json");
  fs.mkdirSync(path.dirname(cronFile), { recursive: true });
  fs.writeFileSync(cronFile, JSON.stringify(value, null, 2), "utf8");
  fs.chmodSync(cronFile, 0o600);
  return cronFile;
};

try {
  const profileDir = path.join(
    tmpRoot,
    "home",
    "ubuntu",
    ".hermes",
    "profiles",
    "xiaoman"
  );
  fs.mkdirSync(profileDir, { recursive: true });
  const missingCron = path.join(profileDir, "cron", "jobs.json");

  const run = (extraEnv = {}) =>
    spawnSync("bash", [script], {
      cwd: repoRoot,
      env: {
        ...process.env,
        QINTOPIA_XIAOMAN_LEGACY_CRON_OBSERVATION_ENABLE: "1",
        QINTOPIA_XIAOMAN_LEGACY_CRON_OBSERVATION_TEST_MODE: "1",
        QINTOPIA_XIAOMAN_LEGACY_CRON_OBSERVATION_TEST_ROOT: tmpRoot,
        QINTOPIA_XIAOMAN_PROFILE_DIR: profileDir,
        QINTOPIA_XIAOMAN_LEGACY_CRON_FILE: missingCron,
        ...extraEnv,
      },
      encoding: "utf8",
    });

  const productionOverride = spawnSync("bash", [script], {
    cwd: repoRoot,
    env: {
      ...process.env,
      QINTOPIA_XIAOMAN_LEGACY_CRON_OBSERVATION_ENABLE: "1",
      QINTOPIA_XIAOMAN_PROFILE_DIR: profileDir,
      QINTOPIA_XIAOMAN_LEGACY_CRON_FILE: missingCron,
    },
    encoding: "utf8",
  });
  if (productionOverride.status === 0) {
    throw new Error("legacy cron observation accepted production path overrides");
  }

  const missing = run();
  if (missing.status !== 0 || !missing.stdout.includes('"cron_file_present":false')) {
    throw new Error(
      `missing cron observation failed\n${missing.stdout}\n${missing.stderr}`
    );
  }

  const emptyCron = writeCron(profileDir, {
    updated_at: "2026-07-20T00:00:00Z",
    jobs: [],
  });
  const empty = run({
    QINTOPIA_XIAOMAN_LEGACY_CRON_FILE: emptyCron,
  });
  if (empty.status !== 0 || !empty.stdout.includes('"no_legacy_cron_jobs"')) {
    throw new Error(`empty cron observation failed\n${empty.stdout}\n${empty.stderr}`);
  }

  const legacyCron = writeCron(profileDir, {
    jobs: [
      {
        name: "legacy-approval-question",
        enabled: true,
        schedule: "*/5 * * * *",
        command: "ask-for-approval",
      },
    ],
  });
  const legacy = run({
    QINTOPIA_XIAOMAN_LEGACY_CRON_FILE: legacyCron,
  });
  if (legacy.status === 0 || !legacy.stderr.includes("runtime cron job declarations")) {
    throw new Error("legacy cron observation accepted a job declaration");
  }
  if (`${legacy.stdout}\n${legacy.stderr}`.includes("ask-for-approval")) {
    throw new Error("legacy cron observation leaked cron command text");
  }

  fs.writeFileSync(legacyCron, "{not-json", "utf8");
  const invalid = run({
    QINTOPIA_XIAOMAN_LEGACY_CRON_FILE: legacyCron,
  });
  if (invalid.status === 0 || !invalid.stderr.includes("must be JSON")) {
    throw new Error("legacy cron observation accepted invalid JSON");
  }

  const outside = run({
    QINTOPIA_XIAOMAN_LEGACY_CRON_FILE: path.join(tmpRoot, "jobs.json"),
  });
  if (outside.status === 0) {
    throw new Error("legacy cron observation accepted a cron file outside profile");
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Xiaoman legacy cron observation test passed.");
