#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const script = path.join(
  repoRoot,
  "deploy/sidecar/scripts/huabaosi-image-generation-staging-readiness-smoke.sh"
);
const tempRoot = fs.mkdtempSync(
  path.join(repoRoot, ".tmp-huabaosi-staging-readiness-")
);
const releaseSha = "0123456789abcdef0123456789abcdef01234567";
const envFile = path.join(tempRoot, "message-sidecar-staging.env");
const releaseRoot = path.join(tempRoot, "qintopia-agent-os-staging-releases");
const sidecarPath = path.join(
  releaseRoot,
  releaseSha,
  "sidecar",
  "qintopia-message-sidecar"
);
const secretValue = "huabaosi-readiness-env-secret-must-not-appear";

const runReadiness = (extraEnv = {}) =>
  spawnSync("bash", [script], {
    cwd: repoRoot,
    env: {
      ...process.env,
      QINTOPIA_HUABAOSI_IMAGE_STAGING_READINESS_ENABLE: "1",
      QINTOPIA_HUABAOSI_IMAGE_STAGING_APPROVAL: "approved-staging-image-generation",
      QINTOPIA_HUABAOSI_IMAGE_STAGING_READINESS_TEST_MODE: "1",
      QINTOPIA_HUABAOSI_IMAGE_STAGING_READINESS_ENV_FILE: envFile,
      QINTOPIA_HUABAOSI_IMAGE_STAGING_READINESS_RELEASE_ROOT: releaseRoot,
      QINTOPIA_HUABAOSI_IMAGE_STAGING_RELEASE_SHA: releaseSha,
      ...extraEnv,
    },
    encoding: "utf8",
  });

const parseReport = (result) => {
  const line = result.stdout
    .split(/\r?\n/)
    .find((entry) => entry.startsWith("huabaosi_image_generation_staging_readiness="));
  if (!line) {
    throw new Error(
      `missing readiness report\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`
    );
  }
  return JSON.parse(line.slice("huabaosi_image_generation_staging_readiness=".length));
};

