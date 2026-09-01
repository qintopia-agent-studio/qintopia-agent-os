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
const commitSha = "1".repeat(40);
const runtimeSha = "2".repeat(40);
const deployBundleSha = "3".repeat(40);
const createdAt = new Date().toISOString();
const expiresAt = new Date(Date.parse(createdAt) + 60 * 60 * 1000).toISOString();

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

const resignRequest = (request, signedAt = request.created_at) => {
  delete request.signature;
  const signatureMetadata = {
    algorithm: "hmac-sha256",
    issuer: "github-actions",
    key_id: keyId,
    signed_at: signedAt,
  };
  request.signature = {
    ...signatureMetadata,
    value: signRequest(request, signatureMetadata),
  };
  return request;
};

const buildRequest = (overrides = {}) => {
  const effectiveRequestId = overrides.request_id ?? requestId;
  const request = {
    schema_version: 1,
    request_id: effectiveRequestId,
    environment: "production",
    repository: "qintopia-agent-studio/qintopia-agent-os",
    requested_by: "codex",
    created_at: createdAt,
    expires_at: expiresAt,
    commit_sha: commitSha,
    runtime_sha: runtimeSha,
    runtime_artifact_profile: "huabaosi-production",
    deploy_bundle_sha: deployBundleSha,
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
  return resignRequest(request);
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
        runtime_sha: runtimeSha,
        deploy_bundle_sha: deployBundleSha,
        commit_sha: commitSha,
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
    "activate-space-automation-runtime-production.sh",
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
  writeExecutable(
    path.relative(
      tmpRoot,
      path.join(scriptsDir, "activate-space-automation-runtime-production.sh")
    ),
    `#!/usr/bin/env bash
set -euo pipefail
if [[ "\${QINTOPIA_SPACE_AUTOMATION_RUNTIME_ACTIVATION:-}" != "approved-production-space-automation-runtime" ]]; then
  exit 98
fi
if [[ "\${QINTOPIA_SPACE_AUTOMATION_RUNTIME_COMMIT_SHA:-}" != ${JSON.stringify(
      commitSha
    )} || "\${QINTOPIA_SPACE_AUTOMATION_RUNTIME_RUNTIME_SHA:-}" != ${JSON.stringify(
      runtimeSha
    )} || "\${QINTOPIA_SPACE_AUTOMATION_RUNTIME_DEPLOY_BUNDLE_SHA:-}" != ${JSON.stringify(
      deployBundleSha
    )} || "\${QINTOPIA_SPACE_AUTOMATION_RUNTIME_RELEASE_SHA:-}" != ${JSON.stringify(
      sha
    )} ]]; then
  exit 97
fi
printf '%s\n' "activate-space-automation-runtime-production.sh" >> ${JSON.stringify(
      activationLog
    )}
`
  );
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

  const spaceRuntimeRequestId = "deploy-20260809T000003Z-abcdef123456";
  const spaceRuntimeRequestPath = path.join(
    tmpRoot,
    "space-runtime-activation-request.json"
  );
  fs.writeFileSync(
    spaceRuntimeRequestPath,
    `${JSON.stringify(
      buildRequest({
        request_id: spaceRuntimeRequestId,
        activation: {
          targets: ["space-automation-runtime"],
          approval: "approved-production-space-automation-runtime",
        },
      }),
      null,
      2
    )}\n`,
    "utf8"
  );
  const spaceRuntimeActivation = spawnSync(
    "bash",
    [runnerPath, "--request-file", spaceRuntimeRequestPath],
    { cwd: stateDir, env, encoding: "utf8" }
  );
  if (spaceRuntimeActivation.status !== 0) {
    throw new Error(
      `Space runtime activation routing failed\n${spaceRuntimeActivation.stdout}\n${spaceRuntimeActivation.stderr}`
    );
  }
  const spaceRuntimeResult = JSON.parse(
    fs.readFileSync(
      path.join(stateDir, "results", `${spaceRuntimeRequestId}.json`),
      "utf8"
    )
  );
  const spaceRuntimeCheck = spaceRuntimeResult.checks.find(
    (check) => check.name === "production-timer-activation"
  );
  const spaceRuntimeDetail = JSON.parse(spaceRuntimeCheck.detail);
  if (
    spaceRuntimeDetail.targets[0].target !== "space-automation-runtime" ||
    spaceRuntimeDetail.targets[0].status !== "passed"
  ) {
    throw new Error(
      `unexpected Space runtime activation evidence ${spaceRuntimeCheck.detail}`
    );
  }

  const failedActivationRequestId = "deploy-20260809T000001Z-abcdef123456";
  const injectedSecret = "postgres://activation-secret@example.invalid/qintopia";
  writeExecutable(
    path.relative(
      tmpRoot,
      path.join(scriptsDir, "activate-space-automation-runtime-production.sh")
    ),
    `#!/usr/bin/env bash
set -euo pipefail
echo ${JSON.stringify(injectedSecret)} >&2
echo "qintopia_activation_safe_failure=preflight_failed" >&2
echo "qintopia_activation_safe_failure=${injectedSecret}" >&2
exit 42
`
  );
  const failedActivationRequestPath = path.join(
    tmpRoot,
    "failed-space-runtime-activation-request.json"
  );
  fs.writeFileSync(
    failedActivationRequestPath,
    `${JSON.stringify(
      buildRequest({
        request_id: failedActivationRequestId,
        activation: {
          targets: ["space-automation-runtime"],
          approval: "approved-production-space-automation-runtime",
        },
      }),
      null,
      2
    )}\n`,
    "utf8"
  );
  const failedActivation = spawnSync(
    "bash",
    [runnerPath, "--request-file", failedActivationRequestPath],
    {
      cwd: stateDir,
      env,
      encoding: "utf8",
    }
  );
  if (failedActivation.status !== 42) {
    throw new Error(
      `expected failed activation to exit 42, got ${failedActivation.status}\nstdout:\n${failedActivation.stdout}\nstderr:\n${failedActivation.stderr}`
    );
  }
  const failedActivationResult = JSON.parse(
    fs.readFileSync(
      path.join(stateDir, "results", `${failedActivationRequestId}.json`),
      "utf8"
    )
  );
  const failedActivationCheck = failedActivationResult.checks.find(
    (check) => check.name === "production-timer-activation"
  );
  if (!failedActivationCheck || failedActivationCheck.status !== "failed") {
    throw new Error("failed activation check was not recorded");
  }
  const failedActivationDetail = JSON.parse(failedActivationCheck.detail);
  const failedActivationTarget = failedActivationDetail.targets[0];
  if (
    failedActivationTarget.status !== "failed" ||
    failedActivationTarget.detail !== "exit 42: preflight_failed"
  ) {
    throw new Error(
      `expected failed activation detail to contain only the safe marker, got ${JSON.stringify(
        failedActivationTarget
      )}`
    );
  }
  const persistedFailure = JSON.stringify(failedActivationResult);
  if (
    persistedFailure.includes(injectedSecret) ||
    `${failedActivation.stdout}\n${failedActivation.stderr}`.includes(injectedSecret)
  ) {
    throw new Error(
      "production activation persisted or repeated injected child stderr"
    );
  }

  const unsupportedTargetPath = path.join(tmpRoot, "unsupported-activation.json");
  fs.writeFileSync(
    unsupportedTargetPath,
    `${JSON.stringify(
      buildRequest({
        request_id: "deploy-20260809T000004Z-abcdef123456",
        activation: {
          targets: ["space-automation-runtime", "xiaoman-weekly-preview"],
          approval: "approved-production-space-automation-runtime",
        },
      }),
      null,
      2
    )}\n`,
    "utf8"
  );
  const unsupportedTarget = spawnSync(
    "bash",
    [runnerPath, "--request-file", unsupportedTargetPath],
    { cwd: stateDir, env, encoding: "utf8" }
  );
  if (
    unsupportedTarget.status === 0 ||
    !unsupportedTarget.stderr.includes(
      "space-automation-runtime must be the sole activation target when selected"
    )
  ) {
    throw new Error("production activation accepted a non-Space runtime target");
  }

  const runRejectedTimestampRequest = (fileName, request, expectedMessage) => {
    const requestPath = path.join(tmpRoot, fileName);
    fs.writeFileSync(requestPath, `${JSON.stringify(request, null, 2)}\n`, "utf8");
    const rejected = spawnSync("bash", [runnerPath, "--request-file", requestPath], {
      cwd: stateDir,
      env,
      encoding: "utf8",
    });
    if (rejected.status === 0 || !rejected.stderr.includes(expectedMessage)) {
      throw new Error(
        `expected timestamp rejection ${JSON.stringify(expectedMessage)}, got ${rejected.status}\n${rejected.stderr}`
      );
    }
  };

  const staleCreatedAt = new Date(Date.now() - 16 * 60 * 1000).toISOString();
  runRejectedTimestampRequest(
    "stale-activation-request.json",
    buildRequest({
      request_id: "deploy-20260809T000005Z-abcdef123456",
      created_at: staleCreatedAt,
      expires_at: new Date(Date.parse(staleCreatedAt) + 60 * 60 * 1000).toISOString(),
    }),
    "request is stale"
  );

  runRejectedTimestampRequest(
    "long-lived-activation-request.json",
    buildRequest({
      request_id: "deploy-20260809T000006Z-abcdef123456",
      expires_at: new Date(Date.parse(createdAt) + 61 * 60 * 1000).toISOString(),
    }),
    "request TTL exceeds 60 minutes"
  );

  const signatureSkewRequest = buildRequest({
    request_id: "deploy-20260809T000007Z-abcdef123456",
  });
  resignRequest(
    signatureSkewRequest,
    new Date(Date.parse(signatureSkewRequest.created_at) + 6 * 60 * 1000).toISOString()
  );
  runRejectedTimestampRequest(
    "signature-skew-activation-request.json",
    signatureSkewRequest,
    "signature.signed_at must be within 5 minutes of created_at"
  );

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
