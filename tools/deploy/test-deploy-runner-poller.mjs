#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { execFileSync, spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "qintopia-poller-test-"));

const writeExecutable = (relativePath, content) => {
  const filePath = path.join(tmpRoot, relativePath);
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, content, "utf8");
  fs.chmodSync(filePath, 0o755);
  return filePath;
};

const fakeCoscli = writeExecutable(
  "fake-coscli",
  `#!/usr/bin/env bash
set -euo pipefail

command_name="\${1:-}"
if [[ "$command_name" == "config" ]]; then
  exit 0
fi

if [[ "$command_name" != "cp" ]]; then
  echo "unsupported fake coscli command: $*" >&2
  exit 64
fi

source_path="\${2:-}"
dest_path="\${3:-}"

case "\${QINTOPIA_FAKE_COS_MODE:-}" in
  missing-pointer)
    echo "NoSuchKey: object not found" >&2
    exit 1
    ;;
  processed|failed|remote-result|active|invalid-request)
    if [[ "$source_path" == *"/qintopia-agent-os/deploy-requests/production/current.json" ]]; then
      cat >"$dest_path" <<'JSON'
{
  "schema_version": 1,
  "environment": "production",
  "repository": "qintopia-agent-studio/qintopia-agent-os",
  "request_id": "deploy-20260706T000000Z-0123456789ab",
  "request_key": "qintopia-agent-os/deploy-requests/production/requests/deploy-20260706T000000Z-0123456789ab.json",
  "result_key": "qintopia-agent-os/deploy-results/production/deploy-20260706T000000Z-0123456789ab.json"
}
JSON
      exit 0
    fi
    if [[ "$source_path" == *"/qintopia-agent-os/deploy-requests/production/requests/deploy-20260706T000000Z-0123456789ab.json" ]]; then
      if [[ "\${QINTOPIA_FAKE_COS_MODE:-}" == "invalid-request" ]]; then
        cat >"$dest_path" <<'JSON'
{
  "schema_version": 1,
  "environment": "production",
  "repository": "qintopia-agent-studio/qintopia-agent-os",
  "request_id": "deploy-20260706T000000Z-0123456789ab",
  "commit_sha": "0123456789abcdef0123456789abcdef01234567",
  "runtime_sha": "0123456789abcdef0123456789abcdef01234567",
  "runtime_artifact_profile": "qiwe-production",
  "deploy_bundle_sha": "89abcdef0123456789abcdef0123456789abcdef",
  "release_sha": "fedcba9876543210fedcba9876543210fedcba98",
  "release_scope": ["sidecar-runtime", "deploy-bundle", "hermes-plugins"],
  "restart_targets": ["qintopia-system-services", "hermes-erhua"],
  "cos": {
    "request_key": "qintopia-agent-os/deploy-requests/production/requests/deploy-20260706T000000Z-bad.json",
    "result_key": "qintopia-agent-os/deploy-results/production/deploy-20260706T000000Z-0123456789ab.json"
  }
}
JSON
        exit 0
      fi
      cat >"$dest_path" <<'JSON'
{
  "schema_version": 1,
  "environment": "production",
  "repository": "qintopia-agent-studio/qintopia-agent-os",
  "request_id": "deploy-20260706T000000Z-0123456789ab",
  "commit_sha": "0123456789abcdef0123456789abcdef01234567",
  "runtime_sha": "0123456789abcdef0123456789abcdef01234567",
  "runtime_artifact_profile": "qiwe-production",
  "deploy_bundle_sha": "89abcdef0123456789abcdef0123456789abcdef",
  "release_sha": "fedcba9876543210fedcba9876543210fedcba98",
  "release_scope": ["sidecar-runtime", "deploy-bundle", "hermes-plugins"],
  "restart_targets": ["qintopia-system-services", "hermes-erhua"],
  "cos": {
    "request_key": "qintopia-agent-os/deploy-requests/production/requests/deploy-20260706T000000Z-0123456789ab.json",
    "result_key": "qintopia-agent-os/deploy-results/production/deploy-20260706T000000Z-0123456789ab.json"
  }
}
JSON
      exit 0
    fi
    if [[ "$source_path" == *"/qintopia-agent-os/deploy-results/production/deploy-20260706T000000Z-0123456789ab.json" ]]; then
      if [[ "\${QINTOPIA_FAKE_COS_MODE:-}" == "remote-result" ]]; then
        cat >"$dest_path" <<'JSON'
{
  "schema_version": 1,
  "request_id": "deploy-20260706T000000Z-0123456789ab",
  "environment": "production",
  "status": "succeeded"
}
JSON
        exit 0
      fi
      if [[ " $* " == *" --disable-log "* ]]; then
        exit 1
      fi
      echo "NoSuchKey: object not found"
      exit 1
    fi
    if [[ "$dest_path" == *"/qintopia-agent-os/deploy-results/production/deploy-20260706T000000Z-0123456789ab.json" ]]; then
      mkdir -p "\${QINTOPIA_FAKE_COS_UPLOAD_DIR:-/tmp}/deploy-results"
      cp "$source_path" "\${QINTOPIA_FAKE_COS_UPLOAD_DIR:-/tmp}/deploy-results/deploy-20260706T000000Z-0123456789ab.json"
      exit 0
    fi
    echo "request should not be downloaded for already consumed state" >&2
    exit 65
    ;;
  *)
    echo "unknown fake mode: \${QINTOPIA_FAKE_COS_MODE:-}" >&2
    exit 66
    ;;
esac
`
);