try {
  const missing = runReadiness({
    QINTOPIA_HUABAOSI_IMAGE_STAGING_SIDECAR_SHA256: "0".repeat(64),
  });
  if (missing.status === 0) {
    throw new Error("expected missing readiness inputs to fail");
  }
  const missingReport = parseReport(missing);
  if (
    missingReport.success !== false ||
    missingReport.env_file_present !== false ||
    missingReport.release_root_present !== false ||
    !missingReport.limitations.includes("env_file_path_missing") ||
    !missingReport.limitations.includes("release_root_path_missing")
  ) {
    throw new Error(
      `missing readiness report is invalid: ${JSON.stringify(missingReport)}`
    );
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

  const ready = runReadiness({
    QINTOPIA_HUABAOSI_IMAGE_STAGING_SIDECAR_SHA256: sidecarHash,
  });
  if (ready.status !== 0) {
    throw new Error(
      `expected ready inputs to pass\nstdout:\n${ready.stdout}\nstderr:\n${ready.stderr}`
    );
  }
  const readyReport = parseReport(ready);
  if (
    readyReport.success !== true ||
    readyReport.action_status !== "ready_for_staging_preflight" ||
    readyReport.env_file_secure !== true ||
    readyReport.release_root_secure !== true ||
    readyReport.sidecar_binary_secure !== true ||
    readyReport.sidecar_hash_matches !== true ||
    readyReport.sidecar_binary_sha256 !== sidecarHash ||
    readyReport.test_mode !== true
  ) {
    throw new Error(`ready report is invalid: ${JSON.stringify(readyReport)}`);
  }
  if (`${ready.stdout}\n${ready.stderr}`.includes(secretValue)) {
    throw new Error("readiness smoke exposed staging env contents");
  }

  fs.chmodSync(sidecarPath, 0o500);
  const ownerExecutable = runReadiness({
    QINTOPIA_HUABAOSI_IMAGE_STAGING_SIDECAR_SHA256: sidecarHash,
  });
  if (ownerExecutable.status !== 0) {
    throw new Error(
      `expected owner-executable sidecar to pass readiness\nstdout:\n${ownerExecutable.stdout}\nstderr:\n${ownerExecutable.stderr}`
    );
  }
  const ownerExecutableReport = parseReport(ownerExecutable);
  if (
    ownerExecutableReport.success !== true ||
    ownerExecutableReport.sidecar_binary_secure !== true ||
    ownerExecutableReport.sidecar_hash_matches !== true
  ) {
    throw new Error(
      `owner-executable report is invalid: ${JSON.stringify(ownerExecutableReport)}`
    );
  }
  fs.chmodSync(sidecarPath, 0o555);

  const parentLink = path.join(tempRoot, "linked-staging-parent");
  fs.symlinkSync(tempRoot, parentLink, "dir");
  const symlinkParentEnv = runReadiness({
    QINTOPIA_HUABAOSI_IMAGE_STAGING_READINESS_ENV_FILE: path.join(
      parentLink,
      "message-sidecar-staging.env"
    ),
    QINTOPIA_HUABAOSI_IMAGE_STAGING_READINESS_RELEASE_ROOT: path.join(
      parentLink,
      "qintopia-agent-os-staging-releases"
    ),
    QINTOPIA_HUABAOSI_IMAGE_STAGING_SIDECAR_SHA256: sidecarHash,
  });
  if (symlinkParentEnv.status === 0) {
    throw new Error("expected symlink parent path to fail readiness");
  }
  const symlinkParentReport = parseReport(symlinkParentEnv);
  if (
    symlinkParentReport.env_file_secure !== false ||
    !symlinkParentReport.limitations.includes("env_file_path_parent_is_symlink")
  ) {
    throw new Error(
      `symlink parent report is invalid: ${JSON.stringify(symlinkParentReport)}`
    );
  }

  fs.chmodSync(sidecarPath, 0o444);
  const notExecutable = runReadiness({
    QINTOPIA_HUABAOSI_IMAGE_STAGING_SIDECAR_SHA256: sidecarHash,
  });
  if (notExecutable.status === 0) {
    throw new Error("expected non-executable sidecar to fail readiness");
  }
  const notExecutableReport = parseReport(notExecutable);
  if (
    notExecutableReport.sidecar_binary_secure !== false ||
    !notExecutableReport.limitations.includes("sidecar_binary_path_not_executable")
  ) {
    throw new Error(
      `non-executable report is invalid: ${JSON.stringify(notExecutableReport)}`
    );
  }
  fs.chmodSync(sidecarPath, 0o555);

  fs.chmodSync(sidecarPath, 0o755);
  const ownerWritable = runReadiness({
    QINTOPIA_HUABAOSI_IMAGE_STAGING_SIDECAR_SHA256: sidecarHash,
  });
  if (ownerWritable.status === 0) {
    throw new Error("expected owner-writable sidecar to fail readiness");
  }
  const ownerWritableReport = parseReport(ownerWritable);
  if (
    ownerWritableReport.sidecar_binary_secure !== false ||
    !ownerWritableReport.limitations.includes(
      "sidecar_binary_path_owner_group_or_world_writable"
    )
  ) {
    throw new Error(
      `owner-writable report is invalid: ${JSON.stringify(ownerWritableReport)}`
    );
  }
  fs.chmodSync(sidecarPath, 0o555);

  const mismatch = runReadiness({
    QINTOPIA_HUABAOSI_IMAGE_STAGING_SIDECAR_SHA256: "f".repeat(64),
  });
  if (mismatch.status === 0) {
    throw new Error("expected sidecar hash mismatch to fail");
  }
  const mismatchReport = parseReport(mismatch);
  if (
    mismatchReport.sidecar_hash_matches !== false ||
    !mismatchReport.limitations.includes("sidecar_hash_mismatch")
  ) {
    throw new Error(
      `hash mismatch report is invalid: ${JSON.stringify(mismatchReport)}`
    );
  }

  console.log("Huabaosi image staging readiness smoke test passed.");
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
  fs.rmSync(tempRoot, { recursive: true, force: true });
}
