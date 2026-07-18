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
  "deploy/sidecar/scripts/render-staging-runtime-env.py"
);
const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "qintopia-staging-env-"));
const secretValue = "render-secret-must-not-appear";
const databaseUrl = `postgres://staging_user:${secretValue}@127.0.0.1:5432/qintopia_staging`;
const databaseHash = crypto.createHash("sha256").update(databaseUrl).digest("hex");
const valuesPath = path.join(tmpRoot, "values.json");
const outputPath = path.join(tmpRoot, "message-sidecar-staging.env");
const feishuKey = (suffix) => `QINTOPIA_HUABAOSI_${"FEISHU"}_${suffix}`;

const values = {
  QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED: "1",
  QINTOPIA_SIDECAR_DATABASE_URL: databaseUrl,
  QINTOPIA_HUABAOSI_IMAGE_PROVIDER: "openai-compatible",
  QINTOPIA_HUABAOSI_IMAGE_MODEL: "gpt-image-2",
  QINTOPIA_HUABAOSI_IMAGE_API_BASE_URL: "https://image.example.test/v1",
  QINTOPIA_HUABAOSI_IMAGE_API_KEY: secretValue,
  QINTOPIA_HUABAOSI_IMAGE_STORAGE_BACKEND: "feishu-base",
  QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED: "1",
  QINTOPIA_HUABAOSI_FEISHU_MIRROR_APPROVAL: "approved-huabaosi-feishu-artifact-mirror",
  QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA:
    "0123456789abcdef0123456789abcdef01234567",
  QINTOPIA_DEPLOYED_COMMIT_SHA: "0123456789abcdef0123456789abcdef01234567",
  QINTOPIA_HUABAOSI_FEISHU_DATABASE_URL_SHA256: databaseHash,
  [feishuKey("BASE_TOKEN")]: secretValue,
  [feishuKey("ALLOWED_BASE_TOKENS")]: secretValue,
  [feishuKey("ARTIFACT_TABLE_ID")]: "tblFixture",
  [feishuKey("ALLOWED_ARTIFACT_TABLE_IDS")]: "tblFixture",
  QINTOPIA_HUABAOSI_FEISHU_PROFILE_ENV_PATH:
    "/home/ubuntu/.hermes/profiles/huabaosi/.env",
  QINTOPIA_HUABAOSI_FEISHU_SCHEMA_VERSION: "huabaosi-generated-image-v1",
  QINTOPIA_HUABAOSI_MEDIA_MAX_BYTES: "5000000",
  QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS: "media.example.test,cloud.example.test",
  QINTOPIA_QIWE_IMAGE_SEND_ENABLED: "1",
  QINTOPIA_QIWE_IMAGE_SEND_WEBHOOK_READY: "1",
  QIWE_API_URL: "https://qiwe.example.test",
  QIWE_TOKEN: secretValue,
  QIWE_GUID: "staging-guid",
  QINTOPIA_QIWE_IMAGE_SEND_ALLOWED_HOSTS: "qiwe.example.test",
  QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS: "isolated-staging-group",
};

const templatePath = path.join(
  repoRoot,
  "docs/operations/message-sidecar-staging-values.template.json"
);
const templateValues = JSON.parse(fs.readFileSync(templatePath, "utf8"));
if (
  JSON.stringify(Object.keys(templateValues).sort()) !==
  JSON.stringify(Object.keys(values).sort())
) {
  throw new Error("staging values template keys drifted from the renderer contract");
}
if (!Object.values(templateValues).some((value) => String(value).includes("<"))) {
  throw new Error("staging values template must keep non-secret placeholders");
}

const writeValues = (filePath, data) => {
  fs.writeFileSync(filePath, `${JSON.stringify(data, null, 2)}\n`, {
    encoding: "utf8",
    mode: 0o600,
  });
};

const runRenderer = (args) =>
  spawnSync("python3", [script, ...args], {
    cwd: repoRoot,
    encoding: "utf8",
  });

const parseReport = (result) => {
  const line = result.stdout
    .split(/\r?\n/)
    .find((entry) => entry.startsWith("staging_runtime_env_render="));
  if (!line) {
    throw new Error(
      `missing render report\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`
    );
  }
  return JSON.parse(line.slice("staging_runtime_env_render=".length));
};