const fakeRunner = writeExecutable(
  "fake-runner",
  `#!/usr/bin/env bash
set -euo pipefail
if [[ "\${QINTOPIA_FAKE_RUNNER_EXPECTED:-idle}" == "idle" ]]; then
  echo "runner should not execute for idle poller states" >&2
  exit 67
fi
request_file=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --request-file)
      request_file="\${2:-}"
      shift 2
      ;;
    *)
      shift
      ;;
  esac
done
if [[ -z "$request_file" ]]; then
  echo "missing --request-file" >&2
  exit 2
fi
python3 - "$request_file" <<'PY'
import json
import os
import sys

request_file = sys.argv[1]
state_dir = os.environ["QINTOPIA_DEPLOY_RUNNER_STATE_DIR"]
with open(request_file, encoding="utf-8") as fh:
    request = json.load(fh)
result_path = os.path.join(state_dir, "results", f"{request['request_id']}.json")
with open(result_path, "w", encoding="utf-8") as fh:
    json.dump(
        {
            "schema_version": 1,
            "request_id": request["request_id"],
            "environment": "production",
            "status": "dry_run_succeeded",
            "release_sha": request["release_sha"],
            "commit_sha": request["commit_sha"],
            "runtime_sha": request["runtime_sha"],
            "runtime_artifact_profile": request["runtime_artifact_profile"],
            "deploy_bundle_sha": request["deploy_bundle_sha"],
            "release_scope": request["release_scope"],
            "previous_sha": "",
            "current_target": "",
            "restart_targets": request["restart_targets"],
            "checks": [{"name": "deploy-runner", "status": "passed"}],
            "rollback": {"attempted": False, "status": "not_needed"},
        },
        fh,
    )
    fh.write("\\n")
PY
`
);

const baseEnv = {
  ...process.env,
  QINTOPIA_COS_ENV_FILE: path.join(tmpRoot, "missing.env"),
  QINTOPIA_DEPLOY_RUNNER_BIN: fakeRunner,
  COSCLI_PATH: fakeCoscli,
  TENCENT_COS_BUCKET: "qintopia-agent-os-artifacts-1305166808",
  TENCENT_COS_REGION: "ap-shanghai",
  TENCENT_COS_SECRET_ID: "test-secret-id",
  TENCENT_COS_SECRET_KEY: "test-secret-key",
  DEPLOY_REQUEST_SIGNING_KEY: "test-signing-key",
  DEPLOY_REQUEST_SIGNING_KEY_ID: "production",
};

const poller = path.join(repoRoot, "deploy/runner/poll-deploy-requests.sh");
const requestName = "deploy-20260706T000000Z-0123456789ab.json";

const runCase = ({ name, mode, archive, runnerExpected = "idle" }) => {
  const stateDir = path.join(tmpRoot, name);
  const uploadDir = path.join(tmpRoot, `${name}-uploads`);
  fs.mkdirSync(path.join(stateDir, "requests", "pending"), { recursive: true });
  fs.mkdirSync(path.join(stateDir, "requests", "processed"), { recursive: true });
  fs.mkdirSync(path.join(stateDir, "requests", "failed"), { recursive: true });
  fs.mkdirSync(path.join(stateDir, "results"), { recursive: true });
  fs.mkdirSync(uploadDir, { recursive: true });
  if (archive) {
    fs.writeFileSync(
      path.join(stateDir, "requests", archive, requestName),
      "{}\n",
      "utf8"
    );
  }

  const result = spawnSync("bash", [poller], {
    cwd: repoRoot,
    env: {
      ...baseEnv,
      QINTOPIA_DEPLOY_RUNNER_STATE_DIR: stateDir,
      QINTOPIA_FAKE_COS_MODE: mode,
      QINTOPIA_FAKE_COS_UPLOAD_DIR: uploadDir,
      QINTOPIA_FAKE_RUNNER_EXPECTED: runnerExpected,
    },
    encoding: "utf8",
  });

  if (result.status !== 0) {
    throw new Error(
      `${name}: expected idle success, got ${result.status}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`
    );
  }
  return { output: `${result.stdout}${result.stderr}`, stateDir, uploadDir };
};

