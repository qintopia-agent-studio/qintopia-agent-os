#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpRoot = fs.mkdtempSync(
  path.join(os.tmpdir(), "qintopia-production-activation-test-")
);

const signingKey = "test-signing-key";
const keyId = "production";
const requestId = "deploy-20260809T000000Z-abcdef123456";
const sha = "113ce49141b06fc44edcee42026aee0a614ac027";

const writeExecutable = (relativePath, content) => {
  const filePath = path.join(tmpRoot, relativePath);
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, content, "utf8");
  fs.chmodSync(filePath, 0o755);
  return filePath;
};

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

const buildRequest = (overrides = {}) => {
  const effectiveRequestId = overrides.request_id ?? requestId;
  const request = {
    schema_version: 1,
    request_id: effectiveRequestId,
    environment: "production",
    repository: "qintopia-agent-studio/qintopia-agent-os",
    requested_by: "codex",
    created_at: "2026-08-09T00:00:00Z",
    expires_at: "2099-08-09T01:00:00Z",
    commit_sha: sha,
    runtime_sha: sha,
    runtime_artifact_profile: "huabaosi-production",
    deploy_bundle_sha: sha,
    release_sha: sha,
    release_scope: ["production-activation"],
    restart_targets: ["qintopia-system-services"],
    rollback_on_smoke_failure: false,
    dry_run: false,
    activation: {
      targets: [
        "erhua-morning-brief",
        "xiaoman-weekly-recruitment",
        "xiaoman-weekly-plan-confirmation",
        "xiaoman-weekly-preview",
      ],
    },
    cos: {
      bucket: "qintopia-agent-os-artifacts-1305166808",
      region: "ap-shanghai",
      prefix: "qintopia-agent-os",
      request_key: `qintopia-agent-os/deploy-requests/production/requests/${effectiveRequestId}.json`,
      result_key: `qintopia-agent-os/deploy-results/production/${effectiveRequestId}.json`,
    },
    ...overrides,
  };
  const signatureMetadata = {
    algorithm: "hmac-sha256",
    issuer: "github-actions",
    key_id: keyId,
    signed_at: "2026-08-09T00:00:00Z",
  };
  request.signature = {
    ...signatureMetadata,
    value: signRequest(request, signatureMetadata),
  };
  return request;
};

