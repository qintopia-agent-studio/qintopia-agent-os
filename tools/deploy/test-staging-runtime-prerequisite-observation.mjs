#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const script = path.join(
  repoRoot,
  "deploy/sidecar/scripts/staging-runtime-prerequisite-observation-smoke.sh"
);
const fixtureParent =
  process.platform === "darwin" ? fs.realpathSync(os.tmpdir()) : repoRoot;
const tmpRoot = fs.mkdtempSync(
  path.join(fixtureParent, "qintopia-staging-runtime-prereq-")
);
const releaseSha = "0123456789abcdef0123456789abcdef01234567";
const envFile = path.join(tmpRoot, "message-sidecar-staging.env");
const releaseRoot = path.join(tmpRoot, "qintopia-agent-os-staging-releases");
const sidecarPath = path.join(
  releaseRoot,
  releaseSha,
  "sidecar",
  "qintopia-message-sidecar"
);
const secretValue = "staging-prereq-secret-must-not-appear";

const runObservation = (extraEnv = {}) =>
  spawnSync("bash", [script], {
    cwd: repoRoot,
    env: {
      ...process.env,
      QINTOPIA_STAGING_RUNTIME_PREREQUISITE_OBSERVATION_ENABLE: "1",
      QINTOPIA_STAGING_RUNTIME_PREREQUISITE_OBSERVATION_TEST_MODE: "1",
      QINTOPIA_STAGING_RUNTIME_PREREQUISITE_OBSERVATION_ENV_FILE: envFile,
      QINTOPIA_STAGING_RUNTIME_PREREQUISITE_OBSERVATION_RELEASE_ROOT: releaseRoot,
      QINTOPIA_STAGING_RUNTIME_PREREQUISITE_RELEASE_SHA: releaseSha,
      ...extraEnv,
    },
    encoding: "utf8",
  });

const parseReport = (result) => {
  const line = result.stdout
    .split(/\r?\n/)
    .find((entry) => entry.startsWith("staging_runtime_prerequisite_observation="));
  if (!line) {
    throw new Error(
      `missing observation report\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`
    );
  }
  return JSON.parse(line.slice("staging_runtime_prerequisite_observation=".length));
};

