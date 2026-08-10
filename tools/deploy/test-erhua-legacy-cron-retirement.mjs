#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpParent = fs.existsSync("/private/tmp") ? "/private/tmp" : "/tmp";
const tmpRoot = fs.mkdtempSync(path.join(tmpParent, "qintopia-erhua-cron-retirement-"));
const sourceRetirement = path.join(
  repoRoot,
  "deploy/sidecar/scripts/retire-erhua-legacy-cron-production.sh"
);
const sourceObservation = path.join(
  repoRoot,
  "deploy/sidecar/scripts/erhua-legacy-cron-observation-smoke.sh"
);
const fixedCronFile = "/home/ubuntu/.hermes/profiles/erhua/cron/jobs.json";
const fixedObservedSha =
  "59edf8abc1602a10a5ffb83120c631395d8c486df66343bfd1591a94da30412c";

const writeExecutable = (filePath, content) => {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, content, "utf8");
  fs.chmodSync(filePath, 0o755);
};
const sha256 = (bytes) => crypto.createHash("sha256").update(bytes).digest("hex");
const modeOf = (filePath) => fs.statSync(filePath).mode & 0o777;
const listBackups = (cronDir) =>
  fs
    .readdirSync(cronDir)
    .filter((name) => name.startsWith("jobs.json.retired-"))
    .sort();