try {
  const runnerPath = writeExecutable(
    "deploy/runner/qintopia-agent-os-deploy-runner",
    fs.readFileSync(
      path.join(repoRoot, "deploy/runner/qintopia-agent-os-deploy-runner"),
      "utf8"
    )
  );
  const stateDir = path.join(tmpRoot, "state");
  const releaseRoot = path.join(tmpRoot, "releases");
  const releaseDir = path.join(releaseRoot, sha);
  const scriptsDir = path.join(releaseDir, "deploy/sidecar/scripts");
  const activationLog = path.join(tmpRoot, "activation.log");
  fs.mkdirSync(scriptsDir, { recursive: true });
  fs.mkdirSync(path.join(stateDir, "results"), { recursive: true });
  fs.writeFileSync(
    path.join(releaseDir, "manifest.json"),
    `${JSON.stringify(
      {
        release_sha: sha,
        runtime_sha: sha,
        deploy_bundle_sha: sha,
        commit_sha: sha,
      },
      null,
      2
    )}\n`,
    "utf8"
  );
  fs.symlinkSync(releaseDir, path.join(releaseRoot, "current"));

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

  const scriptNames = [
    "erhua-legacy-cron-observation-smoke.sh",
    "xiaoman-legacy-cron-observation-smoke.sh",
    "activate-erhua-morning-brief-production.sh",
    "erhua-morning-brief-timer-observation-smoke.sh",
    "activate-xiaoman-weekly-recruitment-production.sh",
    "xiaoman-weekly-recruitment-production-observation-smoke.sh",
    "activate-xiaoman-weekly-plan-confirmation-production.sh",
    "xiaoman-weekly-plan-confirmation-production-observation-smoke.sh",
    "activate-xiaoman-weekly-preview-production.sh",
    "xiaoman-weekly-preview-production-observation-smoke.sh",
  ];
  for (const scriptName of scriptNames) {
    writeExecutable(
      path.relative(tmpRoot, path.join(scriptsDir, scriptName)),
      `#!/usr/bin/env bash
set -euo pipefail
printf '%s\\n' ${JSON.stringify(scriptName)} >> ${JSON.stringify(activationLog)}
`
    );
  }
  for (const scriptName of [
    "apply-erhua-morning-brief-production-config.sh",
    "apply-xiaoman-weekly-preview-production-config.sh",
  ]) {
    writeExecutable(
      path.relative(tmpRoot, path.join(scriptsDir, scriptName)),
      `#!/usr/bin/env bash
set -euo pipefail
echo "${scriptName} must not be called by production activation" >&2
exit 99
`
    );
  }

  const requestFile = path.join(tmpRoot, "activation-request.json");
  fs.writeFileSync(requestFile, `${JSON.stringify(buildRequest(), null, 2)}\n`, "utf8");

  const env = {
    ...process.env,
    PATH: `${path.join(tmpRoot, "bin")}${path.delimiter}${process.env.PATH ?? ""}`,
    QINTOPIA_DEPLOY_RUNNER_STATE_DIR: stateDir,
    QINTOPIA_RELEASE_ROOT: releaseRoot,
    QINTOPIA_COS_ENV_FILE: path.join(tmpRoot, "missing.env"),
    DEPLOY_REQUEST_SIGNING_KEY: signingKey,
    DEPLOY_REQUEST_SIGNING_KEY_ID: keyId,
    TENCENT_COS_BUCKET: "qintopia-agent-os-artifacts-1305166808",
    TENCENT_COS_REGION: "ap-shanghai",
  };
  const result = spawnSync("bash", [runnerPath, "--request-file", requestFile], {
    cwd: stateDir,
    env,
    encoding: "utf8",
  });
  if (result.status !== 0) {
    throw new Error(
      `expected production activation to pass, got ${result.status}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`
    );
  }

  const deployResult = JSON.parse(
    fs.readFileSync(path.join(stateDir, "results", `${requestId}.json`), "utf8")
  );
  if (deployResult.status !== "succeeded") {
    throw new Error(`expected succeeded deploy result, got ${deployResult.status}`);
  }
  const activationCheck = deployResult.checks.find(
    (check) => check.name === "production-timer-activation"
  );
  if (!activationCheck || activationCheck.status !== "passed") {
    throw new Error("production activation check was not recorded as passed");
  }
  const activationDetail = JSON.parse(activationCheck.detail);
  const passedTargets = activationDetail.targets.map((target) => [
    target.target,
    target.status,
  ]);
  const expectedTargets = [
    ["erhua-morning-brief", "passed"],
    ["xiaoman-weekly-recruitment", "passed"],
    ["xiaoman-weekly-plan-confirmation", "passed"],
    ["xiaoman-weekly-preview", "passed"],
  ];
  if (JSON.stringify(passedTargets) !== JSON.stringify(expectedTargets)) {
    throw new Error(
      `unexpected activation target evidence ${JSON.stringify(passedTargets)}`
    );
  }

  const failedPreviewRequestId = "deploy-20260809T000001Z-abcdef123456";
  writeExecutable(
    path.relative(
      tmpRoot,
      path.join(scriptsDir, "activate-xiaoman-weekly-preview-production.sh")
    ),
    `#!/usr/bin/env bash
set -euo pipefail
echo "xiaoman weekly preview activation requires QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_ENABLED=1" >&2
exit 42
`
  );
  const failedPreviewRequestPath = path.join(
    tmpRoot,
    "failed-preview-activation-request.json"
  );
  fs.writeFileSync(
    failedPreviewRequestPath,
    `${JSON.stringify(
      buildRequest({
        request_id: failedPreviewRequestId,
        activation: {
          targets: ["xiaoman-weekly-preview"],
        },
      }),
      null,
      2
    )}\n`,
    "utf8"
  );
  const failedPreview = spawnSync(
    "bash",
    [runnerPath, "--request-file", failedPreviewRequestPath],
    {
      cwd: stateDir,
      env,
      encoding: "utf8",
    }
  );
  if (failedPreview.status !== 42) {
    throw new Error(
      `expected failed preview activation to exit 42, got ${failedPreview.status}\nstdout:\n${failedPreview.stdout}\nstderr:\n${failedPreview.stderr}`
    );
  }
  const failedPreviewResult = JSON.parse(
    fs.readFileSync(
      path.join(stateDir, "results", `${failedPreviewRequestId}.json`),
      "utf8"
    )
  );
  const failedPreviewCheck = failedPreviewResult.checks.find(
    (check) => check.name === "production-timer-activation"
  );
  if (!failedPreviewCheck || failedPreviewCheck.status !== "failed") {
    throw new Error("failed preview activation check was not recorded");
  }
  const failedPreviewDetail = JSON.parse(failedPreviewCheck.detail);
  const failedPreviewTarget = failedPreviewDetail.targets[0];
  if (
    failedPreviewTarget.status !== "failed" ||
    !failedPreviewTarget.detail.includes(
      "exit 42: xiaoman weekly preview activation requires QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_ENABLED=1"
    )
  ) {
    throw new Error(
      `expected failed preview detail to include target stderr, got ${JSON.stringify(
        failedPreviewTarget
      )}`
    );
  }

  const ordinaryRequestPath = path.join(tmpRoot, "ordinary-request.json");
  fs.writeFileSync(
    ordinaryRequestPath,
    `${JSON.stringify(
      buildRequest({
        request_id: "deploy-20260809T000002Z-abcdef123456",
        release_scope: ["deploy-bundle"],
        dry_run: true,
      }),
      null,
      2
    )}\n`,
    "utf8"
  );
  const rejection = spawnSync(
    "bash",
    [runnerPath, "--request-file", ordinaryRequestPath],
    {
      cwd: stateDir,
      env,
      encoding: "utf8",
    }
  );
  if (
    rejection.status === 0 ||
    !rejection.stderr.includes(
      "activation metadata is only allowed for production-activation"
    )
  ) {
    throw new Error(
      `expected activation metadata rejection, got ${rejection.status}\nstderr:\n${rejection.stderr}`
    );
  }

  console.log("Production timer activation runner test passed.");
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}