try {
  writeValues(valuesPath, values);

  let result = runRenderer([
    "--values",
    valuesPath,
    "--expected-database-url-sha256",
    databaseHash,
  ]);
  let report = parseReport(result);
  if (
    result.status !== 0 ||
    report.success !== true ||
    report.action_status !== "staging_env_render_ready" ||
    report.database_url_sha256 !== databaseHash ||
    report.key_count !== Object.keys(values).length ||
    report.media_host_count !== 2 ||
    fs.existsSync(outputPath) ||
    `${result.stdout}\n${result.stderr}`.includes(secretValue)
  ) {
    throw new Error(`check-only render report invalid: ${JSON.stringify(report)}`);
  }

  result = runRenderer([
    "--values",
    valuesPath,
    "--expected-database-url-sha256",
    databaseHash,
    "--output",
    outputPath,
    "--apply",
    "--approval",
    "approved-staging-runtime-env-provision",
    "--test-mode",
  ]);
  report = parseReport(result);
  if (
    result.status !== 0 ||
    report.action_status !== "staging_env_written" ||
    !fs.existsSync(outputPath) ||
    `${result.stdout}\n${result.stderr}`.includes(secretValue)
  ) {
    throw new Error(`apply render report invalid: ${JSON.stringify(report)}`);
  }
  const rendered = fs.readFileSync(outputPath, "utf8");
  if (
    !rendered.includes(`QINTOPIA_SIDECAR_DATABASE_URL=${databaseUrl}`) ||
    !rendered.includes("QINTOPIA_HUABAOSI_IMAGE_STORAGE_BACKEND=feishu-base") ||
    !rendered.includes(
      "QINTOPIA_HUABAOSI_FEISHU_SCHEMA_VERSION=huabaosi-generated-image-v1"
    ) ||
    !rendered.includes(
      "QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS=isolated-staging-group"
    ) ||
    !rendered.includes(
      "QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS=media.example.test,cloud.example.test"
    ) ||
    (fs.statSync(outputPath).mode & 0o777) !== 0o600
  ) {
    throw new Error("rendered staging env file is invalid");
  }

  const badValuesPath = path.join(tmpRoot, "bad-values.json");
  writeValues(badValuesPath, { ...values, QINTOPIA_UNSUPPORTED_SECRET: secretValue });
  result = runRenderer([
    "--values",
    badValuesPath,
    "--expected-database-url-sha256",
    databaseHash,
  ]);
  report = parseReport(result);
  if (
    result.status === 0 ||
    report.success !== false ||
    !report.error.includes("unsupported keys") ||
    `${result.stdout}\n${result.stderr}`.includes(secretValue)
  ) {
    throw new Error(`unsupported key failure invalid: ${JSON.stringify(report)}`);
  }

  result = runRenderer([
    "--values",
    valuesPath,
    "--expected-database-url-sha256",
    "f".repeat(64),
  ]);
  report = parseReport(result);
  if (
    result.status === 0 ||
    report.success !== false ||
    !report.error.includes("hash does not match") ||
    `${result.stdout}\n${result.stderr}`.includes(secretValue)
  ) {
    throw new Error(`database hash failure invalid: ${JSON.stringify(report)}`);
  }

  const mismatchedReleasePath = path.join(tmpRoot, "mismatched-release-values.json");
  writeValues(mismatchedReleasePath, {
    ...values,
    QINTOPIA_DEPLOYED_COMMIT_SHA: "f".repeat(40),
  });
  result = runRenderer([
    "--values",
    mismatchedReleasePath,
    "--expected-database-url-sha256",
    databaseHash,
  ]);
  report = parseReport(result);
  if (
    result.status === 0 ||
    report.success !== false ||
    !report.error.includes("must match QINTOPIA_DEPLOYED_COMMIT_SHA") ||
    `${result.stdout}\n${result.stderr}`.includes(secretValue)
  ) {
    throw new Error(`release SHA mismatch failure invalid: ${JSON.stringify(report)}`);
  }

  const duplicateHostPath = path.join(tmpRoot, "duplicate-host-values.json");
  writeValues(duplicateHostPath, {
    ...values,
    QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS: "media.example.test,MEDIA.example.test",
  });
  result = runRenderer([
    "--values",
    duplicateHostPath,
    "--expected-database-url-sha256",
    databaseHash,
  ]);
  report = parseReport(result);
  if (
    result.status === 0 ||
    report.success !== false ||
    !report.error.includes("duplicate host entry") ||
    `${result.stdout}\n${result.stderr}`.includes(secretValue)
  ) {
    throw new Error(`duplicate host failure invalid: ${JSON.stringify(report)}`);
  }

  const invalidPortPath = path.join(tmpRoot, "invalid-port-values.json");
  writeValues(invalidPortPath, {
    ...values,
    QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS: "media.example.test:99999",
  });
  result = runRenderer([
    "--values",
    invalidPortPath,
    "--expected-database-url-sha256",
    databaseHash,
  ]);
  report = parseReport(result);
  if (
    result.status === 0 ||
    report.success !== false ||
    !report.error.includes("port outside 1-65535") ||
    `${result.stdout}\n${result.stderr}`.includes(secretValue)
  ) {
    throw new Error(`invalid host port failure invalid: ${JSON.stringify(report)}`);
  }

  const secondOutput = path.join(tmpRoot, "another-message-sidecar-staging.env");
  result = runRenderer([
    "--values",
    valuesPath,
    "--expected-database-url-sha256",
    databaseHash,
    "--output",
    secondOutput,
    "--apply",
    "--approval",
    "approved-staging-runtime-env-provision",
  ]);
  report = parseReport(result);
  if (
    result.status === 0 ||
    report.success !== false ||
    !report.error.includes("non-test apply may write only") ||
    fs.existsSync(secondOutput)
  ) {
    throw new Error(`non-test output guard invalid: ${JSON.stringify(report)}`);
  }

  const realParent = path.join(tmpRoot, "real-staging-parent");
  const symlinkParent = path.join(tmpRoot, "symlink-staging-parent");
  fs.mkdirSync(realParent, { mode: 0o700 });
  fs.symlinkSync(realParent, symlinkParent);
  result = runRenderer([
    "--values",
    valuesPath,
    "--expected-database-url-sha256",
    databaseHash,
    "--output",
    path.join(symlinkParent, "message-sidecar-staging.env"),
    "--apply",
    "--approval",
    "approved-staging-runtime-env-provision",
    "--test-mode",
  ]);
  report = parseReport(result);
  if (
    result.status === 0 ||
    report.success !== false ||
    !report.error.includes("output parent directory must not be a symlink")
  ) {
    throw new Error(`symlink parent guard invalid: ${JSON.stringify(report)}`);
  }

  console.log("Staging runtime env render test passed.");
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}
