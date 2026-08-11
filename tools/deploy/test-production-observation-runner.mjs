#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpRoot = fs.mkdtempSync(
  path.join(os.tmpdir(), "qintopia-production-observation-test-")
);

const signingKey = "test-signing-key";
const keyId = "production";
const requestId = "deploy-20260810T000000Z-abcdef123456";
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
    created_at: "2026-08-10T00:00:00Z",
    expires_at: "2099-08-10T01:00:00Z",
    commit_sha: sha,
    runtime_sha: sha,
    runtime_artifact_profile: "huabaosi-production",
    deploy_bundle_sha: sha,
    release_sha: sha,
    release_scope: ["production-observation"],
    restart_targets: ["qintopia-system-services"],
    rollback_on_smoke_failure: false,
    dry_run: false,
    observation: {
      targets: [
        "qiwe-image-send",
        "xiaoman-daily-case-report-auto-publish",
        "hermes-cron-snapshot",
        "hermes-cron-live-parity",
        "erhua-morning-brief-worker-run",
        "xiaoman-weekly-recruitment-worker-run",
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
    signed_at: "2026-08-10T00:00:00Z",
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
  const observationLog = path.join(tmpRoot, "observation.log");
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

  writeExecutable(
    path.relative(
      tmpRoot,
      path.join(scriptsDir, "qiwe-image-send-production-observation-smoke.sh")
    ),
    `#!/usr/bin/env bash
set -euo pipefail
printf 'qiwe:%s\\n' "\${QINTOPIA_QIWE_IMAGE_SEND_EXPECTED_STATE:-}" >> ${JSON.stringify(
      observationLog
    )}
if [[ "\${QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_OBSERVATION_ENABLE:-}" != "1" ]]; then
  echo "QiWe image-send observation not enabled" >&2
  exit 3
fi
if [[ "\${QINTOPIA_QIWE_IMAGE_SEND_EXPECTED_STATE:-}" != "auto" ]]; then
  echo "QiWe image-send observation must use auto" >&2
  exit 4
fi
echo "qiwe_image_send_production_observation_state=disabled"
echo "DATABASE_URL=postgres://secret@example.invalid/qintopia"
echo "QIWE_TOKEN=secret-token"
echo "QiWe image-send production observation passed"
`
  );
  writeExecutable(
    path.relative(
      tmpRoot,
      path.join(
        scriptsDir,
        "xiaoman-daily-case-report-auto-publish-production-observation-smoke.sh"
      )
    ),
    `#!/usr/bin/env bash
set -euo pipefail
printf 'daily:%s\\n' "\${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_EXPECTED_STATE:-}" >> ${JSON.stringify(
      observationLog
    )}
case "\${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_EXPECTED_STATE:-}" in
  enabled)
    echo "xiaoman daily case report observation requires QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED=1" >&2
    echo "target_group_id=wx-secret-group" >&2
    exit 42
    ;;
  disabled)
    echo "xiaoman daily case report auto-publish observation passed"
    echo "QINTOPIA_SIDECAR_DATABASE_URL=postgres://secret@example.invalid/qintopia"
    ;;
  *)
    echo "unexpected daily expected state" >&2
    exit 43
    ;;
esac
`
  );
  writeExecutable(
    path.relative(
      tmpRoot,
      path.join(scriptsDir, "production-worker-run-evidence-smoke.sh")
    ),
    `#!/usr/bin/env bash
set -euo pipefail
printf 'worker:%s\\n' "\${1:-}" >> ${JSON.stringify(observationLog)}
if [[ "\${QINTOPIA_PRODUCTION_WORKER_RUN_EVIDENCE_ENABLE:-}" != "1" ]]; then
  echo "worker-run evidence not enabled" >&2
  exit 5
fi
case "\${1:-}" in
  erhua-morning-brief-worker-run)
    echo "erhua_morning_brief_worker_run_result=success"
    echo "erhua_morning_brief_worker_run_epoch=1786320600"
    echo "DATABASE_URL=postgres://secret@example.invalid/qintopia"
    ;;
  xiaoman-weekly-recruitment-worker-run)
    echo "xiaoman_weekly_recruitment_worker_run_result=not_started"
    ;;
  xiaoman-weekly-preview-worker-run)
    echo "xiaoman_weekly_preview_worker_run_error=worker_failed"
    echo "QIWE_TOKEN=secret-token" >&2
    exit 1
    ;;
  *)
    echo "unexpected worker-run evidence target: \${1:-}" >&2
    exit 6
    ;;
esac
`
  );
  writeExecutable(
    path.relative(
      tmpRoot,
      path.join(scriptsDir, "hermes-cron-snapshot-observation-smoke.sh")
    ),
    `#!/usr/bin/env bash
set -euo pipefail
printf 'snapshot:%s\\n' "\${QINTOPIA_HERMES_CRON_SNAPSHOT_OBSERVATION_ENABLE:-}" >> ${JSON.stringify(
      observationLog
    )}
if [[ "\${QINTOPIA_HERMES_CRON_SNAPSHOT_OBSERVATION_ENABLE:-}" != "1" ]]; then
  echo "snapshot observation not enabled" >&2
  exit 7
fi
echo "hermes_cron_snapshot_observation_result=success"
echo "hermes_cron_snapshot_timer_unit_present=true"
echo "hermes_cron_snapshot_service_unit_present=true"
echo "hermes_cron_snapshot_repo_present=true"
echo "hermes_cron_snapshot_remote_absent=true"
echo "hermes_cron_snapshot_latest_commit_epoch=1786320600"
echo "WECOM_HOME_CHANNEL=secret-group"
`
  );
  writeExecutable(
    path.relative(
      tmpRoot,
      path.join(scriptsDir, "hermes-cron-live-parity-observation-smoke.sh")
    ),
    `#!/usr/bin/env bash
set -euo pipefail
printf 'parity:%s\\n' "\${QINTOPIA_HERMES_CRON_LIVE_PARITY_OBSERVATION_ENABLE:-}" >> ${JSON.stringify(
      observationLog
    )}
if [[ "\${QINTOPIA_HERMES_CRON_LIVE_PARITY_OBSERVATION_ENABLE:-}" != "1" ]]; then
  echo "live parity observation not enabled" >&2
  exit 8
fi
echo "hermes_cron_live_parity_result=success"
echo "hermes_cron_live_parity_reviewed_count=5"
echo "hermes_cron_live_parity_live_count=5"
echo "hermes_cron_live_parity_enabled_count=0"
echo "chat_id=secret-group"
`
  );

  for (const scriptName of [
    "activate-qiwe-image-send-production.sh",
    "activate-xiaoman-daily-case-report-auto-publish-production.sh",
    "apply-qiwe-image-send-production-config.py",
    "apply-xiaoman-daily-case-report-production-config.py",
  ]) {
    writeExecutable(
      path.relative(tmpRoot, path.join(scriptsDir, scriptName)),
      `#!/usr/bin/env bash
set -euo pipefail
echo "${scriptName} must not be called by production observation" >&2
exit 99
`
    );
  }

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

  const requestFile = path.join(tmpRoot, "observation-request.json");
  fs.writeFileSync(requestFile, `${JSON.stringify(buildRequest(), null, 2)}\n`, "utf8");
  const result = spawnSync("bash", [runnerPath, "--request-file", requestFile], {
    cwd: stateDir,
    env,
    encoding: "utf8",
  });
  if (result.status !== 0) {
    throw new Error(
      `expected production observation to pass, got ${result.status}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`
    );
  }

  const deployResult = JSON.parse(
    fs.readFileSync(path.join(stateDir, "results", `${requestId}.json`), "utf8")
  );
  if (deployResult.status !== "succeeded") {
    throw new Error(`expected succeeded deploy result, got ${deployResult.status}`);
  }
  const observationCheck = deployResult.checks.find(
    (check) => check.name === "production-observation"
  );
  if (!observationCheck || observationCheck.status !== "passed") {
    throw new Error("production observation check was not recorded as passed");
  }
  const observationDetail = JSON.parse(observationCheck.detail);
  const passedTargets = observationDetail.targets.map((target) => [
    target.target,
    target.status,
    target.detail,
  ]);
  if (
    passedTargets[0][0] !== "qiwe-image-send" ||
    passedTargets[0][1] !== "passed" ||
    passedTargets[0][2] !== "qiwe_image_send_production_observation_state=disabled"
  ) {
    throw new Error(
      `unexpected QiWe observation evidence ${JSON.stringify(passedTargets[0])}`
    );
  }
  if (
    passedTargets[1][0] !== "xiaoman-daily-case-report-auto-publish" ||
    passedTargets[1][1] !== "passed" ||
    passedTargets[1][2] !==
      "xiaoman_daily_case_report_auto_publish_observation_state=disabled"
  ) {
    throw new Error(
      `unexpected daily report observation evidence ${JSON.stringify(passedTargets[1])}`
    );
  }
  if (
    passedTargets[2][0] !== "hermes-cron-snapshot" ||
    passedTargets[2][1] !== "passed" ||
    passedTargets[2][2] !==
      "hermes_cron_snapshot_observation_result=success; hermes_cron_snapshot_timer_unit_present=true; hermes_cron_snapshot_service_unit_present=true; hermes_cron_snapshot_repo_present=true; hermes_cron_snapshot_remote_absent=true; hermes_cron_snapshot_latest_commit_epoch=1786320600"
  ) {
    throw new Error(
      `unexpected snapshot observation evidence ${JSON.stringify(passedTargets[2])}`
    );
  }
  if (
    passedTargets[3][0] !== "hermes-cron-live-parity" ||
    passedTargets[3][1] !== "passed" ||
    passedTargets[3][2] !==
      "hermes_cron_live_parity_result=success; hermes_cron_live_parity_reviewed_count=5; hermes_cron_live_parity_live_count=5; hermes_cron_live_parity_enabled_count=0"
  ) {
    throw new Error(
      `unexpected live parity observation evidence ${JSON.stringify(passedTargets[3])}`
    );
  }
  if (
    passedTargets[4][0] !== "erhua-morning-brief-worker-run" ||
    passedTargets[4][1] !== "passed" ||
    passedTargets[4][2] !==
      "erhua_morning_brief_worker_run_result=success; erhua_morning_brief_worker_run_epoch=1786320600"
  ) {
    throw new Error(
      `unexpected worker-run observation evidence ${JSON.stringify(passedTargets[4])}`
    );
  }
  if (
    passedTargets[5][0] !== "xiaoman-weekly-recruitment-worker-run" ||
    passedTargets[5][1] !== "passed" ||
    passedTargets[5][2] !== "xiaoman_weekly_recruitment_worker_run_result=not_started"
  ) {
    throw new Error(
      `unexpected not-started worker-run evidence ${JSON.stringify(passedTargets[5])}`
    );
  }
  const serializedDeployResult = JSON.stringify(deployResult);
  for (const forbidden of [
    "postgres://secret@example.invalid/qintopia",
    "secret-token",
    "wx-secret-group",
    "QINTOPIA_SIDECAR_DATABASE_URL",
    "DATABASE_URL",
    "QIWE_TOKEN",
    "secret-group",
  ]) {
    if (serializedDeployResult.includes(forbidden)) {
      throw new Error(`production observation leaked ${forbidden}`);
    }
  }
  const actualLog = fs.readFileSync(observationLog, "utf8").trim();
  if (
    actualLog !==
    [
      "qiwe:auto",
      "daily:enabled",
      "daily:disabled",
      "snapshot:1",
      "parity:1",
      "worker:erhua-morning-brief-worker-run",
      "worker:xiaoman-weekly-recruitment-worker-run",
    ].join("\n")
  ) {
    throw new Error(`unexpected observation log ${JSON.stringify(actualLog)}`);
  }

  writeExecutable(
    path.relative(
      tmpRoot,
      path.join(
        scriptsDir,
        "xiaoman-daily-case-report-auto-publish-production-observation-smoke.sh"
      )
    ),
    `#!/usr/bin/env bash
set -euo pipefail
echo "daily observation ${"${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_EXPECTED_STATE:-}"} failed" >&2
echo "DATABASE_URL=postgres://failure-secret@example.invalid/qintopia" >&2
exit 42
`
  );
  const failedRequestId = "deploy-20260810T000001Z-abcdef123456";
  const failedRequestFile = path.join(tmpRoot, "failed-observation-request.json");
  fs.writeFileSync(
    failedRequestFile,
    `${JSON.stringify(
      buildRequest({
        request_id: failedRequestId,
        observation: {
          targets: ["xiaoman-daily-case-report-auto-publish"],
        },
      }),
      null,
      2
    )}\n`,
    "utf8"
  );
  const failed = spawnSync("bash", [runnerPath, "--request-file", failedRequestFile], {
    cwd: stateDir,
    env,
    encoding: "utf8",
  });
  if (failed.status !== 1) {
    throw new Error(
      `expected failed observation to exit 1, got ${failed.status}\nstdout:\n${failed.stdout}\nstderr:\n${failed.stderr}`
    );
  }
  const failedResult = JSON.parse(
    fs.readFileSync(path.join(stateDir, "results", `${failedRequestId}.json`), "utf8")
  );
  const failedCheck = failedResult.checks.find(
    (check) => check.name === "production-observation"
  );
  if (!failedCheck || failedCheck.status !== "failed") {
    throw new Error("failed production observation check was not recorded");
  }
  const failedDetail = JSON.parse(failedCheck.detail);
  const failedTarget = failedDetail.targets[0];
  if (
    failedTarget.status !== "failed" ||
    failedTarget.detail !== "exit 1: enabled_attempt=failed; disabled_attempt=failed"
  ) {
    throw new Error(
      `unexpected failed observation detail ${JSON.stringify(failedTarget)}`
    );
  }
  if (
    JSON.stringify(failedResult).includes("postgres://failure-secret@example.invalid")
  ) {
    throw new Error("failed production observation leaked raw stderr");
  }

  const workerFailedRequestId = "deploy-20260810T000003Z-abcdef123456";
  const workerFailedRequestFile = path.join(
    tmpRoot,
    "failed-worker-run-observation-request.json"
  );
  fs.writeFileSync(
    workerFailedRequestFile,
    `${JSON.stringify(
      buildRequest({
        request_id: workerFailedRequestId,
        observation: {
          targets: ["xiaoman-weekly-preview-worker-run"],
        },
      }),
      null,
      2
    )}\n`,
    "utf8"
  );
  const workerFailed = spawnSync(
    "bash",
    [runnerPath, "--request-file", workerFailedRequestFile],
    {
      cwd: stateDir,
      env,
      encoding: "utf8",
    }
  );
  if (workerFailed.status !== 1) {
    throw new Error(
      `expected failed worker-run observation to exit 1, got ${workerFailed.status}\nstdout:\n${workerFailed.stdout}\nstderr:\n${workerFailed.stderr}`
    );
  }
  const workerFailedResult = JSON.parse(
    fs.readFileSync(
      path.join(stateDir, "results", `${workerFailedRequestId}.json`),
      "utf8"
    )
  );
  const workerFailedCheck = workerFailedResult.checks.find(
    (check) => check.name === "production-observation"
  );
  if (!workerFailedCheck || workerFailedCheck.status !== "failed") {
    throw new Error("failed worker-run observation check was not recorded");
  }
  const workerFailedDetail = JSON.parse(workerFailedCheck.detail);
  const workerFailedTarget = workerFailedDetail.targets[0];
  if (
    workerFailedTarget.status !== "failed" ||
    workerFailedTarget.detail !==
      "exit 1: xiaoman_weekly_preview_worker_run_error=worker_failed"
  ) {
    throw new Error(
      `unexpected failed worker-run observation detail ${JSON.stringify(workerFailedTarget)}`
    );
  }
  if (JSON.stringify(workerFailedResult).includes("secret-token")) {
    throw new Error("failed worker-run observation leaked raw stderr");
  }

  const ordinaryRequestPath = path.join(tmpRoot, "ordinary-request.json");
  fs.writeFileSync(
    ordinaryRequestPath,
    `${JSON.stringify(
      buildRequest({
        request_id: "deploy-20260810T000002Z-abcdef123456",
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
      "observation metadata is only allowed for production-observation"
    )
  ) {
    throw new Error(
      `expected observation metadata rejection, got ${rejection.status}\nstderr:\n${rejection.stderr}`
    );
  }

  console.log("Production observation runner test passed.");
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}
