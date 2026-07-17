#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const script = path.join(
  repoRoot,
  "deploy/sidecar/scripts/staging-runtime-readiness-evidence-smoke.sh"
);
const tmpRoot = fs.mkdtempSync(path.join(repoRoot, ".tmp-staging-runtime-evidence-"));
const releaseSha = "0123456789abcdef0123456789abcdef01234567";
const envFile = path.join(tmpRoot, "message-sidecar-staging.env");
const releaseRoot = path.join(tmpRoot, "qintopia-agent-os-staging-releases");
const sidecarPath = path.join(
  releaseRoot,
  releaseSha,
  "sidecar",
  "qintopia-message-sidecar"
);
const secretValue = "staging-runtime-evidence-secret-must-not-appear";
const databaseHash = "a".repeat(64);

const runEvidence = (extraEnv = {}) =>
  spawnSync("bash", [script], {
    cwd: repoRoot,
    env: {
      PATH: process.env.PATH,
      QINTOPIA_STAGING_RUNTIME_READINESS_EVIDENCE_ENABLE: "1",
      QINTOPIA_STAGING_RUNTIME_READINESS_EVIDENCE_TEST_MODE: "1",
      QINTOPIA_STAGING_RUNTIME_READINESS_ENV_FILE: envFile,
      QINTOPIA_STAGING_RUNTIME_READINESS_RELEASE_ROOT: releaseRoot,
      QINTOPIA_STAGING_RUNTIME_RELEASE_SHA: releaseSha,
      QINTOPIA_STAGING_RUNTIME_DATABASE_URL_SHA256: databaseHash,
      ...extraEnv,
    },
    encoding: "utf8",
  });

const parseReport = (result) => {
  const line = result.stdout
    .split(/\r?\n/)
    .find((entry) => entry.startsWith("staging_runtime_readiness_evidence="));
  if (!line) {
    throw new Error(
      `missing staging runtime readiness evidence\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`
    );
  }
  return JSON.parse(line.slice("staging_runtime_readiness_evidence=".length));
};

try {
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
    mode: 0o555,
  });
  fs.chmodSync(releaseRoot, 0o755);
  fs.chmodSync(path.join(releaseRoot, releaseSha), 0o555);
  fs.chmodSync(path.dirname(sidecarPath), 0o555);
  fs.chmodSync(sidecarPath, 0o555);
  const sidecarHash = crypto
    .createHash("sha256")
    .update(fs.readFileSync(sidecarPath))
    .digest("hex");

  let result = runEvidence({
    QINTOPIA_STAGING_RUNTIME_SIDECAR_SHA256: sidecarHash,
  });
  if (result.status !== 0) {
    throw new Error(
      `ready evidence failed\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`
    );
  }
  let report = parseReport(result);
  if (
    report.success !== true ||
    report.action_status !== "ready_for_huabaosi_qiwe_staging_smokes" ||
    report.release_sha !== releaseSha ||
    report.packaged_sidecar_sha256 !== sidecarHash ||
    report.staging_database_url_sha256 !== databaseHash ||
    report.reports.length !== 3 ||
    !report.reports.every((entry) => entry.success === true) ||
    `${result.stdout}\n${result.stderr}`.includes(secretValue)
  ) {
    throw new Error(`ready evidence is invalid: ${JSON.stringify(report)}`);
  }

  result = runEvidence({
    QINTOPIA_STAGING_RUNTIME_SIDECAR_SHA256: "f".repeat(64),
  });
  report = parseReport(result);
  if (
    result.status === 0 ||
    report.success !== false ||
    report.action_status !== "not_ready" ||
    !report.limitations.some((entry) => entry.includes("sidecar_hash_mismatch"))
  ) {
    throw new Error(`hash mismatch evidence is invalid: ${JSON.stringify(report)}`);
  }

  fs.unlinkSync(envFile);
  result = runEvidence({
    QINTOPIA_STAGING_RUNTIME_SIDECAR_SHA256: sidecarHash,
  });
  report = parseReport(result);
  const prerequisiteReport = report.reports.find(
    (entry) => entry.label === "prerequisite"
  );
  if (
    result.status === 0 ||
    report.success !== false ||
    prerequisiteReport?.success !== false ||
    prerequisiteReport?.action_status !== "not_ready" ||
    !report.limitations.includes("prerequisite_env_file_path_missing")
  ) {
    throw new Error(`missing env evidence is invalid: ${JSON.stringify(report)}`);
  }
  fs.writeFileSync(
    envFile,
    [
      `QINTOPIA_SIDECAR_DATABASE_URL=postgres://user:${secretValue}@127.0.0.1:5432/qintopia_staging`,
      `QINTOPIA_HUABAOSI_IMAGE_API_KEY=$(echo ${secretValue})`,
      "",
    ].join("\n"),
    { encoding: "utf8", mode: 0o600 }
  );

  result = runEvidence({
    QINTOPIA_STAGING_RUNTIME_SIDECAR_SHA256: sidecarHash,
    QINTOPIA_STAGING_RUNTIME_DATABASE_URL_SHA256: "",
  });
  if (
    result.status === 0 ||
    !result.stderr.includes(
      "QINTOPIA_STAGING_RUNTIME_DATABASE_URL_SHA256 must be a canonical SHA-256"
    )
  ) {
    throw new Error("expected missing staging database hash to fail");
  }

  console.log("Staging runtime readiness evidence smoke test passed.");
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