try {
  let result = runObservation({
    QINTOPIA_STAGING_RUNTIME_PREREQUISITE_SIDECAR_SHA256: "0".repeat(64),
  });
  if (result.status !== 0) {
    throw new Error(`missing observation should not fail\nstderr:\n${result.stderr}`);
  }
  let report = parseReport(result);
  if (
    report.success !== true ||
    report.ready_for_staging !== false ||
    report.action_status !== "not_ready" ||
    report.env_file_present !== false ||
    report.release_root_present !== false ||
    !report.limitations.includes("env_file_path_missing") ||
    !report.limitations.includes("release_root_path_missing") ||
    `${result.stdout}\n${result.stderr}`.includes(secretValue)
  ) {
    throw new Error(`missing report is invalid: ${JSON.stringify(report)}`);
  }

  fs.mkdirSync(path.dirname(sidecarPath), { recursive: true });
  fs.writeFileSync(
    envFile,
    [
      `QINTOPIA_SIDECAR_DATABASE_URL=postgres://user:${secretValue}@127.0.0.1:5432/qintopia_staging`,
      `QINTOPIA_HUABAOSI_IMAGE_API_KEY=$(echo ${secretValue})`,
      "",
    ].join("\n"),
    { encoding: "utf8", mode: 0o600 }
  );
  fs.writeFileSync(sidecarPath, "#!/usr/bin/env bash\nexit 99\n", {
    encoding: "utf8",
    mode: 0o755,
  });
  fs.chmodSync(releaseRoot, 0o755);
  fs.chmodSync(path.join(releaseRoot, releaseSha), 0o555);
  fs.chmodSync(path.dirname(sidecarPath), 0o555);
  fs.chmodSync(sidecarPath, 0o555);
  const sidecarHash = crypto
    .createHash("sha256")
    .update(fs.readFileSync(sidecarPath))
    .digest("hex");

  result = runObservation({
    QINTOPIA_STAGING_RUNTIME_PREREQUISITE_SIDECAR_SHA256: sidecarHash,
  });
  if (result.status !== 0) {
    throw new Error(`ready observation failed\nstderr:\n${result.stderr}`);
  }
  report = parseReport(result);
  if (
    report.ready_for_staging !== true ||
    report.action_status !== "ready_for_staging_readiness_smokes" ||
    report.env_file_secure !== true ||
    report.release_root_secure !== true ||
    report.sidecar_binary_secure !== true ||
    report.sidecar_hash_matches !== true ||
    report.sidecar_binary_sha256 !== sidecarHash ||
    report.test_mode !== true ||
    `${result.stdout}\n${result.stderr}`.includes(secretValue)
  ) {
    throw new Error(`ready report is invalid: ${JSON.stringify(report)}`);
  }

  fs.chmodSync(sidecarPath, 0o500);
  result = runObservation({
    QINTOPIA_STAGING_RUNTIME_PREREQUISITE_SIDECAR_SHA256: sidecarHash,
  });
  if (result.status !== 0) {
    throw new Error(
      `owner-executable observation should not fail\nstderr:\n${result.stderr}`
    );
  }
  report = parseReport(result);
  if (
    report.ready_for_staging !== true ||
    report.sidecar_binary_secure !== true ||
    report.sidecar_hash_matches !== true
  ) {
    throw new Error(`owner-executable report is invalid: ${JSON.stringify(report)}`);
  }
  fs.chmodSync(sidecarPath, 0o555);

  const parentLink = path.join(tmpRoot, "linked-staging-parent");
  fs.symlinkSync(tmpRoot, parentLink, "dir");
  result = runObservation({
    QINTOPIA_STAGING_RUNTIME_PREREQUISITE_OBSERVATION_ENV_FILE: path.join(
      parentLink,
      "message-sidecar-staging.env"
    ),
    QINTOPIA_STAGING_RUNTIME_PREREQUISITE_OBSERVATION_RELEASE_ROOT: path.join(
      parentLink,
      "qintopia-agent-os-staging-releases"
    ),
    QINTOPIA_STAGING_RUNTIME_PREREQUISITE_SIDECAR_SHA256: sidecarHash,
  });
  if (result.status !== 0) {
    throw new Error(
      `symlink parent observation should not fail\nstderr:\n${result.stderr}`
    );
  }
  report = parseReport(result);
  if (
    report.ready_for_staging !== false ||
    report.env_file_secure !== false ||
    !report.limitations.includes("env_file_path_parent_is_symlink")
  ) {
    throw new Error(`symlink parent report is invalid: ${JSON.stringify(report)}`);
  }

  fs.chmodSync(sidecarPath, 0o444);
  result = runObservation({
    QINTOPIA_STAGING_RUNTIME_PREREQUISITE_SIDECAR_SHA256: sidecarHash,
  });
  if (result.status !== 0) {
    throw new Error(
      `non-executable observation should not fail\nstderr:\n${result.stderr}`
    );
  }
  report = parseReport(result);
  if (
    report.ready_for_staging !== false ||
    report.sidecar_binary_secure !== false ||
    !report.limitations.includes("sidecar_binary_path_not_executable")
  ) {
    throw new Error(`non-executable report is invalid: ${JSON.stringify(report)}`);
  }
  fs.chmodSync(sidecarPath, 0o555);

  result = runObservation({
    QINTOPIA_STAGING_RUNTIME_PREREQUISITE_SIDECAR_SHA256: "f".repeat(64),
  });
  report = parseReport(result);
  if (
    result.status !== 0 ||
    report.ready_for_staging !== false ||
    report.sidecar_hash_matches !== false ||
    !report.limitations.includes("sidecar_hash_mismatch")
  ) {
    throw new Error(`hash mismatch report is invalid: ${JSON.stringify(report)}`);
  }

  console.log("Staging runtime prerequisite observation smoke test passed.");
} finally {
  for (const candidate of [
    path.dirname(sidecarPath),
    path.join(releaseRoot, releaseSha),
    releaseRoot,
  ]) {
    if (fs.existsSync(candidate)) {
      fs.chmodSync(candidate, 0o755);
    }
  }
  if (fs.existsSync(sidecarPath)) {
    fs.chmodSync(sidecarPath, 0o755);
  }
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}
