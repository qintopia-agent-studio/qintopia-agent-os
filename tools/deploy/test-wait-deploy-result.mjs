#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const script = path.join(repoRoot, "deploy/runner/wait-deploy-result.sh");
const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "qintopia-wait-result-test-"));

const request = {
  schema_version: 1,
  request_id: "deploy-20260724T010203Z-0123456789ab",
  environment: "production",
  repository: "qintopia-agent-studio/qintopia-agent-os",
  requested_by: "codex",
  created_at: "2026-07-24T01:02:03Z",
  expires_at: "2099-07-24T02:02:03Z",
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
    signed_at: "2026-07-24T01:02:03Z",
    value: "0".repeat(64),
  },
};

const requestFile = path.join(tmpRoot, "request.json");
fs.writeFileSync(requestFile, `${JSON.stringify(request, null, 2)}\n`, "utf8");

const goodResult = {
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
};

const badResult = {
  ...goodResult,
  runtime_artifact_profile: "huabaosi-production",
};

const writeJson = (name, value) => {
  const filePath = path.join(tmpRoot, name);
  fs.writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`, "utf8");
  return filePath;
};

const goodResultFile = writeJson("good-result.json", goodResult);
const badResultFile = writeJson("bad-result.json", badResult);

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

const run = (resultPath) =>
  spawnSync("bash", [script, "--request-file", requestFile], {
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

  const mismatch = run(badResultFile);
  if (mismatch.status === 0) {
    throw new Error("expected runtime_artifact_profile mismatch to fail");
  }
  if (!mismatch.stderr.includes("deploy result runtime_artifact_profile mismatch")) {
    throw new Error(
      `expected runtime_artifact_profile mismatch error\nstdout:\n${mismatch.stdout}\nstderr:\n${mismatch.stderr}`
    );
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Wait deploy result tests passed.");
