#!/usr/bin/env node

import fs from "node:fs";
import assert from "node:assert/strict";
import crypto from "node:crypto";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { execFileSync, spawn, spawnSync } from "node:child_process";

const repoRoot = process.cwd();
// A skipped service activation must not exhaust the recurring poll schedule.
const pollTimer = fs.readFileSync(
  path.join(repoRoot, "deploy/runner/qintopia-agent-os-deploy-runner.timer"),
  "utf8"
);
assert.match(pollTimer, /^OnCalendar=minutely$/m);
assert.doesNotMatch(pollTimer, /^OnUnit(?:Active|Inactive)Sec=/m);
const tmpRoot = fs.realpathSync(
  fs.mkdtempSync(path.join(os.tmpdir(), "qintopia-poller-test-"))
);

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

# COSCLI logs relative to its executable unless --log-path is explicit, not cwd.
# Model an immutable installation even when this fixture runs as root.
log_path=""
args=("$@")
for ((i=0; i<\${#args[@]}; i++)); do
  if [[ "\${args[i]}" == --log-path ]]; then log_path="\${args[i+1]}"; fi
done
if [[ -z "$log_path" || ! -d "$log_path" || ! -w "$log_path" ]]; then
  echo 'coscli.log: read-only file system' >&2
  exit 73
fi

case "\${QINTOPIA_FAKE_COS_MODE:-}" in
  missing-pointer)
    echo "NoSuchKey: object not found" >&2
    exit 1
    ;;
  processed|failed|remote-result|active|invalid-request|hermes-early-failure)
    if [[ "$source_path" == *"/qintopia-agent-os/deploy-requests/production/current.json" ]]; then
      if [[ "\${QINTOPIA_FAKE_COS_MODE:-}" == "hermes-early-failure" ]]; then
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
      if [[ "\${QINTOPIA_FAKE_COS_MODE:-}" == "hermes-early-failure" ]]; then
        cat >"$dest_path" <<'JSON'
{
  "schema_version": 1,
  "request_id": "deploy-20260706T000000Z-0123456789ab",
  "environment": "production",
  "repository": "qintopia-agent-studio/qintopia-agent-os",
  "requested_by": "fixture",
  "created_at": "2026-07-06T00:00:00Z",
  "expires_at": "2026-07-06T00:30:00Z",
  "release_scope": ["hermes-core-release"],
  "restart_targets": ["hermes-core"],
  "rollback_on_smoke_failure": true,
  "dry_run": false,
  "hermes_core_release": {
    "repository": "https://github.com/NousResearch/hermes-agent.git",
    "tag": "v0.1.0",
    "commit_sha": "abcdef0123456789abcdef0123456789abcdef01",
    "source_archive_sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    "artifact_identity_sha256": "123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0",
    "artifact_manifest_sha256": "23456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef01",
    "previous_version": "0.0.9",
    "previous_commit_sha": "fedcba9876543210fedcba9876543210fedcba98",
    "archive_sha256": "3456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef012"
  },
  "cos": {
    "bucket": "fixture",
    "region": "fixture",
    "prefix": "qintopia-agent-os",
    "request_key": "qintopia-agent-os/deploy-requests/production/requests/deploy-20260706T000000Z-0123456789ab.json",
    "result_key": "qintopia-agent-os/deploy-results/production/deploy-20260706T000000Z-0123456789ab.json"
  },
  "signature": {
    "algorithm": "hmac-sha256",
    "issuer": "github-actions",
    "key_id": "fixture",
    "signed_at": "2026-07-06T00:00:00Z",
    "value": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
  }
}
JSON
        exit 0
      fi
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
      uploaded="\${QINTOPIA_FAKE_COS_UPLOAD_DIR:-/tmp}/deploy-results/deploy-20260706T000000Z-0123456789ab.json"
      if [[ -f "$uploaded" ]]; then
        cp "$uploaded" "$dest_path"
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
if [[ "\${QINTOPIA_FAKE_RUNNER_EXPECTED:-idle}" == "early-fail" ]]; then
  echo "simulated runner failure before result creation" >&2
  exit 75
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
import hashlib
import hmac
import json
import os
import sys

request_file = sys.argv[1]
state_dir = os.environ["QINTOPIA_DEPLOY_RUNNER_STATE_DIR"]
with open(request_file, encoding="utf-8") as fh:
    request = json.load(fh)
result_path = os.path.join(state_dir, "results", f"{request['request_id']}.json")
finished_at = "2026-07-06T00:01:00Z"
result = {
    "schema_version": 1,
    "request_id": request["request_id"],
    "environment": "production",
    "status": "dry_run_succeeded",
    "started_at": "2026-07-06T00:00:00Z",
    "finished_at": finished_at,
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
}
metadata = {
    "algorithm": "hmac-sha256",
    "issuer": "qintopia-deploy-runner",
    "key_id": os.environ["DEPLOY_REQUEST_SIGNING_KEY_ID"],
    "signed_at": finished_at,
}
def canonical(value):
    if isinstance(value, list):
        return "[" + ",".join(canonical(item) for item in value) + "]"
    if isinstance(value, dict):
        return "{" + ",".join(json.dumps(key, separators=(",", ":")) + ":" + canonical(value[key]) for key in sorted(value)) + "}"
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))
result["signature"] = {
    **metadata,
    "value": hmac.new(os.environ["DEPLOY_REQUEST_SIGNING_KEY"].encode(), canonical({"result": result, "signature": metadata}).encode(), hashlib.sha256).hexdigest(),
}
with open(result_path, "w", encoding="utf-8") as fh:
    json.dump(result, fh)
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
delete baseEnv.INVOCATION_ID;

const poller = path.join(repoRoot, "deploy/runner/poll-deploy-requests.sh");
const requestName = "deploy-20260706T000000Z-0123456789ab.json";

const runCase = ({
  name,
  mode,
  archive,
  runnerExpected = "idle",
  expectedId = "",
  preexistingClaim = false,
  fixedPreparing = false,
  expectedStatus = 0,
}) => {
  const stateDir = path.join(tmpRoot, name);
  const uploadDir = path.join(tmpRoot, `${name}-uploads`);
  fs.mkdirSync(path.join(stateDir, "requests", "pending"), { recursive: true });
  fs.mkdirSync(path.join(stateDir, "requests", "processed"), { recursive: true });
  fs.mkdirSync(path.join(stateDir, "requests", "failed"), { recursive: true });
  fs.mkdirSync(path.join(stateDir, "requests", "claimed"), { recursive: true });
  fs.mkdirSync(path.join(stateDir, "results"), { recursive: true });
  fs.mkdirSync(uploadDir, { recursive: true });
  if (archive) {
    fs.writeFileSync(
      path.join(stateDir, "requests", archive, requestName),
      "{}\n",
      "utf8"
    );
  }
  if (fixedPreparing) {
    fs.mkdirSync(path.join(stateDir, "recovery"), { recursive: true });
    fs.writeFileSync(
      path.join(stateDir, "recovery/takeover.json"),
      JSON.stringify({
        request_id: expectedId,
        hold_token: "a".repeat(32),
        phase: "preparing",
      })
    );
  }
  if (preexistingClaim) {
    fs.writeFileSync(
      path.join(stateDir, "requests", "claimed", requestName),
      '{"phase":"possibly_executing"}\n'
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
      QINTOPIA_EXPECTED_DEPLOY_REQUEST_ID: expectedId,
    },
    encoding: "utf8",
  });

  if (result.status !== expectedStatus) {
    throw new Error(
      `${name}: expected ${expectedStatus}, got ${result.status}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`
    );
  }
  return { output: `${result.stdout}${result.stderr}`, stateDir, uploadDir };
};

