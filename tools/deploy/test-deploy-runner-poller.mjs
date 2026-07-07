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
  processed|failed|remote-result)
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
      echo "NoSuchKey: object not found" >&2
      exit 1
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
echo "runner should not execute for idle poller states" >&2
exit 67
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

const runCase = ({ name, mode, archive }) => {
  const stateDir = path.join(tmpRoot, name);
  fs.mkdirSync(path.join(stateDir, "requests", "pending"), { recursive: true });
  fs.mkdirSync(path.join(stateDir, "requests", "processed"), { recursive: true });
  fs.mkdirSync(path.join(stateDir, "requests", "failed"), { recursive: true });
  fs.mkdirSync(path.join(stateDir, "results"), { recursive: true });
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
    },
    encoding: "utf8",
  });

  if (result.status !== 0) {
    throw new Error(
      `${name}: expected idle success, got ${result.status}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`
    );
  }
  return `${result.stdout}${result.stderr}`;
};

try {
  const missingPointerOutput = runCase({
    name: "missing-pointer",
    mode: "missing-pointer",
  });
  if (!missingPointerOutput.includes("No deploy request pointer found; idle")) {
    throw new Error("missing-pointer: idle message was not emitted");
  }

  const processedOutput = runCase({
    name: "processed-pointer",
    mode: "processed",
    archive: "processed",
  });
  if (!processedOutput.includes("Deploy request already processed; idle")) {
    throw new Error("processed-pointer: processed idle message was not emitted");
  }

  const remoteResultOutput = runCase({
    name: "remote-result-pointer",
    mode: "remote-result",
  });
  if (!remoteResultOutput.includes("Deploy request result already exists; idle")) {
    throw new Error(
      "remote-result-pointer: remote-result idle message was not emitted"
    );
  }

  const failedOutput = runCase({
    name: "failed-pointer",
    mode: "failed",
    archive: "failed",
  });
  if (!failedOutput.includes("Deploy request already failed; idle")) {
    throw new Error("failed-pointer: failed idle message was not emitted");
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

execFileSync("bash", ["-n", "deploy/runner/poll-deploy-requests.sh"], {
  cwd: repoRoot,
});

console.log("Deploy runner poller behavior test passed.");
