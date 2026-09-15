#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const script = path.join(repoRoot, "deploy/runner/wait-deploy-result.sh");
const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "qintopia-wait-result-test-"));
const signingKey = "test-signing-key";
const signingKeyId = "production";
const createdAt = new Date().toISOString();

const request = {
  schema_version: 1,
  request_id: "deploy-20260724T010203Z-0123456789ab",
  environment: "production",
  repository: "qintopia-agent-studio/qintopia-agent-os",
  requested_by: "codex",
  created_at: createdAt,
  expires_at: new Date(Date.parse(createdAt) + 60 * 60 * 1000).toISOString(),
  commit_sha: "0123456789abcdef0123456789abcdef01234567",
  runtime_sha: "0123456789abcdef0123456789abcdef01234567",
  runtime_artifact_profile: "qiwe-production",
  deploy_bundle_sha: "89abcdef0123456789abcdef0123456789abcdef",
  release_sha: "fedcba9876543210fedcba9876543210fedcba98",
  release_scope: ["sidecar-runtime", "deploy-bundle", "hermes-plugins"],
  restart_targets: ["qintopia-system-services", "hermes-erhua"],
  rollback_on_smoke_failure: true,
  dry_run: false,
  cos: {
    bucket: "qintopia-agent-os-artifacts-1305166808",
    region: "ap-shanghai",
    prefix: "qintopia-agent-os",
    request_key:
      "qintopia-agent-os/deploy-requests/production/requests/deploy-20260724T010203Z-0123456789ab.json",
    result_key:
      "qintopia-agent-os/deploy-results/production/deploy-20260724T010203Z-0123456789ab.json",
  },
  signature: {
    algorithm: "hmac-sha256",
    issuer: "github-actions",
    key_id: "production",
    signed_at: createdAt,
    value: "0".repeat(64),
  },
};

const requestFile = path.join(tmpRoot, "request.json");
fs.writeFileSync(requestFile, `${JSON.stringify(request, null, 2)}\n`, "utf8");

const canonicalJson = (value) => {
  if (Array.isArray(value)) {
    return `[${value.map(canonicalJson).join(",")}]`;
  }
  if (value && typeof value === "object") {
    return `{${Object.keys(value)
      .sort()
      .map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`)
      .join(",")}}`;
  }
  return JSON.stringify(value);
};

const signResult = (value) => {
  const result = { ...value };
  delete result.signature;
  const signatureMetadata = {
    algorithm: "hmac-sha256",
    issuer: "qintopia-deploy-runner",
    key_id: signingKeyId,
    signed_at: result.finished_at,
  };
  return {
    ...result,
    signature: {
      ...signatureMetadata,
      value: crypto
        .createHmac("sha256", signingKey)
        .update(canonicalJson({ result, signature: signatureMetadata }))
        .digest("hex"),
    },
  };
};

const goodResult = signResult({
  schema_version: 1,
  request_id: request.request_id,
  environment: "production",
  status: "succeeded",
  started_at: "2026-07-24T01:03:00Z",
  finished_at: "2026-07-24T01:03:30Z",
  release_sha: request.release_sha,
  commit_sha: request.commit_sha,
  runtime_sha: request.runtime_sha,
  runtime_artifact_profile: request.runtime_artifact_profile,
  deploy_bundle_sha: request.deploy_bundle_sha,
  release_scope: request.release_scope,
  previous_sha: "abcdef0123456789abcdef0123456789abcdef01",
  current_target: "/home/ubuntu/qintopia-agent-os-releases/current",
  restart_targets: request.restart_targets,
  checks: [{ name: "deploy-runner", status: "passed" }],
  rollback: { attempted: false, status: "not_needed" },
});

const badResult = signResult({
  ...goodResult,
  runtime_artifact_profile: "huabaosi-production",
});

const tamperedResult = {
  ...goodResult,
  status: "failed",
};

const invalidRequest = {
  ...request,
  commit_sha: "invalid-commit-sha",
  runtime_sha: "invalid-runtime-sha",
  runtime_artifact_profile: "invalid-profile",
  deploy_bundle_sha: "invalid-deploy-bundle-sha",
  release_sha: "invalid-release-sha",
  release_scope: [],
  restart_targets: [],
};

const invalidRequestFile = path.join(tmpRoot, "invalid-request.json");
fs.writeFileSync(
  invalidRequestFile,
  `${JSON.stringify(invalidRequest, null, 2)}\n`,
  "utf8"
);

const normalizedValidationFailureResult = signResult({
  schema_version: 1,
  request_id: invalidRequest.request_id,
  environment: "production",
  status: "failed",
  started_at: "2026-07-24T01:03:00Z",
  finished_at: "2026-07-24T01:03:01Z",
  release_sha: "0".repeat(40),
  commit_sha: "0".repeat(40),
  runtime_sha: "0".repeat(40),
  runtime_artifact_profile: "huabaosi-production",
  deploy_bundle_sha: "0".repeat(40),
  release_scope: ["sidecar-runtime"],
  previous_sha: "",
  current_target: "",
  restart_targets: ["qintopia-system-services"],
  checks: [{ name: "deploy-request-validation", status: "failed" }],
  rollback: { attempted: false, status: "not_needed" },
  error: "deploy request key or identity is invalid",
});

const badNormalizedValidationFailureResult = signResult({
  ...normalizedValidationFailureResult,
  runtime_artifact_profile: "qiwe-production",
});

const writeJson = (name, value) => {
  const filePath = path.join(tmpRoot, name);
  fs.writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`, "utf8");
  return filePath;
};