try {
  const python = spawnSync("which", ["python3"], { encoding: "utf8" }).stdout.trim();
  if (!python) {
    throw new Error("python3 is required for the retirement fixture");
  }

  const scriptsDir = path.join(tmpRoot, "scripts");
  const retirement = path.join(scriptsDir, "retire-erhua-legacy-cron-production.sh");
  const wrongHashRetirement = path.join(
    scriptsDir,
    "retire-erhua-legacy-cron-production-wrong-hash.sh"
  );
  const profileDir = path.join(
    tmpRoot,
    "home",
    "ubuntu",
    ".hermes",
    "profiles",
    "erhua"
  );
  const cronDir = path.join(profileDir, "cron");
  const cronFile = path.join(cronDir, "jobs.json");
  fs.mkdirSync(cronDir, { recursive: true });
  const registryFile = path.join(tmpRoot, "registry", "reviewed-cron-jobs.json");
  fs.mkdirSync(path.dirname(registryFile), { recursive: true });
  fs.writeFileSync(
    registryFile,
    JSON.stringify({ schema_version: 1, reviewed_jobs: [] }, null, 2),
    "utf8"
  );

  const legacyCron = {
    jobs: [
      {
        name: "legacy-erhua-morning-brief",
        enabled: true,
        schedule: "0 7 * * *",
        command: "send real group content",
      },
      {
        name: "legacy-evening-summary",
        enabled: true,
        schedule: "0 23 * * *",
        prompt: "raw prompt must not leak",
      },
      {
        name: "legacy-weekly-saturday",
        enabled: true,
        schedule: "0 10 * * 6",
        message: "private group message body",
      },
      {
        name: "legacy-weekly-sunday",
        enabled: true,
        schedule: "0 20 * * 0",
        tool: "deprecated Hermes cron",
      },
    ],
  };
  fs.writeFileSync(cronFile, JSON.stringify(legacyCron, null, 2), "utf8");
  fs.chmodSync(cronFile, 0o600);
  const legacyBytes = fs.readFileSync(cronFile);
  const legacySha = sha256(legacyBytes);

  const materializeRetirement = (expectedSha) =>
    fs
      .readFileSync(sourceRetirement, "utf8")
      .replaceAll("/usr/bin/python3", python)
      .replaceAll(fixedCronFile, cronFile)
      .replaceAll(fixedObservedSha, expectedSha);
  const retirementSource = materializeRetirement(legacySha);
  if (
    retirementSource.includes("QINTOPIA_ERHUA_PROFILE_DIR") ||
    retirementSource.includes("QINTOPIA_ERHUA_LEGACY_CRON_FILE")
  ) {
    throw new Error("retirement script must not accept cron path overrides");
  }
  writeExecutable(retirement, retirementSource);
  writeExecutable(wrongHashRetirement, materializeRetirement("0".repeat(64)));

  const run = (script, extraEnv = {}) =>
    spawnSync("bash", [script], {
      cwd: repoRoot,
      env: {
        ...process.env,
        QINTOPIA_ERHUA_PROFILE_DIR: path.join(tmpRoot, "evil-profile"),
        QINTOPIA_ERHUA_LEGACY_CRON_FILE: path.join(tmpRoot, "evil-jobs.json"),
        ...extraEnv,
      },
      encoding: "utf8",
    });

  let result = run(retirement);
  if (result.status === 0) {
    throw new Error("retirement accepted missing owner approval");
  }
  if (sha256(fs.readFileSync(cronFile)) !== legacySha || listBackups(cronDir).length) {
    throw new Error("retirement mutated cron state without owner approval");
  }

  result = run(wrongHashRetirement, {
    QINTOPIA_ERHUA_LEGACY_CRON_RETIREMENT:
      "approved-production-erhua-legacy-cron-retirement",
  });
  if (
    result.status === 0 ||
    !result.stderr.includes("sha256 does not match") ||
    !result.stderr.includes(`actual_sha256=${legacySha}`) ||
    !result.stderr.includes("current_decl_count=4") ||
    !result.stderr.includes("external_calls_executed=false") ||
    !result.stderr.includes("safe_for_chat=false") ||
    sha256(fs.readFileSync(cronFile)) !== legacySha ||
    listBackups(cronDir).length
  ) {
    throw new Error("retirement accepted an unexpected legacy cron hash");
  }
  for (const forbidden of [
    "send real group content",
    "raw prompt must not leak",
    "private group message body",
    "deprecated Hermes cron",
  ]) {
    if (result.stderr.includes(forbidden)) {
      throw new Error(
        `hash mismatch evidence leaked legacy cron content: ${forbidden}`
      );
    }
  }

  result = run(retirement, {
    QINTOPIA_ERHUA_LEGACY_CRON_RETIREMENT:
      "approved-production-erhua-legacy-cron-retirement",
  });
  if (result.status !== 0) {
    throw new Error(`retirement failed\n${result.stdout}\n${result.stderr}`);
  }
  const combinedOutput = `${result.stdout}\n${result.stderr}`;
  for (const forbidden of [
    "send real group content",
    "raw prompt must not leak",
    "private group message body",
    "deprecated Hermes cron",
  ]) {
    if (combinedOutput.includes(forbidden)) {
      throw new Error(`retirement leaked legacy cron content: ${forbidden}`);
    }
  }
  if (
    !combinedOutput.includes('"status":"legacy_cron_retired"') ||
    !combinedOutput.includes(`"previous_sha256":"${legacySha}"`) ||
    !combinedOutput.includes('"previous_decl_count":4') ||
    !combinedOutput.includes('"new_decl_count":0')
  ) {
    throw new Error(`retirement did not emit sanitized evidence\n${combinedOutput}`);
  }
  const retiredCron = JSON.parse(fs.readFileSync(cronFile, "utf8"));
  if (
    retiredCron.retired_by !== "retire-erhua-legacy-cron-production.sh" ||
    retiredCron.previous_sha256 !== legacySha ||
    retiredCron.previous_decl_count !== 4 ||
    !Array.isArray(retiredCron.jobs) ||
    retiredCron.jobs.length !== 0
  ) {
    throw new Error("retirement did not replace cron file with retired metadata");
  }
  if (modeOf(cronFile) !== 0o600) {
    throw new Error("retirement did not preserve cron file mode");
  }
  const backups = listBackups(cronDir);
  if (backups.length !== 1) {
    throw new Error("retirement did not create exactly one backup");
  }
  const backupPath = path.join(cronDir, backups[0]);
  if (
    modeOf(backupPath) !== 0o600 ||
    sha256(fs.readFileSync(backupPath)) !== legacySha
  ) {
    throw new Error("retirement backup did not preserve the previous cron bytes");
  }

  const observation = spawnSync("bash", [sourceObservation], {
    cwd: repoRoot,
    env: {
      ...process.env,
      QINTOPIA_ERHUA_LEGACY_CRON_OBSERVATION_ENABLE: "1",
      QINTOPIA_ERHUA_LEGACY_CRON_OBSERVATION_TEST_MODE: "1",
      QINTOPIA_ERHUA_LEGACY_CRON_OBSERVATION_TEST_ROOT: tmpRoot,
      QINTOPIA_ERHUA_PROFILE_DIR: profileDir,
      QINTOPIA_ERHUA_LEGACY_CRON_FILE: cronFile,
      QINTOPIA_ERHUA_LEGACY_CRON_OBSERVATION_REGISTRY: registryFile,
    },
    encoding: "utf8",
  });
  if (observation.status !== 0) {
    throw new Error(
      `retired cron did not pass observation\n${observation.stdout}\n${observation.stderr}`
    );
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Erhua legacy cron retirement test passed.");
