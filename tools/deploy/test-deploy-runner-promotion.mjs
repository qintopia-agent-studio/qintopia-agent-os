#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "qintopia-runner-test-"));

const writeExecutable = (relativePath, content) => {
  const filePath = path.join(tmpRoot, relativePath);
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, content, "utf8");
  fs.chmodSync(filePath, 0o755);
  return filePath;
};

const signingKey = "test-signing-key";
const keyId = "production";
const requestId = "deploy-20260706T000000Z-0123456789ab";
const sha = "0123456789abcdef0123456789abcdef01234567";
const previousSha = "abcdef0123456789abcdef0123456789abcdef01";

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

const signRequest = (request, metadata) =>
  crypto
    .createHmac("sha256", signingKey)
    .update(canonicalJson({ request, signature: metadata }))
    .digest("hex");

try {
  const stateDir = path.join(tmpRoot, "state");
  const releaseRoot = path.join(tmpRoot, "releases");
  fs.mkdirSync(stateDir, { recursive: true });
  fs.mkdirSync(releaseRoot, { recursive: true });

  const request = {
    schema_version: 1,
    request_id: requestId,
    environment: "production",
    repository: "qintopia-agent-studio/qintopia-agent-os",
    requested_by: "codex",
    created_at: "2026-07-06T00:00:00Z",
    expires_at: "2099-07-06T01:00:00Z",
    commit_sha: sha,
    runtime_sha: sha,
    deploy_bundle_sha: sha,
    release_sha: sha,
    release_scope: ["sidecar-runtime", "deploy-bundle", "hermes-plugins"],
    restart_targets: ["qintopia-system-services"],
    rollback_on_smoke_failure: true,
    dry_run: false,
    cos: {
      bucket: "qintopia-agent-os-artifacts-1305166808",
      region: "ap-shanghai",
      prefix: "qintopia-agent-os",
      request_key: `qintopia-agent-os/deploy-requests/production/requests/${requestId}.json`,
      result_key: `qintopia-agent-os/deploy-results/production/${requestId}.json`,
    },
  };
  const signatureMetadata = {
    algorithm: "hmac-sha256",
    issuer: "github-actions",
    key_id: keyId,
    signed_at: "2026-07-06T00:00:00Z",
  };
  request.signature = {
    ...signatureMetadata,
    value: signRequest(request, signatureMetadata),
  };

  const requestFile = path.join(tmpRoot, "request.json");
  fs.writeFileSync(requestFile, `${JSON.stringify(request, null, 2)}\n`, "utf8");

  writeExecutable(
    "bin/flock",
    `#!/usr/bin/env bash
set -euo pipefail
if [[ "\${1:-}" == "-n" ]]; then
  shift
fi
if [[ "$#" -eq 1 && "\${1:-}" =~ ^[0-9]+$ ]]; then
  exit 0
fi
"$@"
`
  );
  writeExecutable(
    "bin/readlink",
    `#!/usr/bin/env bash
set -euo pipefail
if [[ "\${1:-}" == "-f" ]]; then
  python3 - "\${2:-}" <<'PY'
from pathlib import Path
import sys
print(Path(sys.argv[1]).resolve())
PY
  exit 0
fi
/usr/bin/readlink "$@"
`
  );
  writeExecutable(
    "deploy/runner/promote-release.sh",
    `#!/usr/bin/env bash
echo "simulated promote failure" >&2
exit 42
`
  );
  writeExecutable(
    "deploy/runner/smoke-release.sh",
    `#!/usr/bin/env bash
echo "smoke must not run after promote failure" >&2
exit 43
`
  );
  writeExecutable(
    "deploy/runner/rollback-release.sh",
    `#!/usr/bin/env bash
echo "rollback must not run before current is promoted" >&2
exit 44
`
  );

  const result = spawnSync(
    "bash",
    [
      path.join(repoRoot, "deploy/runner/qintopia-agent-os-deploy-runner"),
      "--request-file",
      requestFile,
    ],
    {
      cwd: tmpRoot,
      env: {
        ...process.env,
        PATH: `${path.join(tmpRoot, "bin")}${path.delimiter}${process.env.PATH ?? ""}`,
        QINTOPIA_DEPLOY_RUNNER_STATE_DIR: stateDir,
        QINTOPIA_RELEASE_ROOT: releaseRoot,
        QINTOPIA_COS_ENV_FILE: path.join(tmpRoot, "missing.env"),
        DEPLOY_REQUEST_SIGNING_KEY: signingKey,
        DEPLOY_REQUEST_SIGNING_KEY_ID: keyId,
        TENCENT_COS_BUCKET: "qintopia-agent-os-artifacts-1305166808",
        TENCENT_COS_REGION: "ap-shanghai",
      },
      encoding: "utf8",
    }
  );

  if (result.status !== 42) {
    throw new Error(
      `expected runner to return promote failure status 42, got ${result.status}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`
    );
  }
  if (result.stderr.includes("smoke must not run")) {
    throw new Error("runner executed smoke after promote failure");
  }

  const resultPath = path.join(stateDir, "results", `${requestId}.json`);
  if (!fs.existsSync(resultPath)) {
    throw new Error("runner did not write failed result");
  }
  const deployResult = JSON.parse(fs.readFileSync(resultPath, "utf8"));
  if (deployResult.status !== "failed") {
    throw new Error(`expected failed result, got ${deployResult.status}`);
  }
  if (deployResult.current_target) {
    throw new Error(
      "failed pre-promotion result must not report a promoted current target"
    );
  }

  fs.rmSync(stateDir, { recursive: true, force: true });
  fs.rmSync(releaseRoot, { recursive: true, force: true });
  fs.mkdirSync(path.join(stateDir, "results"), { recursive: true });
  fs.mkdirSync(releaseRoot, { recursive: true });

  writeExecutable(
    "deploy/runner/promote-release.sh",
    `#!/usr/bin/env bash
set -euo pipefail
release_root=""
while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --release-root)
      release_root="\${2:-}"
      shift 2
      ;;
    *)
      shift
      ;;
  esac
done
mkdir -p "\${release_root}/${sha}" "\${release_root}/${previousSha}"
ln -sfn "\${release_root}/${previousSha}" "\${release_root}/previous"
ln -sfn "\${release_root}/${sha}" "\${release_root}/current"
`
  );
  writeExecutable(
    "deploy/runner/install-release-systemd-units.sh",
    `#!/usr/bin/env bash
echo "simulated systemd install failure" >&2
exit 55
`
  );
  writeExecutable(
    "deploy/runner/rollback-release.sh",
    `#!/usr/bin/env bash
set -euo pipefail
release_root=""
while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --release-root)
      release_root="\${2:-}"
      shift 2
      ;;
    *)
      shift
      ;;
  esac
done
target="$(readlink -f "\${release_root}/previous")"
ln -sfn "$target" "\${release_root}/current"
`
  );
  writeExecutable(
    "deploy/runner/smoke-release.sh",
    `#!/usr/bin/env bash
exit 0
`
  );

  const promotedResult = spawnSync(
    "bash",
    [
      path.join(repoRoot, "deploy/runner/qintopia-agent-os-deploy-runner"),
      "--request-file",
      requestFile,
    ],
    {
      cwd: tmpRoot,
      env: {
        ...process.env,
        PATH: `${path.join(tmpRoot, "bin")}${path.delimiter}${process.env.PATH ?? ""}`,
        QINTOPIA_DEPLOY_RUNNER_STATE_DIR: stateDir,
        QINTOPIA_RELEASE_ROOT: releaseRoot,
        QINTOPIA_COS_ENV_FILE: path.join(tmpRoot, "missing.env"),
        DEPLOY_REQUEST_SIGNING_KEY: signingKey,
        DEPLOY_REQUEST_SIGNING_KEY_ID: keyId,
        TENCENT_COS_BUCKET: "qintopia-agent-os-artifacts-1305166808",
        TENCENT_COS_REGION: "ap-shanghai",
      },
      encoding: "utf8",
    }
  );

  if (promotedResult.status !== 55) {
    throw new Error(
      `expected runner to return install failure status 55, got ${promotedResult.status}\nstdout:\n${promotedResult.stdout}\nstderr:\n${promotedResult.stderr}`
    );
  }

  const promotedDeployResult = JSON.parse(fs.readFileSync(resultPath, "utf8"));
  if (promotedDeployResult.status !== "rolled_back") {
    throw new Error(`expected rolled_back result, got ${promotedDeployResult.status}`);
  }
  if (
    promotedDeployResult.error !==
    "deployment failed during install-release-systemd-units (exit 55) and rollback succeeded"
  ) {
    throw new Error(`expected diagnostic error, got ${promotedDeployResult.error}`);
  }
  const detail = JSON.parse(promotedDeployResult.checks[0].detail);
  if (
    detail.failure_stage !== "install-release-systemd-units" ||
    detail.exit_status !== 55 ||
    detail.promoted_current !== true ||
    detail.profile_activation_attempted !== false
  ) {
    throw new Error(
      `expected deploy-runner failure detail, got ${promotedDeployResult.checks[0].detail}`
    );
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Deploy runner promotion failure behavior test passed.");