const goodResultFile = writeJson("good-result.json", goodResult);
const badResultFile = writeJson("bad-result.json", badResult);
const tamperedResultFile = writeJson("tampered-result.json", tamperedResult);
const normalizedValidationFailureResultFile = writeJson(
  "normalized-validation-failure-result.json",
  normalizedValidationFailureResult
);
const badNormalizedValidationFailureResultFile = writeJson(
  "bad-normalized-validation-failure-result.json",
  badNormalizedValidationFailureResult
);

const fakeCoscli = path.join(tmpRoot, "fake-coscli");
fs.writeFileSync(
  fakeCoscli,
  `#!/usr/bin/env bash
set -euo pipefail
if [[ "\${1:-}" == "config" ]]; then
  exit 0
fi
if [[ "\${1:-}" != "cp" ]]; then
  exit 64
fi
source_path="\${2:-}"
dest_path="\${3:-}"
if [[ "$source_path" != *"/deploy-20260724T010203Z-0123456789ab.json" ]]; then
  echo "unexpected source path: $source_path" >&2
  exit 65
fi
/bin/cp "\${FAKE_RESULT_PATH}" "$dest_path"
`,
  "utf8"
);
fs.chmodSync(fakeCoscli, 0o755);

const run = (resultPath, runRequestFile = requestFile) =>
  spawnSync("bash", [script, "--request-file", runRequestFile], {
    cwd: repoRoot,
    encoding: "utf8",
    env: {
      ...process.env,
      COSCLI_PATH: fakeCoscli,
      FAKE_RESULT_PATH: resultPath,
      TENCENT_COS_BUCKET: "qintopia-agent-os-artifacts-1305166808",
      TENCENT_COS_REGION: "ap-shanghai",
      TENCENT_COS_SECRET_ID: "test-secret-id",
      TENCENT_COS_SECRET_KEY: "test-secret-key",
      DEPLOY_REQUEST_SIGNING_KEY: signingKey,
      DEPLOY_REQUEST_SIGNING_KEY_ID: signingKeyId,
      DEPLOY_RESULT_TIMEOUT_SECONDS: "5",
      DEPLOY_RESULT_POLL_SECONDS: "1",
    },
  });

try {
  const success = run(goodResultFile);
  if (success.status !== 0) {
    throw new Error(
      `expected success\nstdout:\n${success.stdout}\nstderr:\n${success.stderr}`
    );
  }
  if (!success.stdout.includes("Deploy result succeeded: succeeded")) {
    throw new Error("success path did not report succeeded status");
  }

  const tampered = run(tamperedResultFile);
  if (
    tampered.status === 0 ||
    !tampered.stderr.includes("deploy result signature verification failed")
  ) {
    throw new Error(
      `tampered result was not rejected by signature verification\n${tampered.stderr}`
    );
  }

  const mismatch = run(badResultFile);
  if (mismatch.status === 0) {
    throw new Error("expected runtime_artifact_profile mismatch to fail");
  }
  if (!mismatch.stderr.includes("deploy result runtime_artifact_profile mismatch")) {
    throw new Error(
      `expected runtime_artifact_profile mismatch error\nstdout:\n${mismatch.stdout}\nstderr:\n${mismatch.stderr}`
    );
  }

  const normalizedFailure = run(
    normalizedValidationFailureResultFile,
    invalidRequestFile
  );
  if (normalizedFailure.status !== 1) {
    throw new Error(
      `expected normalized validation failure to be consumed as failed\nstdout:\n${normalizedFailure.stdout}\nstderr:\n${normalizedFailure.stderr}`
    );
  }
  if (!normalizedFailure.stderr.includes("Deploy result failed: failed")) {
    throw new Error("normalized validation failure did not report failed status");
  }
  if (
    normalizedFailure.stderr.includes("deploy result runtime_artifact_profile mismatch")
  ) {
    throw new Error(
      `normalized validation failure should not be rejected as mismatch\nstdout:\n${normalizedFailure.stdout}\nstderr:\n${normalizedFailure.stderr}`
    );
  }

  const badNormalizedFailure = run(
    badNormalizedValidationFailureResultFile,
    invalidRequestFile
  );
  if (badNormalizedFailure.status === 0) {
    throw new Error("expected mismatched normalized validation failure to be rejected");
  }
  if (
    !badNormalizedFailure.stderr.includes(
      "deploy result runtime_artifact_profile mismatch"
    )
  ) {
    throw new Error(
      `expected normalized validation mismatch error\nstdout:\n${badNormalizedFailure.stdout}\nstderr:\n${badNormalizedFailure.stderr}`
    );
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Wait deploy result tests passed.");
