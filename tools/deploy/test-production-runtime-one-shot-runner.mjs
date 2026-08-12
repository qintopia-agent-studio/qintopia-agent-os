#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpRoot = fs.mkdtempSync(
  path.join(os.tmpdir(), "qintopia-production-runtime-one-shot-test-")
);

const signingKey = "test-signing-key";
const keyId = "production";
const sha = "113ce49141b06fc44edcee42026aee0a614ac027";
const creativeProfilePayloadSha256 =
  "9c2b0ff0d2a29d00f817cad596804e460ffb48eaf4a440604e5f81ef92b59b7a";

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

const buildRequest = (requestId, runtimeOneShot) => {
  const request = {
    schema_version: 1,
    request_id: requestId,
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
    release_scope: ["production-runtime-one-shot"],
    restart_targets: ["qintopia-system-services"],
    rollback_on_smoke_failure: false,
    dry_run: false,
    runtime_one_shot: runtimeOneShot,
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
  const oneShotLog = path.join(tmpRoot, "runtime-one-shot.log");
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
      path.join(scriptsDir, "erhua-morning-brief-timer-observation-smoke.sh")
    ),
    `#!/usr/bin/env bash
set -euo pipefail
printf 'observe-erhua:%s\\n' "\${QINTOPIA_ERHUA_MORNING_BRIEF_TIMER_EXPECTED_STATE:-}" >> ${JSON.stringify(
      oneShotLog
    )}
if [[ "\${QINTOPIA_ERHUA_MORNING_BRIEF_TIMER_OBSERVATION_ENABLE:-}" != "1" ]]; then
  exit 31
fi
if [[ "\${QINTOPIA_ERHUA_MORNING_BRIEF_TIMER_EXPECTED_STATE:-}" != "enabled" ]]; then
  exit 32
fi
`
  );
  writeExecutable(
    path.relative(
      tmpRoot,
      path.join(scriptsDir, "erhua-morning-brief-one-shot-production.sh")
    ),
    `#!/usr/bin/env bash
set -euo pipefail
if [[ "\${QINTOPIA_ERHUA_MORNING_BRIEF_ONE_SHOT:-}" != "approved-production-erhua-morning-brief-one-shot" ]]; then
  exit 41
fi
if [[ "\${QINTOPIA_ERHUA_MORNING_BRIEF_ONE_SHOT_RELEASE_SHA:-}" != ${JSON.stringify(
      sha
    )} ]]; then
  exit 42
fi
printf 'run-erhua-one-shot\\n' >> ${JSON.stringify(oneShotLog)}
echo "target_group_id=must-not-leak"
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
printf 'observe-daily:%s\\n' "\${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_EXPECTED_STATE:-}" >> ${JSON.stringify(
      oneShotLog
    )}
if [[ "\${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_OBSERVATION_ENABLE:-}" != "1" ]]; then
  exit 51
fi
if [[ "\${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_EXPECTED_STATE:-}" != "enabled" ]]; then
  exit 52
fi
`
  );
  writeExecutable(
    path.relative(
      tmpRoot,
      path.join(scriptsDir, "xiaoman-daily-case-report-auto-publish-backfill.sh")
    ),
    `#!/usr/bin/env bash
set -euo pipefail
if [[ "\${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_BACKFILL:-}" != "approved-production-xiaoman-daily-case-report-auto-publish-backfill" ]]; then
  exit 61
fi
if [[ "\${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_BACKFILL_RELEASE_SHA:-}" != ${JSON.stringify(
      sha
    )} ]]; then
  exit 62
fi
if [[ "\${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_BACKFILL_DATE:-}" != "2026-08-10" ]]; then
  exit 63
fi
printf 'run-daily-backfill:%s\\n' "\${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_BACKFILL_DATE}" >> ${JSON.stringify(
      oneShotLog
    )}
echo "QINTOPIA_SIDECAR_DATABASE_URL=must-not-leak"
`
  );
  writeExecutable(
    path.relative(
      tmpRoot,
      path.join(scriptsDir, "install-hermes-cron-snapshot-timer.sh")
    ),
    `#!/usr/bin/env bash
set -euo pipefail
if [[ "\${QINTOPIA_HERMES_CRON_SNAPSHOT:-}" != "approved-production-hermes-cron-snapshot" ]]; then
  exit 71
fi
printf 'run-hermes-cron-snapshot-install\\n' >> ${JSON.stringify(oneShotLog)}
echo "live_jobs_json=must-not-leak"
`
  );
  writeExecutable(
    path.relative(
      tmpRoot,
      path.join(scriptsDir, "apply-xiaoman-creative-profile-candidates-production.sh")
    ),
    `#!/usr/bin/env bash
set -euo pipefail
if [[ "\${QINTOPIA_XIAOMAN_CREATIVE_PROFILE_CANDIDATES_APPLY:-}" != "approved-production-xiaoman-creative-profile-candidates" ]]; then
  exit 81
fi
if [[ "\${QINTOPIA_XIAOMAN_CREATIVE_PROFILE_CANDIDATES_PAYLOAD_SHA256:-}" != ${JSON.stringify(
      creativeProfilePayloadSha256
    )} ]]; then
  exit 82
fi
printf 'run-creative-profile-apply:%s\\n' "\${QINTOPIA_XIAOMAN_CREATIVE_PROFILE_CANDIDATES_PAYLOAD_SHA256}" >> ${JSON.stringify(
      oneShotLog
    )}
echo "person_id=must-not-leak"
`
  );
  writeExecutable(
    path.relative(tmpRoot, path.join(scriptsDir, "fail-runtime-one-shot-for-test.sh")),
    `#!/usr/bin/env bash
set -euo pipefail
echo "qintopia_runtime_one_shot_safe_failure=runtime one shot fixture failed" >&2
echo "QINTOPIA_SIDECAR_DATABASE_URL=must-not-leak" >&2
exit 72
`
  );

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

  const runRequest = (requestId, runtimeOneShot) => {
    const requestFile = path.join(tmpRoot, `${requestId}.json`);
    fs.writeFileSync(
      requestFile,
      `${JSON.stringify(buildRequest(requestId, runtimeOneShot), null, 2)}\n`,
      "utf8"
    );
    return spawnSync("bash", [runnerPath, "--request-file", requestFile], {
      cwd: stateDir,
      env,
      encoding: "utf8",
    });
  };

  const dailyRequestId = "deploy-20260810T000000Z-abcdef123456";
  let result = runRequest(dailyRequestId, {
    targets: ["xiaoman-daily-case-report-auto-publish-backfill"],
    backfill_date: "2026-08-10",
    approval: "approved-production-xiaoman-daily-case-report-auto-publish-backfill",
  });
  if (result.status !== 0) {
    throw new Error(
      `daily backfill one-shot failed\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`
    );
  }
  let deployResult = JSON.parse(
    fs.readFileSync(path.join(stateDir, "results", `${dailyRequestId}.json`), "utf8")
  );
  let oneShotCheck = deployResult.checks.find(
    (check) => check.name === "production-runtime-one-shot"
  );
  if (!oneShotCheck || oneShotCheck.status !== "passed") {
    throw new Error("daily backfill one-shot check was not recorded as passed");
  }
  let detail = JSON.parse(oneShotCheck.detail);
  if (
    detail.targets[0].target !== "xiaoman-daily-case-report-auto-publish-backfill" ||
    detail.targets[0].status !== "passed" ||
    !detail.targets[0].detail.includes(
      "xiaoman_daily_case_report_backfill_date=2026-08-10"
    )
  ) {
    throw new Error(`unexpected daily backfill evidence ${oneShotCheck.detail}`);
  }
  if (oneShotCheck.detail.includes("must-not-leak")) {
    throw new Error("runtime one-shot evidence leaked raw script output");
  }

  const erhuaRequestId = "deploy-20260810T000001Z-abcdef123456";
  result = runRequest(erhuaRequestId, {
    targets: ["erhua-morning-brief"],
    approval: "approved-production-erhua-morning-brief-one-shot",
  });
  if (result.status !== 0) {
    throw new Error(
      `Erhua one-shot failed\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`
    );
  }
  deployResult = JSON.parse(
    fs.readFileSync(path.join(stateDir, "results", `${erhuaRequestId}.json`), "utf8")
  );
  oneShotCheck = deployResult.checks.find(
    (check) => check.name === "production-runtime-one-shot"
  );
  if (!oneShotCheck || oneShotCheck.status !== "passed") {
    throw new Error("Erhua one-shot check was not recorded as passed");
  }
  detail = JSON.parse(oneShotCheck.detail);
  if (
    detail.targets[0].target !== "erhua-morning-brief" ||
    detail.targets[0].status !== "passed" ||
    detail.targets[0].detail !== "erhua_morning_brief_one_shot=completed"
  ) {
    throw new Error(`unexpected Erhua one-shot evidence ${oneShotCheck.detail}`);
  }

  const snapshotRequestId = "deploy-20260810T000002Z-abcdef123456";
  result = runRequest(snapshotRequestId, {
    targets: ["hermes-cron-snapshot-install"],
    approval: "approved-production-hermes-cron-snapshot",
  });
  if (result.status !== 0) {
    throw new Error(
      `Hermes cron snapshot install one-shot failed\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`
    );
  }
  deployResult = JSON.parse(
    fs.readFileSync(path.join(stateDir, "results", `${snapshotRequestId}.json`), "utf8")
  );
  oneShotCheck = deployResult.checks.find(
    (check) => check.name === "production-runtime-one-shot"
  );
  if (!oneShotCheck || oneShotCheck.status !== "passed") {
    throw new Error("Hermes cron snapshot install check was not recorded as passed");
  }
  detail = JSON.parse(oneShotCheck.detail);
  if (
    detail.targets[0].target !== "hermes-cron-snapshot-install" ||
    detail.targets[0].status !== "passed" ||
    detail.targets[0].detail !== "hermes_cron_snapshot_install=completed"
  ) {
    throw new Error(
      `unexpected Hermes cron snapshot install evidence ${oneShotCheck.detail}`
    );
  }
  if (oneShotCheck.detail.includes("must-not-leak")) {
    throw new Error("snapshot install evidence leaked raw script output");
  }

  const creativeProfileRequestId = "deploy-20260810T000003Z-abcdef123456";
  result = runRequest(creativeProfileRequestId, {
    targets: ["xiaoman-creative-profile-candidates-apply"],
    approval: "approved-production-xiaoman-creative-profile-candidates",
    payload_sha256: creativeProfilePayloadSha256,
  });
  if (result.status !== 0) {
    throw new Error(
      `creative-profile candidates apply one-shot failed\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`
    );
  }
  deployResult = JSON.parse(
    fs.readFileSync(
      path.join(stateDir, "results", `${creativeProfileRequestId}.json`),
      "utf8"
    )
  );
  oneShotCheck = deployResult.checks.find(
    (check) => check.name === "production-runtime-one-shot"
  );
  if (!oneShotCheck || oneShotCheck.status !== "passed") {
    throw new Error(
      "creative-profile candidates apply check was not recorded as passed"
    );
  }
  detail = JSON.parse(oneShotCheck.detail);
  if (
    detail.targets[0].target !== "xiaoman-creative-profile-candidates-apply" ||
    detail.targets[0].status !== "passed" ||
    detail.targets[0].detail !==
      `xiaoman_creative_profile_candidates_apply=completed; payload_sha256=${creativeProfilePayloadSha256}`
  ) {
    throw new Error(
      `unexpected creative-profile candidates apply evidence ${oneShotCheck.detail}`
    );
  }
  if (oneShotCheck.detail.includes("must-not-leak")) {
    throw new Error("creative-profile apply evidence leaked raw script output");
  }

  fs.copyFileSync(
    path.join(scriptsDir, "fail-runtime-one-shot-for-test.sh"),
    path.join(scriptsDir, "install-hermes-cron-snapshot-timer.sh")
  );
  const failedSnapshotRequestId = "deploy-20260810T000004Z-abcdef123456";
  result = runRequest(failedSnapshotRequestId, {
    targets: ["hermes-cron-snapshot-install"],
    approval: "approved-production-hermes-cron-snapshot",
  });
  if (result.status === 0) {
    throw new Error("failing snapshot install one-shot unexpectedly passed");
  }
  deployResult = JSON.parse(
    fs.readFileSync(
      path.join(stateDir, "results", `${failedSnapshotRequestId}.json`),
      "utf8"
    )
  );
  oneShotCheck = deployResult.checks.find(
    (check) => check.name === "production-runtime-one-shot"
  );
  detail = JSON.parse(oneShotCheck.detail);
  if (
    detail.targets[0].target !== "hermes-cron-snapshot-install" ||
    detail.targets[0].status !== "failed" ||
    detail.targets[0].detail !== "exit 72: runtime one shot fixture failed"
  ) {
    throw new Error(
      `unexpected failing Hermes cron snapshot install evidence ${oneShotCheck.detail}`
    );
  }
  if (oneShotCheck.detail.includes("must-not-leak")) {
    throw new Error("failed snapshot install evidence leaked raw script output");
  }

  const commandLog = fs.readFileSync(oneShotLog, "utf8");
  for (const expected of [
    "observe-daily:enabled",
    "run-daily-backfill:2026-08-10",
    "observe-erhua:enabled",
    "run-erhua-one-shot",
    "run-hermes-cron-snapshot-install",
  ]) {
    if (!commandLog.includes(expected)) {
      throw new Error(`missing one-shot command log entry: ${expected}`);
    }
  }

  const invalidRequest = runRequest("deploy-20260810T000005Z-abcdef123456", {
    targets: ["xiaoman-daily-case-report-auto-publish-backfill"],
    approval: "approved-production-xiaoman-daily-case-report-auto-publish-backfill",
  });
  if (invalidRequest.status === 0) {
    throw new Error("runtime one-shot accepted Xiaoman backfill without a date");
  }
  if (!invalidRequest.stderr.includes("runtime_one_shot.backfill_date")) {
    throw new Error(
      `runtime one-shot date rejection was not explicit\n${invalidRequest.stderr}`
    );
  }

  const invalidCreativeProfileRequest = runRequest(
    "deploy-20260810T000006Z-abcdef123456",
    {
      targets: ["xiaoman-creative-profile-candidates-apply"],
      approval: "approved-production-xiaoman-creative-profile-candidates",
    }
  );
  if (invalidCreativeProfileRequest.status === 0) {
    throw new Error(
      "runtime one-shot accepted Xiaoman creative-profile apply without payload SHA-256"
    );
  }
  if (
    !invalidCreativeProfileRequest.stderr.includes("runtime_one_shot.payload_sha256")
  ) {
    throw new Error(
      `runtime one-shot payload hash rejection was not explicit\n${invalidCreativeProfileRequest.stderr}`
    );
  }

  console.log("Production runtime one-shot runner test passed.");
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}