try {
  const missingPointerOutput = runCase({
    name: "missing-pointer",
    mode: "missing-pointer",
  });
  if (!missingPointerOutput.output.includes("No deploy request pointer found; idle")) {
    throw new Error("missing-pointer: idle message was not emitted");
  }

  const processedOutput = runCase({
    name: "processed-pointer",
    mode: "processed",
    archive: "processed",
  });
  if (!processedOutput.output.includes("Deploy request already processed; idle")) {
    throw new Error("processed-pointer: processed idle message was not emitted");
  }

  const remoteResultOutput = runCase({
    name: "remote-result-pointer",
    mode: "remote-result",
  });
  if (
    !remoteResultOutput.output.includes("Deploy request result already exists; idle")
  ) {
    throw new Error(
      "remote-result-pointer: remote-result idle message was not emitted"
    );
  }

  const failedOutput = runCase({
    name: "failed-pointer",
    mode: "failed",
    archive: "failed",
  });
  if (!failedOutput.output.includes("Deploy request already failed; idle")) {
    throw new Error("failed-pointer: failed idle message was not emitted");
  }

  const activeOutput = runCase({
    name: "active-pointer",
    mode: "active",
    runnerExpected: "active",
  });
  const processedRequest = path.join(
    activeOutput.stateDir,
    "requests",
    "processed",
    requestName
  );
  if (!fs.existsSync(processedRequest)) {
    throw new Error("active-pointer: processed request archive was not written");
  }
  const uploadedResult = path.join(
    activeOutput.uploadDir,
    "deploy-results",
    requestName
  );
  if (!fs.existsSync(uploadedResult)) {
    throw new Error("active-pointer: deploy result was not uploaded");
  }
  const uploadedResultJson = JSON.parse(fs.readFileSync(uploadedResult, "utf8"));
  if (
    uploadedResultJson.runtime_artifact_profile !== "qiwe-production" ||
    uploadedResultJson.release_scope?.length !== 3
  ) {
    throw new Error(
      "active-pointer: uploaded deploy result did not retain identity fields"
    );
  }

  const invalidRequestStateDir = path.join(tmpRoot, "invalid-request");
  const invalidRequestUploadDir = path.join(tmpRoot, "invalid-request-uploads");
  fs.mkdirSync(path.join(invalidRequestStateDir, "requests", "pending"), {
    recursive: true,
  });
  fs.mkdirSync(path.join(invalidRequestStateDir, "requests", "processed"), {
    recursive: true,
  });
  fs.mkdirSync(path.join(invalidRequestStateDir, "requests", "failed"), {
    recursive: true,
  });
  fs.mkdirSync(path.join(invalidRequestStateDir, "results"), { recursive: true });
  fs.mkdirSync(invalidRequestUploadDir, { recursive: true });
  const invalidRequest = spawnSync("bash", [poller], {
    cwd: repoRoot,
    env: {
      ...baseEnv,
      QINTOPIA_DEPLOY_RUNNER_STATE_DIR: invalidRequestStateDir,
      QINTOPIA_FAKE_COS_MODE: "invalid-request",
      QINTOPIA_FAKE_COS_UPLOAD_DIR: invalidRequestUploadDir,
      QINTOPIA_FAKE_RUNNER_EXPECTED: "idle",
    },
    encoding: "utf8",
  });
  if (invalidRequest.status === 0) {
    throw new Error("invalid-request: expected invalid request to fail");
  }
  const invalidUploadedResult = path.join(
    invalidRequestUploadDir,
    "deploy-results",
    requestName
  );
  if (!fs.existsSync(invalidUploadedResult)) {
    throw new Error("invalid-request: fallback deploy result was not uploaded");
  }
  const invalidUploadedResultJson = JSON.parse(
    fs.readFileSync(invalidUploadedResult, "utf8")
  );
  if (
    invalidUploadedResultJson.status !== "failed" ||
    invalidUploadedResultJson.runtime_artifact_profile !== "qiwe-production" ||
    invalidUploadedResultJson.deploy_bundle_sha !==
      "89abcdef0123456789abcdef0123456789abcdef"
  ) {
    throw new Error(
      "invalid-request: fallback deploy result did not retain request identity"
    );
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

execFileSync("bash", ["-n", "deploy/runner/poll-deploy-requests.sh"], {
  cwd: repoRoot,
});

console.log("Deploy runner poller behavior test passed.");
