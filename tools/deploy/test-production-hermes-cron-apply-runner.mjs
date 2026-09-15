#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpRoot = fs.mkdtempSync(
  path.join(os.tmpdir(), "qintopia-production-hermes-cron-apply-test-")
);

const signingKey = "test-signing-key";
const keyId = "production";
const requestId = "deploy-20260811T010000Z-abcdef123456";
const sha = "f426017c852acc6ed1d554a9b64ffd90d303bbc4";
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
    release_scope: ["production-hermes-cron-apply"],
    restart_targets: ["qintopia-system-services"],
    rollback_on_smoke_failure: false,
    dry_run: false,
    hermes_cron_apply: {
      mode: "install",
      targets: [
        "erhua-morning-brief",
        "erhua-activity-recruitment",
        "xiaoman-daily-case-report",
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
    signed_at: request.created_at,
  };
  request.signature = {
    ...signatureMetadata,
    value: signRequest(request, signatureMetadata),
  };
  return request;
};

const readDeployResult = (stateDir, id) =>
  JSON.parse(fs.readFileSync(path.join(stateDir, "results", `${id}.json`), "utf8"));

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
  const applyLog = path.join(tmpRoot, "apply.log");
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

  const applyScripts = [
    [
      "apply-erhua-morning-brief-hermes-cron.sh",
      "QINTOPIA_ERHUA_MORNING_BRIEF_HERMES_CRON",
      "approved-production-erhua-morning-brief-hermes-cron",
    ],
    [
      "apply-erhua-activity-recruitment-hermes-cron.sh",
      "QINTOPIA_ERHUA_ACTIVITY_RECRUITMENT_HERMES_CRON",
      "approved-production-erhua-activity-recruitment-hermes-cron",
    ],
    [
      "apply-xiaoman-daily-case-report-hermes-cron.sh",
      "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_HERMES_CRON",
      "approved-production-xiaoman-daily-case-report-hermes-cron",
    ],
    [
      "apply-xiaoman-weekly-recruitment-hermes-cron.sh",
      "QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_HERMES_CRON",
      "approved-production-xiaoman-weekly-recruitment-hermes-cron",
    ],
    [
      "apply-xiaoman-weekly-plan-confirmation-hermes-cron.sh",
      "QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_HERMES_CRON",
      "approved-production-xiaoman-weekly-plan-confirmation-hermes-cron",
    ],
    [
      "apply-xiaoman-weekly-preview-hermes-cron.sh",
      "QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_HERMES_CRON",
      "approved-production-xiaoman-weekly-preview-hermes-cron",
    ],
  ];

  for (const [scriptName, envName, approval] of applyScripts) {
    writeExecutable(
      path.relative(tmpRoot, path.join(scriptsDir, scriptName)),
      `#!/usr/bin/env bash
set -euo pipefail
if [[ "\${${envName}:-}" != "${approval}" ]]; then
  echo "missing fixed approval for ${scriptName}" >&2
  exit 44
fi
printf '%s %s\\n' ${JSON.stringify(scriptName)} "\${1:-}" >> ${JSON.stringify(applyLog)}
`
    );
  }
  for (const scriptName of [
    "activate-xiaoman-weekly-preview-production.sh",
    "retire-xiaoman-legacy-cron-production.sh",
    "xiaoman-daily-case-report-auto-publish-backfill.sh",
  ]) {
    writeExecutable(
      path.relative(tmpRoot, path.join(scriptsDir, scriptName)),
      `#!/usr/bin/env bash
set -euo pipefail
echo "${scriptName} must not be called by production Hermes cron apply" >&2
exit 99
`
    );
  }

  const requestFile = path.join(tmpRoot, "hermes-cron-apply-request.json");
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
      `expected production Hermes cron apply to pass, got ${result.status}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`
    );
  }

  const deployResult = readDeployResult(stateDir, requestId);
  if (deployResult.status !== "succeeded") {
    throw new Error(`expected succeeded deploy result, got ${deployResult.status}`);
  }
  const applyCheck = deployResult.checks.find(
    (check) => check.name === "production-hermes-cron-apply"
  );
  if (!applyCheck || applyCheck.status !== "passed") {
    throw new Error("Hermes cron apply check was not recorded as passed");
  }
  const applyDetail = JSON.parse(applyCheck.detail);
  const passedTargets = applyDetail.targets.map((target) => [
    target.target,
    target.mode,
    target.status,
    target.detail,
  ]);
  const expectedTargets = [
    ["erhua-morning-brief", "install", "passed", "mode=install"],
    ["erhua-activity-recruitment", "install", "passed", "mode=install"],
    ["xiaoman-daily-case-report", "install", "passed", "mode=install"],
    ["xiaoman-weekly-recruitment", "install", "passed", "mode=install"],
    ["xiaoman-weekly-plan-confirmation", "install", "passed", "mode=install"],
    ["xiaoman-weekly-preview", "install", "passed", "mode=install"],
  ];
  if (JSON.stringify(passedTargets) !== JSON.stringify(expectedTargets)) {
    throw new Error(
      `unexpected Hermes cron apply target evidence ${JSON.stringify(passedTargets)}`
    );
  }
  const expectedLog = [
    "apply-erhua-morning-brief-hermes-cron.sh --install",
    "apply-erhua-activity-recruitment-hermes-cron.sh --install",
    "apply-xiaoman-daily-case-report-hermes-cron.sh --install",
    "apply-xiaoman-weekly-recruitment-hermes-cron.sh --install",
    "apply-xiaoman-weekly-plan-confirmation-hermes-cron.sh --install",
    "apply-xiaoman-weekly-preview-hermes-cron.sh --install",
  ].join("\n");
  const actualLog = fs.readFileSync(applyLog, "utf8").trim();
  if (actualLog !== expectedLog) {
    throw new Error(`unexpected apply log ${JSON.stringify(actualLog)}`);
  }

  const enableRequestId = "deploy-20260811T010001Z-abcdef123456";
  const enableRequestPath = path.join(tmpRoot, "hermes-cron-enable-request.json");
  fs.writeFileSync(
    enableRequestPath,
    `${JSON.stringify(
      buildRequest({
        request_id: enableRequestId,
        hermes_cron_apply: {
          mode: "enable",
          targets: ["xiaoman-weekly-preview"],
        },
      }),
      null,
      2
    )}\n`,
    "utf8"
  );
  const enableResult = spawnSync(
    "bash",
    [runnerPath, "--request-file", enableRequestPath],
    {
      cwd: stateDir,
      env,
      encoding: "utf8",
    }
  );
  if (enableResult.status !== 0) {
    throw new Error(
      `expected production Hermes cron enable to pass, got ${enableResult.status}\nstdout:\n${enableResult.stdout}\nstderr:\n${enableResult.stderr}`
    );
  }
  const enableDeployResult = readDeployResult(stateDir, enableRequestId);
  const enableCheck = enableDeployResult.checks.find(
    (check) => check.name === "production-hermes-cron-apply"
  );
  const enableDetail = JSON.parse(enableCheck.detail);
  if (
    enableDetail.mode !== "enable" ||
    enableDetail.targets.length !== 1 ||
    enableDetail.targets[0].target !== "xiaoman-weekly-preview" ||
    enableDetail.targets[0].detail !== "mode=enable"
  ) {
    throw new Error(`unexpected enable evidence ${JSON.stringify(enableDetail)}`);
  }

  const failedRequestId = "deploy-20260811T010002Z-abcdef123456";
  writeExecutable(
    path.relative(
      tmpRoot,
      path.join(scriptsDir, "apply-xiaoman-weekly-preview-hermes-cron.sh")
    ),
    `#!/usr/bin/env bash
set -euo pipefail
echo "raw live cron secret group-id-fixture" >&2
exit 42
`
  );
  const failedRequestPath = path.join(tmpRoot, "failed-hermes-cron-apply-request.json");
  fs.writeFileSync(
    failedRequestPath,
    `${JSON.stringify(
      buildRequest({
        request_id: failedRequestId,
        hermes_cron_apply: {
          mode: "install",
          targets: ["xiaoman-weekly-preview"],
        },
      }),
      null,
      2
    )}\n`,
    "utf8"
  );
  const failed = spawnSync("bash", [runnerPath, "--request-file", failedRequestPath], {
    cwd: stateDir,
    env,
    encoding: "utf8",
  });
  if (failed.status !== 42) {
    throw new Error(
      `expected failed Hermes cron apply to exit 42, got ${failed.status}\nstdout:\n${failed.stdout}\nstderr:\n${failed.stderr}`
    );
  }
  const failedDeployResult = readDeployResult(stateDir, failedRequestId);
  const failedCheck = failedDeployResult.checks.find(
    (check) => check.name === "production-hermes-cron-apply"
  );
  if (!failedCheck || failedCheck.status !== "failed") {
    throw new Error("failed Hermes cron apply check was not recorded");
  }
  if (failedCheck.detail.includes("group-id-fixture")) {
    throw new Error("failed Hermes cron apply leaked raw script stderr");
  }
  const failedDetail = JSON.parse(failedCheck.detail);
  if (
    failedDetail.targets[0].status !== "failed" ||
    failedDetail.targets[0].detail !== "exit 42"
  ) {
    throw new Error(`unexpected failed Hermes cron apply detail ${failedCheck.detail}`);
  }

  const safeFailedRequestId = "deploy-20260811T010004Z-abcdef123456";
  const safeFailureMessage =
    "reviewed weekly preview job schedule does not match the reviewed declaration";
  writeExecutable(
    path.relative(
      tmpRoot,
      path.join(scriptsDir, "apply-xiaoman-weekly-preview-hermes-cron.sh")
    ),
    `#!/usr/bin/env bash
set -euo pipefail
echo "raw live cron secret group-id-fixture" >&2
echo "qintopia_hermes_cron_apply_safe_failure=${safeFailureMessage}" >&2
exit 45
`
  );
  const safeFailedRequestPath = path.join(
    tmpRoot,
    "safe-failed-hermes-cron-apply-request.json"
  );
  fs.writeFileSync(
    safeFailedRequestPath,
    `${JSON.stringify(
      buildRequest({
        request_id: safeFailedRequestId,
        hermes_cron_apply: {
          mode: "install",
          targets: ["xiaoman-weekly-preview"],
        },
      }),
      null,
      2
    )}\n`,
    "utf8"
  );
  const safeFailed = spawnSync(
    "bash",
    [runnerPath, "--request-file", safeFailedRequestPath],
    {
      cwd: stateDir,
      env,
      encoding: "utf8",
    }
  );
  if (safeFailed.status !== 45) {
    throw new Error(
      `expected safe failed Hermes cron apply to exit 45, got ${safeFailed.status}\nstdout:\n${safeFailed.stdout}\nstderr:\n${safeFailed.stderr}`
    );
  }
  const safeFailedDeployResult = readDeployResult(stateDir, safeFailedRequestId);
  const safeFailedCheck = safeFailedDeployResult.checks.find(
    (check) => check.name === "production-hermes-cron-apply"
  );
  if (safeFailedCheck.detail.includes("group-id-fixture")) {
    throw new Error("safe failed Hermes cron apply leaked raw script stderr");
  }
  const safeFailedDetail = JSON.parse(safeFailedCheck.detail);
  if (safeFailedDetail.targets[0].detail !== `exit 45: ${safeFailureMessage}`) {
    throw new Error(
      `unexpected safe failed Hermes cron apply detail ${safeFailedCheck.detail}`
    );
  }

  const ordinaryRequestPath = path.join(tmpRoot, "ordinary-request.json");
  fs.writeFileSync(
    ordinaryRequestPath,
    `${JSON.stringify(
      buildRequest({
        request_id: "deploy-20260811T010003Z-abcdef123456",
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
      "hermes_cron_apply metadata is only allowed for production-hermes-cron-apply"
    )
  ) {
    throw new Error(
      `expected hermes_cron_apply metadata rejection, got ${rejection.status}\nstderr:\n${rejection.stderr}`
    );
  }

  console.log("Production Hermes cron apply runner test passed.");
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}
