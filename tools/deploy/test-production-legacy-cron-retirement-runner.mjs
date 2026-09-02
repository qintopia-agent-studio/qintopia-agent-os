#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpRoot = fs.mkdtempSync(
  path.join(os.tmpdir(), "qintopia-production-legacy-cron-retirement-test-")
);

const signingKey = "test-signing-key";
const keyId = "production";
const requestId = "deploy-20260809T010000Z-abcdef123456";
const sha = "113ce49141b06fc44edcee42026aee0a614ac027";
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
    commit_sha: sha,
    runtime_sha: sha,
    runtime_artifact_profile: "huabaosi-production",
    deploy_bundle_sha: sha,
    release_sha: sha,
    release_scope: ["production-legacy-cron-retirement"],
    restart_targets: ["qintopia-system-services"],
    rollback_on_smoke_failure: false,
    dry_run: false,
    legacy_cron_retirement: {
      targets: ["erhua-legacy-cron", "xiaoman-legacy-cron"],
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
    signed_at: request.created_at,
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
  const retirementLog = path.join(tmpRoot, "retirement.log");
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

  for (const scriptName of [
    "retire-erhua-legacy-cron-production.sh",
    "erhua-legacy-cron-observation-smoke.sh",
    "retire-xiaoman-legacy-cron-production.sh",
    "xiaoman-legacy-cron-observation-smoke.sh",
  ]) {
    writeExecutable(
      path.relative(tmpRoot, path.join(scriptsDir, scriptName)),
      `#!/usr/bin/env bash
set -euo pipefail
printf '%s\\n' ${JSON.stringify(scriptName)} >> ${JSON.stringify(retirementLog)}
`
    );
  }
  for (const scriptName of [
    "activate-erhua-morning-brief-production.sh",
    "activate-xiaoman-weekly-recruitment-production.sh",
    "activate-xiaoman-weekly-plan-confirmation-production.sh",
    "activate-xiaoman-weekly-preview-production.sh",
    "apply-erhua-morning-brief-production-config.sh",
    "apply-xiaoman-weekly-preview-production-config.sh",
  ]) {
    writeExecutable(
      path.relative(tmpRoot, path.join(scriptsDir, scriptName)),
      `#!/usr/bin/env bash
set -euo pipefail
echo "${scriptName} must not be called by production legacy cron retirement" >&2
exit 99
`
    );
  }

  const requestFile = path.join(tmpRoot, "retirement-request.json");
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
      `expected production legacy cron retirement to pass, got ${result.status}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`
    );
  }

  const deployResult = JSON.parse(
    fs.readFileSync(path.join(stateDir, "results", `${requestId}.json`), "utf8")
  );
  if (deployResult.status !== "succeeded") {
    throw new Error(`expected succeeded deploy result, got ${deployResult.status}`);
  }
  const retirementCheck = deployResult.checks.find(
    (check) => check.name === "production-legacy-cron-retirement"
  );
  if (!retirementCheck || retirementCheck.status !== "passed") {
    throw new Error("legacy cron retirement check was not recorded as passed");
  }
  const retirementDetail = JSON.parse(retirementCheck.detail);
  const passedTargets = retirementDetail.targets.map((target) => [
    target.target,
    target.status,
  ]);
  const expectedTargets = [
    ["erhua-legacy-cron", "passed"],
    ["xiaoman-legacy-cron", "passed"],
  ];
  if (JSON.stringify(passedTargets) !== JSON.stringify(expectedTargets)) {
    throw new Error(
      `unexpected legacy cron retirement target evidence ${JSON.stringify(
        passedTargets
      )}`
    );
  }
  const expectedLog = [
    "retire-erhua-legacy-cron-production.sh",
    "erhua-legacy-cron-observation-smoke.sh",
    "retire-xiaoman-legacy-cron-production.sh",
    "xiaoman-legacy-cron-observation-smoke.sh",
  ].join("\n");
  const actualLog = fs.readFileSync(retirementLog, "utf8").trim();
  if (actualLog !== expectedLog) {
    throw new Error(`unexpected retirement log ${JSON.stringify(actualLog)}`);
  }

  const failedXiaomanRequestId = "deploy-20260809T010001Z-abcdef123456";
  writeExecutable(
    path.relative(
      tmpRoot,
      path.join(scriptsDir, "retire-xiaoman-legacy-cron-production.sh")
    ),
    `#!/usr/bin/env bash
set -euo pipefail
echo "Xiaoman legacy cron retirement failed: legacy cron file sha256 does not match the reviewed production observation" >&2
exit 42
`
  );
  const failedXiaomanRequestPath = path.join(
    tmpRoot,
    "failed-xiaoman-retirement-request.json"
  );
  fs.writeFileSync(
    failedXiaomanRequestPath,
    `${JSON.stringify(
      buildRequest({
        request_id: failedXiaomanRequestId,
        legacy_cron_retirement: {
          targets: ["xiaoman-legacy-cron"],
        },
      }),
      null,
      2
    )}\n`,
    "utf8"
  );
  const failedXiaoman = spawnSync(
    "bash",
    [runnerPath, "--request-file", failedXiaomanRequestPath],
    {
      cwd: stateDir,
      env,
      encoding: "utf8",
    }
  );
  if (failedXiaoman.status !== 42) {
    throw new Error(
      `expected failed Xiaoman retirement to exit 42, got ${failedXiaoman.status}\nstdout:\n${failedXiaoman.stdout}\nstderr:\n${failedXiaoman.stderr}`
    );
  }
  const failedXiaomanResult = JSON.parse(
    fs.readFileSync(
      path.join(stateDir, "results", `${failedXiaomanRequestId}.json`),
      "utf8"
    )
  );
  const failedXiaomanCheck = failedXiaomanResult.checks.find(
    (check) => check.name === "production-legacy-cron-retirement"
  );
  if (!failedXiaomanCheck || failedXiaomanCheck.status !== "failed") {
    throw new Error("failed Xiaoman retirement check was not recorded");
  }
  const failedXiaomanDetail = JSON.parse(failedXiaomanCheck.detail);
  const failedXiaomanTarget = failedXiaomanDetail.targets[0];
  if (
    failedXiaomanTarget.status !== "failed" ||
    !failedXiaomanTarget.detail.includes(
      "exit 42: Xiaoman legacy cron retirement failed: legacy cron file sha256 does not match the reviewed production observation"
    )
  ) {
    throw new Error(
      `expected failed Xiaoman detail to include target stderr, got ${JSON.stringify(
        failedXiaomanTarget
      )}`
    );
  }

  const ordinaryRequestPath = path.join(tmpRoot, "ordinary-request.json");
  fs.writeFileSync(
    ordinaryRequestPath,
    `${JSON.stringify(
      buildRequest({
        request_id: "deploy-20260809T010002Z-abcdef123456",
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
      "legacy_cron_retirement metadata is only allowed for production-legacy-cron-retirement"
    )
  ) {
    throw new Error(
      `expected legacy_cron_retirement metadata rejection, got ${rejection.status}\nstderr:\n${rejection.stderr}`
    );
  }

  console.log("Production legacy cron retirement runner test passed.");
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}