try {
  const claimedOutput = runCase({
    name: "unfinished-claim",
    mode: "active",
    preexistingClaim: true,
    expectedStatus: 75,
  });
  if (!claimedOutput.output.includes("unfinished deploy request claim")) {
    throw new Error("unfinished-claim: consumer did not stop before pointer read");
  }
  const wrongIdOutput = runCase({
    name: "wrong-expected-id",
    mode: "active",
    expectedId: "deploy-20260706T000000Z-ffffffffffff",
    expectedStatus: 75,
  });
  if (!wrongIdOutput.output.includes("does not match expected request ID")) {
    throw new Error("wrong-expected-id: pointer mismatch was not rejected");
  }
  const claimedFixed = runCase({
    name: "fixed-claim-before-marker",
    mode: "active",
    fixedPreparing: true,
    expectedId: requestName.slice(0, -5),
    expectedStatus: 75,
  });
  // The fixture runner has a deliberately wrong immutable identity: stop before
  // any execution, but verify the real claim/marker publication ordering.
  assert.match(claimedFixed.output, /fixed takeover staged runner identity mismatch/);
  assert.ok(
    fs.existsSync(path.join(claimedFixed.stateDir, "requests/claimed", requestName))
  );
  assert.equal(
    fs.readFileSync(
      path.join(claimedFixed.stateDir, "recovery/takeover-consumed"),
      "utf8"
    ),
    requestName.slice(0, -5) + "\n"
  );
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
  if (
    fs.readdirSync(path.join(activeOutput.stateDir, "requests", "claimed")).length !== 0
  ) {
    throw new Error("active-pointer: finalized claim was not removed");
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
      "89abcdef0123456789abcdef0123456789abcdef" ||
    invalidUploadedResultJson.signature?.algorithm !== "hmac-sha256" ||
    invalidUploadedResultJson.signature?.issuer !== "qintopia-deploy-runner" ||
    invalidUploadedResultJson.signature?.key_id !== "production" ||
    !/^[0-9a-f]{64}$/.test(invalidUploadedResultJson.signature?.value ?? "")
  ) {
    throw new Error(
      "invalid-request: fallback deploy result did not retain identity and signature"
    );
  }

  const hermesEarlyFailureStateDir = path.join(tmpRoot, "hermes-early-failure");
  const hermesEarlyFailureUploadDir = path.join(
    tmpRoot,
    "hermes-early-failure-uploads"
  );
  fs.mkdirSync(path.join(hermesEarlyFailureStateDir, "requests", "pending"), {
    recursive: true,
  });
  fs.mkdirSync(path.join(hermesEarlyFailureStateDir, "requests", "processed"), {
    recursive: true,
  });
  fs.mkdirSync(path.join(hermesEarlyFailureStateDir, "requests", "failed"), {
    recursive: true,
  });
  fs.mkdirSync(path.join(hermesEarlyFailureStateDir, "results"), { recursive: true });
  fs.mkdirSync(hermesEarlyFailureUploadDir, { recursive: true });
  const hermesEarlyFailure = spawnSync("bash", [poller], {
    cwd: repoRoot,
    env: {
      ...baseEnv,
      QINTOPIA_DEPLOY_RUNNER_STATE_DIR: hermesEarlyFailureStateDir,
      QINTOPIA_FAKE_COS_MODE: "hermes-early-failure",
      QINTOPIA_FAKE_COS_UPLOAD_DIR: hermesEarlyFailureUploadDir,
      QINTOPIA_FAKE_RUNNER_EXPECTED: "early-fail",
    },
    encoding: "utf8",
  });
  if (hermesEarlyFailure.status === 0) {
    throw new Error("hermes-early-failure: expected runner failure");
  }
  const hermesFallbackPath = path.join(
    hermesEarlyFailureUploadDir,
    "deploy-results",
    requestName
  );
  if (!fs.existsSync(hermesFallbackPath)) {
    throw new Error(
      `hermes-early-failure: fallback deploy result was not uploaded\nstdout:\n${hermesEarlyFailure.stdout}\nstderr:\n${hermesEarlyFailure.stderr}`
    );
  }
  if (
    !fs.existsSync(
      path.join(hermesEarlyFailureStateDir, "requests", "claimed", requestName)
    )
  ) {
    throw new Error(
      "hermes-early-failure: uncertain fallback lost its persistent claim"
    );
  }
  const hermesFallback = JSON.parse(fs.readFileSync(hermesFallbackPath, "utf8"));
  if (
    hermesFallback.status !== "failed" ||
    JSON.stringify(hermesFallback.release_scope) !==
      JSON.stringify(["hermes-core-release"]) ||
    JSON.stringify(hermesFallback.restart_targets) !==
      JSON.stringify(["hermes-core"]) ||
    !hermesFallback.hermes_core ||
    hermesFallback.release_sha !== undefined ||
    hermesFallback.runtime_artifact_profile !== undefined ||
    hermesFallback.hermes_core.transaction_status !== "failed" ||
    hermesFallback.hermes_core.tag !== "v0.1.0" ||
    hermesFallback.hermes_core.commit_sha !==
      "abcdef0123456789abcdef0123456789abcdef01" ||
    hermesFallback.hermes_core.source_archive_sha256 !==
      "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" ||
    hermesFallback.hermes_core.artifact_identity_sha256 !==
      "123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0" ||
    hermesFallback.hermes_core.artifact_manifest_sha256 !==
      "23456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef01" ||
    hermesFallback.hermes_core.previous_commit_sha !==
      "fedcba9876543210fedcba9876543210fedcba98" ||
    hermesFallback.hermes_core.new_version !== null ||
    /unknown|^0+$/.test(JSON.stringify(hermesFallback)) ||
    hermesFallback.signature?.issuer !== "qintopia-deploy-runner"
  ) {
    throw new Error(
      "hermes-early-failure: fallback deploy result was not Hermes-specific"
    );
  }
  // Run the actual launcher against local path/systemctl/COS boundaries. No VM,
  // production path, real credentials, systemd mutation or runner effect is used.
  const launchRoot = path.join(tmpRoot, "takeover");
  const state = path.join(launchRoot, "state");
  const recovery = path.join(state, "recovery");
  const releases = path.join(launchRoot, "releases");
  const units = path.join(launchRoot, "units");
  const proc = path.join(launchRoot, "proc");
  const cgroups = path.join(launchRoot, "cgroups");
  const privateEnv = path.join(launchRoot, "cos.env");
  const fixtureBin = path.join(launchRoot, "bin");
  const rid = "deploy-20261002T083059Z-16e8d56b9800";
  const nextId = "deploy-20261002T103059Z-16e8d56b9800";
  const o = "16e8d56b98001579c6288ba13199b80d6d3dfc74";
  const p = "83d694f2c3bc21fd78a73d25da3197379e2a14d5";
  const t = "70e7984fab92ddab956009585212d0e9729767b5";
  const token = "a".repeat(32);
  const key = "simulated-retirement-key";
  const write = (name, text, mode = 0o600) => {
    fs.mkdirSync(path.dirname(name), { recursive: true });
    if (fs.existsSync(name)) fs.chmodSync(name, 0o600);
    fs.writeFileSync(name, text, { mode });
    fs.chmodSync(name, mode);
  };
  const digest = (bytes) => crypto.createHash("sha256").update(bytes).digest("hex");
  const canonical = (value) =>
    Array.isArray(value)
      ? `[${value.map(canonical).join(",")}]`
      : value && typeof value === "object"
        ? `{${Object.keys(value)
            .sort()
            .map((k) => `${JSON.stringify(k)}:${canonical(value[k])}`)
            .join(",")}}`
        : JSON.stringify(value);
  const requestKey = `qintopia-agent-os/deploy-requests/production/requests/${rid}.json`;
  const resultKey = `qintopia-agent-os/deploy-results/production/${rid}.json`;
  function signedRequest(expired = true) {
    const end = new Date(Date.now() + (expired ? -600_000 : 600_000));
    const start = new Date(end.getTime() - 1_800_000).toISOString();
    const unsigned = {
      schema_version: 1,
      request_id: rid,
      environment: "production",
      repository: "qintopia-agent-studio/qintopia-agent-os",
      commit_sha: o,
      runtime_sha: p,
      release_sha: t,
      deploy_bundle_sha: "4".repeat(40),
      release_scope: ["deploy-bundle"],
      restart_targets: ["qintopia-system-services"],
      dry_run: false,
      created_at: start,
      expires_at: end.toISOString(),
      cos: { request_key: requestKey, result_key: resultKey },
    };
    const signature = {
      algorithm: "hmac-sha256",
      issuer: "github-actions",
      key_id: "simulated",
      signed_at: start,
    };
    return {
      ...unsigned,
      signature: {
        ...signature,
        value: crypto
          .createHmac("sha256", key)
          .update(canonical({ request: unsigned, signature }))
          .digest("hex"),
      },
    };
  }
  const config = path.join(launchRoot, "cos.json");
  const portFile = path.join(launchRoot, "port");
  const serverCode = path.join(launchRoot, "cos.py");
  write(
    serverCode,
    String.raw`import hashlib,hmac,http.server,json,pathlib,sys,urllib.parse
config,port=map(pathlib.Path,sys.argv[1:])
class Handler(http.server.BaseHTTPRequestHandler):
 def do_GET(self):
  c=json.loads(config.read_text()); auth=urllib.parse.parse_qs(self.headers.get('Authorization',''))
  times=auth.get('q-key-time',[''])[0]
  headers='host='+urllib.parse.quote(self.headers.get('Host',''),safe='~-._')
  string='get\n'+self.path+'\n\n'+headers+'\n'
  sign='sha1\n'+times+'\n'+hashlib.sha1(string.encode()).hexdigest()+'\n'
  secret=hmac.new(b'simulated',times.encode(),hashlib.sha1).hexdigest()
  expected=hmac.new(secret.encode(),sign.encode(),hashlib.sha1).hexdigest()
  if auth.get('q-signature') != [expected]: code,body=403,b'bad auth'
  elif self.path == '/'+c['request_key']: code,body=200,json.dumps(c['request']).encode()
  elif c['mode']=='present': code,body=200,json.dumps(c.get('result',{}),separators=(',',':')).encode()
  elif c['mode']=='unreadable': code,body=403,b'AccessDenied'
  elif c['mode']=='other-404': code,body=404,b'<Error><Code>NoSuchBucket</Code></Error>'
  elif c['mode']=='wrong-key': code,body=404,b'<Error><Code>NoSuchKey</Code><Key>wrong</Key></Error>'
  elif c['mode']=='missing-identity': code,body=404,b'<Error><Code>NoSuchKey</Code></Error>'
  elif c['mode']=='wrong-resource': code,body=404,b'<Error><Code>NoSuchKey</Code><Resource>/wrong</Resource></Error>'
  elif c['mode']=='conflicting-identity': code,body=404,('<Error><Code>NoSuchKey</Code><Key>wrong</Key><Resource>/'+c['result_key']+'</Resource></Error>').encode()
  elif c['mode']=='legacy-key': code,body=404,('<Error><Code>NoSuchKey</Code><Key>'+c['result_key']+'</Key></Error>').encode()
  else: code,body=404,('<Error><Code>NoSuchKey</Code><Message>The specified key does not exist.</Message><Resource>/'+c['result_key']+'</Resource><RequestId>simulated</RequestId><TraceId>simulated</TraceId></Error>').encode()
  self.send_response(code); self.end_headers(); self.wfile.write(body)
 def log_message(self,*args): pass
server=http.server.HTTPServer(('127.0.0.1',0),Handler)
port.write_text(str(server.server_port));server.serve_forever()
`
  );
  const server = spawn("python3", [serverCode, config, portFile], {
    stdio: ["ignore", "ignore", "pipe"],
  });
  try {
    for (let i = 0; i < 100 && !fs.existsSync(portFile); i++) {
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
    assert.ok(fs.existsSync(portFile), "COS simulation must start");
    const port = fs.readFileSync(portFile, "utf8");
    write(
      privateEnv,
      `TENCENT_COS_BUCKET=simulated\nTENCENT_COS_REGION=simulated\nTENCENT_COS_SECRET_ID=simulated\nTENCENT_COS_SECRET_KEY=simulated\nTENCENT_COS_ENDPOINT=http://127.0.0.1:${port}\nDEPLOY_REQUEST_SIGNING_KEY=${key}\nDEPLOY_REQUEST_SIGNING_KEY_ID=simulated\n`.replaceAll(
        "\\n",
        "\n"
      )
    );
    write(
      path.join(fixtureBin, "id"),
      "#!/bin/sh\necho 0\n".replaceAll("\\n", "\n"),
      0o755
    );
    write(
      path.join(fixtureBin, "flock"),
      `#!/usr/bin/env python3
import fcntl,sys
try: fcntl.flock(int(sys.argv[-1]),fcntl.LOCK_EX|fcntl.LOCK_NB)
except BlockingIOError: sys.exit(1)
`,
      0o755
    );
    write(
      path.join(fixtureBin, "systemctl"),
      `#!/bin/sh
case "$*" in
 *qintopia-agentos-management-ui.service*)
  echo LoadState=not-found
  echo ActiveState=inactive
  echo SubState=dead
  echo UnitFileState=disabled
  echo InvocationID=
  echo Result=success
  echo ExecMainCode=0
  echo ExecMainStatus=0
  echo MainPID=0
  echo ControlPID=\${FIXTURE_UI_CONTROL_PID:-0}
  echo ControlGroup=\${FIXTURE_UI_CGROUP:-}
  echo NRestarts=0
  ;;
 *LoadState*--value*) echo loaded ;;
 *UnitFileState*--value*) echo disabled ;;
 *InvocationID*--value*) echo aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa ;;
 *ActiveState*--value*) echo inactive ;;
 disable*|daemon-reload) exit 0 ;;
 show*)
  echo LoadState=loaded
  echo ActiveState=inactive
  echo MainPID=\${FIXTURE_MAIN_PID:-0}
  echo ControlPID=0
  echo ControlGroup=\${FIXTURE_CGROUP:-}
  ;;
 *) exit 90 ;;
esac
`,
      0o755
    );
    write(
      path.join(fixtureBin, "systemd-run"),
      "#!/bin/sh\nexit 75\n".replaceAll("\\n", "\n"),
      0o755
    );
    for (const directory of [
      proc,
      cgroups,
      path.join(releases, o),
      path.join(releases, p),
    ])
      fs.mkdirSync(directory, { recursive: true });
    // Preserve the real old-runner digest for the consume identity check; never execute it.
    const oldRunner = execFileSync(
      "git",
      ["show", `${o}:deploy/runner/qintopia-agent-os-deploy-runner`],
      { cwd: repoRoot }
    );
    write(
      path.join(releases, o, "deploy/runner/qintopia-agent-os-deploy-runner"),
      oldRunner,
      0o755
    );
    fs.symlinkSync(path.join(releases, o), path.join(releases, "current"));
    fs.symlinkSync(path.join(releases, p), path.join(releases, "previous"));
    const staged = path.join(recovery, "staged");
    let source = fs
      .readFileSync(
        path.join(repoRoot, "deploy/runner/run-fixed-takeover-request.sh"),
        "utf8"
      )
      .replaceAll("/var/lib/qintopia-agent-os-deploy", state)
      .replaceAll("/home/ubuntu/qintopia-agent-os-releases", releases)
      .replaceAll("/etc/systemd/system", units)
      .replaceAll("/etc/qintopia/cos-artifacts.env", privateEnv)
      .replaceAll('Path("/proc")', `Path(${JSON.stringify(proc)})`)
      .replaceAll("/sys/fs/cgroup", cgroups)
      .replaceAll("metadata.st_uid != 0", "metadata.st_uid != os.geteuid()")
      .replaceAll("info.st_uid != 0", "info.st_uid != os.geteuid()")
      .replaceAll("(path.parent, *path.parent.parents)", "(path.parent,)")
      .replaceAll("(root, *root.parents)", "(root,)");
    const launcher = path.join(
      staged,
      "payload/deploy/runner/run-fixed-takeover-request.sh"
    );
    const files = [
      "qintopia-agent-os-deploy-runner",
      "quiesce-space-automation-runtime.sh",
      "wait-deploy-result.sh",
      "poll-deploy-requests.sh",
      "recover-release-lineage.sh",
      "qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf",
    ];
    const manifest = {
      schema_version: 1,
      commit_sha: "4".repeat(40),
      target: "server-operator-files",
      files: [],
    };
    for (const file of files) {
      const relative = `payload/deploy/runner/${file}`;
      write(path.join(staged, relative), "simulated immutable bytes\n", 0o755);
      const bytes = fs.readFileSync(path.join(staged, relative));
      manifest.files.push({
        path: relative,
        sha256: digest(bytes),
        size_bytes: bytes.length,
      });
    }
    write(launcher, source, 0o755);
    manifest.files.push({
      path: "payload/deploy/runner/run-fixed-takeover-request.sh",
      sha256: digest(fs.readFileSync(launcher)),
      size_bytes: fs.statSync(launcher).size,
    });
    write(path.join(staged, "artifact-manifest.json"), JSON.stringify(manifest));
    write(
      path.join(staged, "SHA256SUMS"),
      `${digest(fs.readFileSync(path.join(staged, "artifact-manifest.json")))}  artifact-manifest.json\n`
    );
    write(
      path.join(
        units,
        "qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf"
      ),
      "simulated immutable bytes\n"
    );
    const env = {
      ...process.env,
      PATH: `${fixtureBin}:${process.env.PATH}`,
      // Host systemd identity must not leak into this simulated consumer.
      INVOCATION_ID: token,
    };
    const reset = (legacy = false) => {
      write(
        path.join(recovery, "takeover.json"),
        JSON.stringify({
          request_id: rid,
          hold_token: token,
          timer_was_enabled: true,
          ...(legacy ? {} : { phase: "preparing" }),
        })
      );
      write(path.join(recovery, "hold"), token + "\n");
      fs.rmSync(path.join(recovery, "takeover-consumed"), { force: true });
      fs.rmSync(path.join(recovery, `retired-${rid}.consumed`), { force: true });
      if (legacy) write(path.join(recovery, "takeover-consumed"), rid + "\n");
      write(
        config,
        JSON.stringify({
          mode: "absent",
          request_key: requestKey,
          result_key: resultKey,
          request: signedRequest(),
        })
      );
    };
    const invoke = (mode, id = rid, extra = {}) =>
      spawnSync("bash", [launcher, mode, id], {
        env: { ...env, ...extra },
        encoding: "utf8",
      });
    const rejected = (label, change, undo = () => {}, extra = {}) => {
      reset();
      change();
      const before = fs.readFileSync(path.join(recovery, "takeover.json"), "utf8");
      const response = invoke("retire-unstarted", rid, extra);
      assert.equal(
        response.status,
        75,
        `${label}: ${response.stdout} ${response.stderr}`
      );
      assert.equal(
        fs.readFileSync(path.join(recovery, "takeover.json"), "utf8"),
        before,
        label
      );
      assert.equal(fs.readFileSync(path.join(recovery, "hold"), "utf8"), token + "\n");
      undo();
    };
    for (const mode of [
      "present",
      "unreadable",
      "other-404",
      "wrong-key",
      "wrong-resource",
      "missing-identity",
      "conflicting-identity",
    ])
      rejected(mode, () => {
        const value = JSON.parse(fs.readFileSync(config));
        value.mode = mode;
        write(config, JSON.stringify(value));
      });
    rejected("unexpired", () => {
      const value = JSON.parse(fs.readFileSync(config));
      value.request = signedRequest(false);
      write(config, JSON.stringify(value));
    });
    rejected("bad-signature", () => {
      const value = JSON.parse(fs.readFileSync(config));
      value.request.signature.value = "0".repeat(64);
      write(config, JSON.stringify(value));
    });
    for (const file of [
      `requests/claimed/${rid}.json`,
      `recovery/${rid}.json`,
      `results/${rid}.json`,
    ]) {
      rejected(
        file,
        () => write(path.join(state, file), "{}"),
        () => fs.rmSync(path.join(state, file))
      );
    }
    rejected("new-format-marker", () =>
      write(path.join(recovery, "takeover-consumed"), rid + "\n")
    );
    rejected(
      "detached-consumer",
      () => write(path.join(proc, "999999/cmdline"), "bash\0poll-deploy-requests.sh\0"),
      () => fs.rmSync(path.join(proc, "999999"), { recursive: true })
    );
    rejected(
      "unit-pid",
      () => {},
      () => {},
      { FIXTURE_MAIN_PID: "12" }
    );
    rejected(
      "populated-cgroup",
      () => write(path.join(cgroups, "fixed/cgroup.events"), "populated 1\n"),
      () => {},
      { FIXTURE_CGROUP: "/fixed" }
    );
    rejected(
      "pointer-drift",
      () => {
        fs.unlinkSync(path.join(releases, "current"));
        fs.symlinkSync(path.join(releases, p), path.join(releases, "current"));
      },
      () => {
        fs.unlinkSync(path.join(releases, "current"));
        fs.symlinkSync(path.join(releases, o), path.join(releases, "current"));
      }
    );
    for (const lockName of ["recovery/takeover.lock", "poller.lock", "deploy.lock"]) {
      const ready = path.join(launchRoot, "lock-ready");
      fs.rmSync(ready, { force: true });
      const holder = spawn(
        "python3",
        [
          "-c",
          "import fcntl,pathlib,sys,time; f=open(sys.argv[1],'a'); fcntl.flock(f,fcntl.LOCK_EX); pathlib.Path(sys.argv[2]).touch(); time.sleep(30)",
          path.join(state, lockName),
          ready,
        ],
        { stdio: "ignore" }
      );
      try {
        for (let i = 0; i < 100 && !fs.existsSync(ready); i++)
          await new Promise((resolve) => setTimeout(resolve, 20));
        assert.ok(fs.existsSync(ready), "lock holder ready");
        rejected(`busy-${lockName}`, () => {});
      } finally {
        holder.kill();
        await new Promise((resolve) => holder.once("exit", resolve));
      }
    }
    // Execute the recovery helper's actual COS reader, not a duplicate validator.
    const recoverySource = fs.readFileSync(
      path.join(repoRoot, "deploy/runner/recover-release-lineage.sh"),
      "utf8"
    );
    const reader = recoverySource
      .split(
        `if ! python3 - "$request_id" "$remote_result" "$remote_state_file" <<'PY'\n`
      )[1]
      ?.split("\nPY\nthen")[0];
    assert.ok(reader, "recovery COS reader must be found");
    for (const mode of [
      "absent",
      "legacy-key",
      "present",
      "unreadable",
      "other-404",
      "wrong-key",
      "wrong-resource",
      "missing-identity",
      "conflicting-identity",
    ]) {
      reset();
      const value = JSON.parse(fs.readFileSync(config));
      value.mode = mode;
      write(config, JSON.stringify(value));
      const stateFile = path.join(launchRoot, "readback.state");
      fs.rmSync(stateFile, { force: true });
      const response = spawnSync(
        "python3",
        ["-c", reader, rid, path.join(launchRoot, "readback.json"), stateFile],
        {
          encoding: "utf8",
          env: {
            ...env,
            TENCENT_COS_BUCKET: "simulated",
            TENCENT_COS_REGION: "simulated",
            TENCENT_COS_SECRET_ID: "simulated",
            TENCENT_COS_SECRET_KEY: "simulated",
            TENCENT_COS_AUTH_MODE: "SecretKey",
            TENCENT_COS_SESSION_TOKEN: "",
            TENCENT_COS_ENDPOINT: `http://127.0.0.1:${port}`,
          },
        }
      );
      const accepted = ["absent", "legacy-key", "present"].includes(mode);
      assert.equal(response.status === 0, accepted, `${mode}: ${response.stderr}`);
      if (accepted)
        assert.equal(
          fs.readFileSync(stateFile, "utf8"),
          mode === "present" ? "present\n" : "absent\n"
        );
      else assert.equal(fs.existsSync(stateFile), false);
    }
    for (const legacy of [false, true]) {
      reset(legacy);
      if (legacy) {
        const value = JSON.parse(fs.readFileSync(config));
        value.mode = "legacy-key";
        write(config, JSON.stringify(value));
      }
      const result = invoke("retire-unstarted");
      assert.equal(result.status, 0, result.stderr);
      assert.equal(fs.existsSync(path.join(recovery, "takeover-consumed")), false);
      assert.equal(
        fs.existsSync(path.join(recovery, `retired-${rid}.consumed`)),
        legacy
      );
      const retired = JSON.parse(fs.readFileSync(path.join(recovery, "takeover.json")));
      assert.equal(retired.phase, "retired");
      assert.match(retired.retired_requests[rid].request_sha256, /^[0-9a-f]{64}$/);
      assert.equal(fs.readFileSync(path.join(recovery, "hold"), "utf8"), token + "\n");
      assert.equal(invoke("retire-unstarted").status, 0, "idempotent retirement");
      if (legacy) {
        // Interrupted after the durable retirement record, before marker rename.
        fs.renameSync(
          path.join(recovery, `retired-${rid}.consumed`),
          path.join(recovery, "takeover-consumed")
        );
        assert.equal(
          invoke("retire-unstarted").status,
          0,
          "resume marker archival without replay"
        );
      }
      assert.notEqual(invoke("consume").status, 0, "old ID cannot be rebound");
      const next = invoke("consume", nextId);
      assert.equal(next.status, 75, next.stderr); // simulated systemd failure before claim
      assert.equal(
        JSON.parse(fs.readFileSync(path.join(recovery, "takeover.json"))).request_id,
        nextId
      );
      assert.equal(fs.existsSync(path.join(recovery, "takeover-consumed")), false);
    }
    reset();
    write(path.join(state, `requests/pending/${rid}.json`), '{"interrupted":', 0o644);
    assert.equal(
      invoke("retire-unstarted").status,
      0,
      "partial download can be retired before claim"
    );
    assert.equal(
      fs.readFileSync(path.join(state, `requests/pending/${rid}.json`), "utf8"),
      '{"interrupted":'
    );
    assert.match(
      JSON.parse(fs.readFileSync(path.join(recovery, "takeover.json")))
        .retired_requests[rid].pending_sha256,
      /^[0-9a-f]{64}$/
    );

    // Exercise the complete staged recovery script, including real request/result
    // HMAC verification. Only OS paths, ownership and remote transports are mocked.
    const helper = path.join(
      staged,
      "payload/deploy/runner/recover-release-lineage.sh"
    );
    const remap = (text) =>
      text
        .replaceAll("/var/lib/qintopia-agent-os-deploy", state)
        .replaceAll("/home/ubuntu/qintopia-agent-os-releases", releases)
        .replaceAll("/etc/systemd/system", units)
        .replaceAll("/etc/qintopia/cos-artifacts.env", privateEnv)
        .replaceAll("/sys/fs/cgroup", cgroups)
        .replaceAll("metadata.st_uid != 0", "metadata.st_uid != os.geteuid()")
        .replaceAll("info.st_uid != 0", "info.st_uid != os.geteuid()")
        .replaceAll("path.st_uid != 0", "path.st_uid != os.geteuid()")
        .replaceAll("/usr/bin/systemctl", path.join(fixtureBin, "systemctl"))
        .replaceAll(
          "/usr/bin:/bin:/usr/sbin:/sbin",
          `${fixtureBin}:${process.env.PATH}`
        )
        .replaceAll('Path("/proc")', `Path(${JSON.stringify(proc)})`)
        .replaceAll("/etc/nginx", path.join(launchRoot, "nginx"))
        .replaceAll("i<120", "i<1");
    write(helper, remap(recoverySource), 0o755);
    write(
      path.join(staged, "payload/deploy/runner/wait-deploy-result.sh"),
      fs.readFileSync(path.join(repoRoot, "deploy/runner/wait-deploy-result.sh")),
      0o755
    );
    const uiHelper = path.join(
      staged,
      "payload/deploy/runner/management-ui-lifecycle.sh"
    );
    write(
      uiHelper,
      remap(
        fs.readFileSync(
          path.join(repoRoot, "deploy/runner/management-ui-lifecycle.sh"),
          "utf8"
        )
      ),
      0o755
    );
    for (const relative of [
      "deploy/runner/management-ui-lifecycle.sh",
      "runtime/nginx/templates/management-ui-http.conf.template",
      "runtime/nginx/templates/management-ui-https.conf.template",
    ]) {
      if (!relative.endsWith(".sh"))
        write(
          path.join(staged, "payload", relative),
          fs.readFileSync(path.join(repoRoot, relative)),
          0o644
        );
      manifest.files.push({ path: "payload/" + relative });
    }
    const refreshBundle = () => {
      for (const item of manifest.files) {
        const bytes = fs.readFileSync(path.join(staged, item.path));
        item.sha256 = digest(bytes);
        item.size_bytes = bytes.length;
      }
      write(path.join(staged, "artifact-manifest.json"), JSON.stringify(manifest));
      write(
        path.join(staged, "SHA256SUMS"),
        `${digest(fs.readFileSync(path.join(staged, "artifact-manifest.json")))}  artifact-manifest.json\n`
      );
    };
    refreshBundle();
    write(
      path.join(fixtureBin, "coscli"),
      `#!/usr/bin/env python3
import json,pathlib,sys
args=sys.argv[1:]
if '--log-path' not in args: sys.exit(73)
if args[0]=='config': sys.exit(0)
c=json.loads(pathlib.Path(${JSON.stringify(config)}).read_text())
if args[0]!='cp' or not args[1].endswith(c['request_key']): sys.exit(74)
pathlib.Path(args[2]).write_text(json.dumps(c['request'],separators=(',',':')))
`,
      0o755
    );
    const resultPath = path.join(state, `results/${rid}.json`);
    const requestPath = path.join(state, `requests/failed/${rid}.json`);
    const journalPath = path.join(recovery, `${rid}.json`);
    const closeMarker = path.join(recovery, `closed-${rid}.consumed`);
    const signObject = (unsigned, kind, issuer, signedAt = "") => {
      const signature = {
        algorithm: "hmac-sha256",
        issuer,
        key_id: "simulated",
        signed_at:
          signedAt || (kind === "result" ? unsigned.finished_at : unsigned.created_at),
      };
      return {
        ...unsigned,
        signature: {
          ...signature,
          value: crypto
            .createHmac("sha256", key)
            .update(canonical({ [kind]: unsigned, signature }))
            .digest("hex"),
        },
      };
    };
    const resetClosure = () => {
      reset();
      for (const name of ["pending", "processed", "failed", "claimed"]) {
        fs.rmSync(path.join(state, "requests", name), { recursive: true, force: true });
        fs.mkdirSync(path.join(state, "requests", name), { recursive: true });
      }
      for (const name of fs.readdirSync(recovery)) {
        if (name.startsWith("deploy-") || name.startsWith("closed-"))
          fs.rmSync(path.join(recovery, name), { force: true });
      }
      fs.rmSync(path.join(releases, t), { recursive: true, force: true });
      for (const [name, sha] of [
        ["current", o],
        ["previous", p],
      ]) {
        fs.unlinkSync(path.join(releases, name));
        fs.symlinkSync(path.join(releases, sha), path.join(releases, name));
        write(
          path.join(releases, sha, "manifest.json"),
          JSON.stringify({ release_sha: sha }),
          0o444
        );
      }
      const { signature: _ignored, ...unsigned } = signedRequest();
      unsigned.runtime_artifact_profile = "huabaosi-production";
      unsigned.cos = {
        ...unsigned.cos,
        bucket: "simulated",
        region: "simulated",
        prefix: "qintopia-agent-os",
      };
      const request = signObject(unsigned, "request", "github-actions");
      write(requestPath, JSON.stringify(request));
      const result = signObject(
        {
          schema_version: 1,
          environment: "production",
          request_id: rid,
          release_sha: t,
          commit_sha: o,
          runtime_sha: p,
          deploy_bundle_sha: unsigned.deploy_bundle_sha,
          runtime_artifact_profile: unsigned.runtime_artifact_profile,
          release_scope: unsigned.release_scope,
          restart_targets: unsigned.restart_targets,
          status: "failed",
          finished_at: new Date().toISOString(),
          rollback: { attempted: false, status: "not_needed" },
          checks: [
            {
              name: "deploy-runner",
              status: "failed",
              detail: JSON.stringify({
                failure_stage: "quiesce-space-automation-runtime",
                exit_status: 1,
                promoted_current: false,
                profile_activation_attempted: false,
              }),
            },
          ],
        },
        "result",
        "qintopia-deploy-runner"
      );
      write(resultPath, JSON.stringify(result));
      write(
        journalPath,
        JSON.stringify({
          schema_version: 1,
          request_id: rid,
          request_sha256: digest(fs.readFileSync(requestPath)),
          direction: "O→T",
          phase: "intent",
          original_current_sha: o,
          original_previous_sha: p,
          hold_token: token,
          execution: {
            unit: "qintopia-agent-os-fixed-takeover.service",
            invocation_id: token,
            unit_invocation_verified: true,
          },
          result_upload: {
            phase: "upload_intent",
            payload_sha256: digest(fs.readFileSync(resultPath)),
          },
          manifest_sha256: {
            current: digest(fs.readFileSync(path.join(releases, o, "manifest.json"))),
            previous: digest(fs.readFileSync(path.join(releases, p, "manifest.json"))),
          },
        })
      );
      write(path.join(recovery, "takeover-consumed"), rid + "\n");
      write(
        config,
        JSON.stringify({
          mode: "present",
          request_key: requestKey,
          result_key: resultKey,
          request,
          result,
        })
      );
    };
    const close = (extra = {}) =>
      spawnSync("bash", [helper, "--request-id", rid], {
        encoding: "utf8",
        env: { ...env, ...extra },
      });
    const changeJSON = (file, change) => {
      const data = JSON.parse(fs.readFileSync(file));
      change(data);
      write(file, JSON.stringify(data));
    };
    const replaceSignedResult = (change) => {
      const unsigned = JSON.parse(fs.readFileSync(resultPath));
      delete unsigned.signature;
      change(unsigned);
      const signed = signObject(unsigned, "result", "qintopia-deploy-runner");
      write(resultPath, JSON.stringify(signed));
      changeJSON(config, (c) => {
        c.result = signed;
      });
      changeJSON(journalPath, (j) => {
        j.result_upload.payload_sha256 = digest(fs.readFileSync(resultPath));
      });
    };
    resetClosure();
    const binding = fs
      .readFileSync(
        path.join(repoRoot, "deploy/runner/qintopia-agent-os-deploy-runner"),
        "utf8"
      )
      .split(
        "# The fixed poller owns FD7 across execution/upload/archive. Give all helpers\n"
      )[1]
      ?.split("\n(\n  flock -n 9")[0];
    const helperLock = remap(
      fs.readFileSync(
        path.join(repoRoot, "deploy/runner/management-ui-lifecycle.sh"),
        "utf8"
      )
    ).match(/^check_inherited_lock\(\) \{[\s\S]*?^\}/m)?.[0];
    assert.ok(binding && helperLock);
    const lockFile = path.join(state, "deploy.lock");
    const bindAndCheck = (setup) =>
      spawnSync(
        "bash",
        [
          "-c",
          `set -e\nLOCK_FILE="${lockFile}"\nstate="${state}"\n${setup}\n${binding}\n${helperLock}\ncheck_inherited_lock`,
        ],
        { encoding: "utf8", env: { ...env, QINTOPIA_FIXED_TAKEOVER_LOCK: "1" } }
      );
    assert.notEqual(bindAndCheck("").status, 0, "missing FD7 must refuse");
    assert.notEqual(
      bindAndCheck(`exec 7>"${path.join(launchRoot, "other.lock")}"; flock -n 7`)
        .status,
      0,
      "wrong FD7 must refuse"
    );
    assert.notEqual(
      bindAndCheck(`exec 7>"${lockFile}"`).status,
      0,
      "unheld FD7 must refuse"
    );
    const inherited = bindAndCheck(`exec 7>"${lockFile}"; flock -n 7`);
    assert.equal(inherited.status, 0, inherited.stderr);
    const closedUi = (mode, lock = true) =>
      spawnSync(
        "bash",
        [
          "-c",
          `${lock ? `exec 9>"${path.join(state, "deploy.lock")}"; flock -n 9;` : ""} exec bash "$1" "$2"`,
          "fixture",
          uiHelper,
          mode,
        ],
        { encoding: "utf8", env }
      );
    assert.equal(
      closedUi("verify-closed").status,
      0,
      "real staged helper must verify a definite failed request"
    );
    const offsetRequest = JSON.parse(fs.readFileSync(requestPath));
    delete offsetRequest.signature;
    write(
      requestPath,
      JSON.stringify(
        signObject(
          offsetRequest,
          "request",
          "github-actions",
          new Date(Date.parse(offsetRequest.created_at) + 60000).toISOString()
        )
      )
    );
    changeJSON(journalPath, (j) => {
      j.request_sha256 = digest(fs.readFileSync(requestPath));
    });
    assert.equal(
      closedUi("verify-closed").status,
      0,
      "existing signing clock tolerance must remain valid"
    );
    changeJSON(requestPath, (r) => {
      r.signature.value = "0".repeat(64);
    });
    changeJSON(journalPath, (j) => {
      j.request_sha256 = digest(fs.readFileSync(requestPath));
    });
    assert.notEqual(
      closedUi("verify-closed").status,
      0,
      "staged helper must independently verify request signature"
    );
    resetClosure();
    assert.notEqual(
      closedUi("verify-closed", false).status,
      0,
      "helper must require inherited lock ownership"
    );
    for (const mode of [
      "quiesce",
      "prepare",
      "activate",
      "install-http",
      "issue-cert",
      "install-https",
    ])
      assert.notEqual(
        closedUi(mode).status,
        0,
        "archived failure cannot authorize " + mode
      );
    const completeFiles = [...manifest.files];
    manifest.files = manifest.files.filter(
      (f) => !f.path.endsWith("management-ui-lifecycle.sh")
    );
    refreshBundle();
    assert.notEqual(
      closedUi("verify-closed").status,
      0,
      "unlisted helper is not trusted by a valid manifest"
    );
    manifest.files = completeFiles;
    refreshBundle();
    for (const [label, change, extra] of [
      [
        "remote absent",
        () =>
          changeJSON(config, (c) => {
            c.mode = "absent";
          }),
      ],
      [
        "remote unreadable",
        () =>
          changeJSON(config, (c) => {
            c.mode = "unreadable";
          }),
      ],
      [
        "different remote bytes",
        () =>
          changeJSON(config, (c) => {
            c.result.status = "succeeded";
          }),
      ],
      [
        "claim remains",
        () => write(path.join(state, `requests/claimed/${rid}.json`), "{}"),
      ],
      ["T exists", () => fs.mkdirSync(path.join(releases, t))],
      [
        "manifest drift",
        () => write(path.join(releases, o, "manifest.json"), "{}", 0o444),
      ],
      ["later journal", () => write(path.join(recovery, `${nextId}.json`), "{}")],
      ["hold changed", () => write(path.join(recovery, "hold"), "b".repeat(32) + "\n")],
      [
        "upload digest changed",
        () =>
          changeJSON(journalPath, (j) => {
            j.result_upload.payload_sha256 = "0".repeat(64);
          }),
      ],
      [
        "missing invocation",
        () =>
          changeJSON(journalPath, (j) => {
            delete j.execution.invocation_id;
          }),
      ],
      ["live consumer", () => {}, { FIXTURE_MAIN_PID: "42" }],
      ["UI residual process", () => {}, { FIXTURE_UI_CONTROL_PID: "42" }],
      ["UI unknown cgroup", () => {}, { FIXTURE_UI_CGROUP: "/missing" }],
      [
        "signed success",
        () =>
          replaceSignedResult((r) => {
            r.status = "succeeded";
          }),
      ],
      [
        "signed later failure",
        () =>
          replaceSignedResult((r) => {
            const detail = JSON.parse(r.checks[0].detail);
            detail.failure_stage = "install-release-systemd-units";
            r.checks[0].detail = JSON.stringify(detail);
          }),
      ],
      [
        "signed promoted failure",
        () =>
          replaceSignedResult((r) => {
            const detail = JSON.parse(r.checks[0].detail);
            detail.promoted_current = true;
            r.checks[0].detail = JSON.stringify(detail);
          }),
      ],
    ]) {
      resetClosure();
      change();
      const before = fs.readFileSync(path.join(recovery, "takeover.json"));
      const response = close(extra);
      assert.notEqual(response.status, 0, `${label} must refuse`);
      assert.deepEqual(
        fs.readFileSync(path.join(recovery, "takeover.json")),
        before,
        label
      );
      assert.ok(fs.existsSync(path.join(recovery, "takeover-consumed")), label);
    }
    resetClosure();
    replaceSignedResult((r) => {
      const detail = JSON.parse(r.checks[0].detail);
      detail.failure_stage = "quiesce-management-ui";
      r.checks[0].detail = JSON.stringify(detail);
    });
    const closed = close();
    assert.equal(closed.status, 0, closed.stderr);
    assert.ok(fs.existsSync(closeMarker));
    assert.equal(fs.readFileSync(path.join(recovery, "hold"), "utf8"), token + "\n");
    assert.equal(close().status, 0, "repeat closure must not replay");
    fs.renameSync(closeMarker, path.join(recovery, "takeover-consumed"));
    assert.equal(close().status, 0, "resume interrupted marker archive");
    assert.notEqual(invoke("consume").status, 0, "closed request cannot be replayed");
    const fresh = invoke("consume", nextId);
    assert.equal(fresh.status, 75, fresh.stderr);
    assert.equal(
      JSON.parse(fs.readFileSync(path.join(recovery, "takeover.json"))).request_id,
      nextId
    );
    assert.notEqual(close().status, 0, "old closure cannot alter a new binding");

    // Launcher -> poller -> current real runner -> signed result -> finalize.
    // Promotion/install/smoke are OS boundaries; no production artifact is built here.
    resetClosure();
    assert.equal(close().status, 0);
    changeJSON(path.join(recovery, "takeover.json"), (r) => {
      r.timer_was_enabled = false;
    });
    write(
      path.join(releases, o, "manifest.json"),
      JSON.stringify({ release_sha: o, previous_sha: p }),
      0o444
    );
    const runnerDir = path.join(staged, "payload/deploy/runner");
    for (const name of ["qintopia-agent-os-deploy-runner", "poll-deploy-requests.sh"])
      write(
        path.join(runnerDir, name),
        remap(fs.readFileSync(path.join(repoRoot, "deploy/runner", name), "utf8")),
        0o755
      );
    write(
      path.join(fixtureBin, "stat"),
      `#!/usr/bin/env python3
import os,sys
p=sys.argv[-1]
s=os.fstat(int(p.rsplit('/',1)[1])) if p.startswith('/proc/self/fd/') else os.stat(p)
print(str(s.st_dev)+':'+str(s.st_ino))
`,
      0o755
    );
    const executionLog = path.join(launchRoot, "execution.log");
    write(
      path.join(runnerDir, "quiesce-space-automation-runtime.sh"),
      `#!/bin/sh\necho new-quiesce >> '${executionLog}'\n`,
      0o755
    );
    write(
      path.join(runnerDir, "promote-release.sh"),
      `#!/usr/bin/env python3
import json,os,pathlib,sys
args=sys.argv[1:];request=json.loads(pathlib.Path(args[args.index('--request-file')+1]).read_text())
root=pathlib.Path(args[args.index('--release-root')+1]);target=root/request['release_sha']
assert not target.exists()
old=(root/'current').resolve(); target.mkdir()
manifest={**request,'previous_sha':old.name}
(target/'manifest.json').write_text(json.dumps(manifest))
runner=target/'deploy/runner';runner.mkdir(parents=True)
for name in ('install-release-systemd-units.sh','smoke-release.sh'):
 p=runner/name;p.write_text('#!/bin/sh'+chr(10)+'exit 0'+chr(10));p.chmod(0o755)
(root/'previous').unlink();(root/'previous').symlink_to(old)
(root/'current').unlink();(root/'current').symlink_to(target)
`,
      0o755
    );
    for (const name of ["promote-release.sh"])
      manifest.files.push({ path: `payload/deploy/runner/${name}` });
    refreshBundle();
    const freshUnsigned = { ...JSON.parse(fs.readFileSync(requestPath)) };
    delete freshUnsigned.signature;
    freshUnsigned.request_id = nextId;
    freshUnsigned.created_at = new Date().toISOString();
    freshUnsigned.expires_at = new Date(Date.now() + 1800000).toISOString();
    freshUnsigned.requested_by = "fixture";
    freshUnsigned.rollback_on_smoke_failure = true;
    freshUnsigned.cos = {
      ...freshUnsigned.cos,
      request_key: `qintopia-agent-os/deploy-requests/production/requests/${nextId}.json`,
      result_key: `qintopia-agent-os/deploy-results/production/${nextId}.json`,
    };
    const freshRequest = signObject(freshUnsigned, "request", "github-actions");
    write(config, JSON.stringify({ request: freshRequest }));
    const remoteBytes = path.join(launchRoot, "uploaded.json");
    write(
      path.join(fixtureBin, "coscli"),
      `#!/usr/bin/env python3
import json,pathlib,sys
args=sys.argv[1:]
if '--log-path' not in args: sys.exit(73)
if args[0]=='config': sys.exit(0)
r=json.loads(pathlib.Path(${JSON.stringify(config)}).read_text())['request']
source,target=args[1:3];remote=pathlib.Path(${JSON.stringify(remoteBytes)})
if not source.startswith('cos://'): remote.write_bytes(pathlib.Path(source).read_bytes());sys.exit(0)
if source.endswith('/current.json'):
 data={'schema_version':1,'environment':'production','repository':r['repository'],'request_id':r['request_id'],
       'request_key':r['cos']['request_key'],'result_key':r['cos']['result_key']}
elif source.endswith(r['cos']['request_key']): data=r
elif source.endswith(r['cos']['result_key']):
 if not remote.exists(): print('NoSuchKey',file=sys.stderr);sys.exit(1)
 pathlib.Path(target).write_bytes(remote.read_bytes());sys.exit(0)
else: sys.exit(64)
pathlib.Path(target).write_text(json.dumps(data,separators=(',',':')))
`,
      0o755
    );
    const consumerLog = path.join(launchRoot, "consumer.log");
    write(
      path.join(fixtureBin, "systemd-run"),
      `#!/usr/bin/env python3
import os,sys
for arg in sys.argv[1:]:
 if arg.startswith('--property=Environment='):
  k,v=arg.split('Environment=',1)[1].split('=',1);os.environ[k]=v
os.environ['INVOCATION_ID']='${token}'
fd=os.open(${JSON.stringify(consumerLog)},os.O_WRONLY|os.O_CREAT|os.O_TRUNC,0o600)
os.dup2(fd,1);os.dup2(fd,2)
os.execv('/bin/bash',['bash',sys.argv[-1]])
`,
      0o755
    );
    const taken = invoke("consume", nextId);
    assert.equal(
      taken.status,
      0,
      `${taken.stderr}\n${fs.readFileSync(consumerLog, "utf8")}`
    );
    assert.equal(fs.readFileSync(executionLog, "utf8"), "new-quiesce\n");
    assert.equal(
      fs.realpathSync(path.join(releases, "current")),
      path.join(releases, t)
    );
    assert.equal(
      fs.realpathSync(path.join(releases, "previous")),
      path.join(releases, o)
    );
    assert.equal(fs.existsSync(path.join(recovery, "hold")), false);
    assert.equal(JSON.parse(fs.readFileSync(remoteBytes)).status, "succeeded");
    assert.notEqual(
      invoke("consume", nextId).status,
      0,
      "successful request cannot replay"
    );
    // The existing post-promotion recovery validator must accept the recorded
    // staged execution identity, while rejecting altered executable bytes.
    const claimValidator = recoverySource
      .split(
        '"$direction" "$original_current" "$request_id" "$hold" "${state}/recovery/takeover.json" <<\'PY\'\n'
      )[1]
      ?.split("\nPY\n")[0];
    assert.ok(claimValidator, "recovery claim validator must be found");
    const takeoverJournal = JSON.parse(
      fs.readFileSync(path.join(recovery, `${nextId}.json`))
    );
    const recoveredClaim = {
      ...takeoverJournal,
      phase: "possibly_executing",
      recovery_eligible: true,
    };
    const recoveryClaim = path.join(state, `requests/claimed/${nextId}.json`);
    write(recoveryClaim, JSON.stringify(recoveredClaim));
    write(path.join(recovery, "hold"), token + "\n");
    const validateClaim = () =>
      spawnSync(
        "python3",
        [
          "-c",
          claimValidator.replaceAll(
            "metadata.st_uid != 0",
            "metadata.st_uid != os.geteuid()"
          ),
          path.join(recovery, `${nextId}.json`),
          recoveryClaim,
          path.join(state, `requests/processed/${nextId}.json`),
          releases,
          "O→T",
          o,
          nextId,
          path.join(recovery, "hold"),
          path.join(recovery, "takeover.json"),
        ],
        { encoding: "utf8" }
      );
    const validClaim = validateClaim();
    assert.equal(validClaim.status, 0, validClaim.stderr);
    const runnerFile = path.join(runnerDir, "qintopia-agent-os-deploy-runner");
    const originalRunner = fs.readFileSync(runnerFile);
    fs.appendFileSync(runnerFile, "\n# simulated drift\n");
    assert.notEqual(
      validateClaim().status,
      0,
      "changed execution bytes cannot authorize recovery"
    );
    write(runnerFile, originalRunner, 0o755);
    fs.unlinkSync(recoveryClaim);
    fs.unlinkSync(path.join(recovery, "hold"));
    // Continue with the ordinary full six-target request through T's runner.
    fs.cpSync(runnerDir, path.join(releases, t, "deploy/runner"), { recursive: true });
    fs.cpSync(path.join(staged, "payload/runtime"), path.join(releases, t, "runtime"), {
      recursive: true,
    });
    fs.mkdirSync(path.join(releases, t, "deploy-bundle"), { recursive: true });
    fs.copyFileSync(
      path.join(staged, "artifact-manifest.json"),
      path.join(releases, t, "deploy-bundle/artifact-manifest.json")
    );
    fs.chmodSync(path.join(releases, t, "deploy-bundle/artifact-manifest.json"), 0o444);
    fs.chmodSync(path.join(releases, t, "manifest.json"), 0o444);
    const fullId = "deploy-20261002T113059Z-444444444444";
    const fullUnsigned = {
      ...freshUnsigned,
      request_id: fullId,
      commit_sha: "4".repeat(40),
      runtime_sha: "4".repeat(40),
      release_sha: "4".repeat(40),
      release_scope: ["sidecar-runtime", "deploy-bundle", "hermes-plugins"],
      restart_targets: [
        "qintopia-system-services",
        "hermes-erhua",
        "hermes-xiaoman",
        "hermes-silaoshi",
        "hermes-huabaosi",
        "hermes-anan",
      ],
      cos: {
        ...freshUnsigned.cos,
        request_key: `qintopia-agent-os/deploy-requests/production/requests/${fullId}.json`,
        result_key: `qintopia-agent-os/deploy-results/production/${fullId}.json`,
      },
    };
    write(
      config,
      JSON.stringify({ request: signObject(fullUnsigned, "request", "github-actions") })
    );
    fs.unlinkSync(remoteBytes);
    const wrongInvocation = spawnSync(
      "bash",
      [path.join(releases, t, "deploy/runner/poll-deploy-requests.sh")],
      {
        encoding: "utf8",
        env: {
          ...env,
          INVOCATION_ID: "b".repeat(32),
          QINTOPIA_COS_ENV_FILE: privateEnv,
          QINTOPIA_DEPLOY_RUNNER_STATE_DIR: state,
          QINTOPIA_DEPLOY_RUNNER_BIN: path.join(
            releases,
            t,
            "deploy/runner/qintopia-agent-os-deploy-runner"
          ),
          QINTOPIA_RELEASE_ROOT: releases,
        },
      }
    );
    assert.notEqual(wrongInvocation.status, 0, "unmatched invocation must be rejected");
    assert.match(
      wrongInvocation.stderr,
      /consumer systemd invocation identity is not verified/
    );
    assert.equal(
      fs.existsSync(path.join(state, `requests/claimed/${fullId}.json`)),
      false
    );
    const full = spawnSync(
      "bash",
      [path.join(releases, t, "deploy/runner/poll-deploy-requests.sh")],
      {
        encoding: "utf8",
        env: {
          ...env,
          QINTOPIA_COS_ENV_FILE: privateEnv,
          QINTOPIA_DEPLOY_RUNNER_STATE_DIR: state,
          QINTOPIA_DEPLOY_RUNNER_BIN: path.join(
            releases,
            t,
            "deploy/runner/qintopia-agent-os-deploy-runner"
          ),
          QINTOPIA_RELEASE_ROOT: releases,
        },
      }
    );
    assert.equal(full.status, 0, full.stderr);
    assert.equal(JSON.parse(fs.readFileSync(remoteBytes)).status, "succeeded");
    assert.equal(
      fs.realpathSync(path.join(releases, "current")),
      path.join(releases, "4".repeat(40))
    );
    assert.equal(
      fs.realpathSync(path.join(releases, "previous")),
      path.join(releases, t)
    );
    assert.equal(
      JSON.parse(fs.readFileSync(path.join(recovery, `${fullId}.json`))).phase,
      "smoke-passed"
    );
    console.log(
      "Normal six-target full release after takeover passed (OS boundaries mocked)."
    );
    console.log(
      "Launcher through real staged runner, signed upload and finalize passed (OS boundaries mocked)."
    );
    console.log("Whole staged recovery signature, CAS and refusal matrix passed.");
    console.log(
      "Takeover unstarted retirement and refusal matrix passed (local boundary mocks)."
    );
  } finally {
    server.kill();
    await new Promise((resolve) => server.once("exit", resolve));
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

execFileSync("bash", ["-n", "deploy/runner/poll-deploy-requests.sh"], {
  cwd: repoRoot,
});

console.log("Deploy runner poller behavior test passed.");
