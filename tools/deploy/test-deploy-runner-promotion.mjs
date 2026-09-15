#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "qintopia-runner-test-"));
const promotionEventLog = path.join(tmpRoot, "promotion-events.log");

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
const originalPreviousSha = "fedcba9876543210fedcba9876543210fedcba98";
const createdAt = new Date().toISOString();
const expiresAt = new Date(Date.parse(createdAt) + 60 * 60 * 1000).toISOString();

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
  const runnerPath = writeExecutable(
    "deploy/runner/qintopia-agent-os-deploy-runner",
    fs.readFileSync(
      path.join(repoRoot, "deploy/runner/qintopia-agent-os-deploy-runner"),
      "utf8"
    )
  );
  const stateDir = path.join(tmpRoot, "state");
  const releaseRoot = path.join(tmpRoot, "releases");
  fs.mkdirSync(stateDir, { recursive: true });
  fs.mkdirSync(releaseRoot, { recursive: true });

  const buildRequest = (runtimeArtifactProfile = "huabaosi-production") => ({
    schema_version: 1,
    request_id: requestId,
    environment: "production",
    repository: "qintopia-agent-studio/qintopia-agent-os",
    requested_by: "codex",
    created_at: createdAt,
    expires_at: expiresAt,
    commit_sha: sha,
    runtime_sha: sha,
    runtime_artifact_profile: runtimeArtifactProfile,
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
  });
  const request = buildRequest();
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
    "deploy/runner/quiesce-space-automation-runtime.sh",
    `#!/usr/bin/env bash
set -euo pipefail
printf 'quiesce\n' >>"${promotionEventLog}"
`
  );
  writeExecutable(
    "deploy/runner/promote-release.sh",
    `#!/usr/bin/env bash
printf 'promote\n' >>"${promotionEventLog}"
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

  const result = spawnSync("bash", [runnerPath, "--request-file", requestFile], {
    cwd: stateDir,
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
  });

  if (result.status !== 42) {
    throw new Error(
      `expected runner to return promote failure status 42, got ${result.status}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`
    );
  }
  if (result.stderr.includes("smoke must not run")) {
    throw new Error("runner executed smoke after promote failure");
  }
  const failedPromotionEvents = fs.readFileSync(promotionEventLog, "utf8").trim();
  if (failedPromotionEvents !== "quiesce\npromote") {
    throw new Error(
      `runner must quiesce Space runtime immediately before promotion, got ${failedPromotionEvents}`
    );
  }

  const resultPath = path.join(stateDir, "results", `${requestId}.json`);
  if (!fs.existsSync(resultPath)) {
    throw new Error("runner did not write failed result");
  }
  const deployResult = JSON.parse(fs.readFileSync(resultPath, "utf8"));
  if (deployResult.status !== "failed") {
    throw new Error(`expected failed result, got ${deployResult.status}`);
  }
  if (
    deployResult.commit_sha !== sha ||
    deployResult.runtime_sha !== sha ||
    deployResult.runtime_artifact_profile !== "huabaosi-production" ||
    deployResult.deploy_bundle_sha !== sha
  ) {
    throw new Error("failed result did not retain deploy identity fields");
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
  fs.rmSync(promotionEventLog, { force: true });

  writeExecutable(
    "deploy/runner/quiesce-space-automation-runtime.sh",
    `#!/usr/bin/env bash
set -euo pipefail
printf 'quiesce\n' >>"${promotionEventLog}"
exit 61
`
  );
  writeExecutable(
    "deploy/runner/promote-release.sh",
    `#!/usr/bin/env bash
printf 'promote\n' >>"${promotionEventLog}"
exit 42
`
  );

  const quiesceFailureResult = spawnSync(
    "bash",
    [runnerPath, "--request-file", requestFile],
    {
      cwd: stateDir,
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

  if (quiesceFailureResult.status !== 61) {
    throw new Error(
      `expected runner to return quiesce failure status 61, got ${quiesceFailureResult.status}\nstdout:\n${quiesceFailureResult.stdout}\nstderr:\n${quiesceFailureResult.stderr}`
    );
  }
  const quiesceFailureEvents = fs.readFileSync(promotionEventLog, "utf8").trim();
  if (quiesceFailureEvents !== "quiesce") {
    throw new Error(
      `quiesce failure must prevent release promotion, got ${quiesceFailureEvents}`
    );
  }
  const quiesceFailedDeployResult = JSON.parse(fs.readFileSync(resultPath, "utf8"));
  if (
    quiesceFailedDeployResult.status !== "failed" ||
    quiesceFailedDeployResult.error !==
      "deployment failed during quiesce-space-automation-runtime (exit 61)" ||
    quiesceFailedDeployResult.current_target
  ) {
    throw new Error("quiesce failure result must remain pre-promotion and fail closed");
  }
  const quiesceFailureDetail = JSON.parse(quiesceFailedDeployResult.checks[0].detail);
  if (
    quiesceFailureDetail.failure_stage !== "quiesce-space-automation-runtime" ||
    quiesceFailureDetail.exit_status !== 61 ||
    quiesceFailureDetail.promoted_current !== false
  ) {
    throw new Error(
      `expected pre-promotion quiesce failure detail, got ${quiesceFailedDeployResult.checks[0].detail}`
    );
  }

  fs.rmSync(stateDir, { recursive: true, force: true });
  fs.rmSync(releaseRoot, { recursive: true, force: true });
  fs.mkdirSync(path.join(stateDir, "results"), { recursive: true });
  fs.mkdirSync(releaseRoot, { recursive: true });
  fs.mkdirSync(path.join(releaseRoot, previousSha), { recursive: true });
  fs.mkdirSync(path.join(releaseRoot, originalPreviousSha), { recursive: true });
  fs.symlinkSync(
    path.join(releaseRoot, previousSha),
    path.join(releaseRoot, "current")
  );
  fs.symlinkSync(
    path.join(releaseRoot, originalPreviousSha),
    path.join(releaseRoot, "previous")
  );
  fs.rmSync(promotionEventLog, { force: true });

  writeExecutable(
    "deploy/runner/quiesce-space-automation-runtime.sh",
    `#!/usr/bin/env bash
set -euo pipefail
printf 'quiesce\n' >>"${promotionEventLog}"
`
  );

  writeExecutable(
    "deploy/runner/promote-release.sh",
    `#!/usr/bin/env bash
set -euo pipefail
printf 'promote-partial\n' >>"${promotionEventLog}"
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
ln -sfn "\${release_root}/${previousSha}" "\${release_root}/previous"
exit 62
`
  );
  writeExecutable(
    "deploy/runner/install-release-systemd-units.sh",
    `#!/usr/bin/env bash
echo "install must not run after partial promotion failure" >&2
exit 71
`
  );
  writeExecutable(
    "deploy/runner/rollback-release.sh",
    `#!/usr/bin/env bash
echo "full rollback must not run when current did not change" >&2
exit 72
`
  );

  const runPartialPromotion = () =>
    spawnSync("bash", [runnerPath, "--request-file", requestFile], {
      cwd: stateDir,
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
    });

  const partialPromotionResult = runPartialPromotion();
  if (partialPromotionResult.status !== 62) {
    throw new Error(
      `expected partial promotion failure status 62, got ${partialPromotionResult.status}\nstdout:\n${partialPromotionResult.stdout}\nstderr:\n${partialPromotionResult.stderr}`
    );
  }
  if (
    fs.realpathSync(path.join(releaseRoot, "current")) !==
      fs.realpathSync(path.join(releaseRoot, previousSha)) ||
    fs.realpathSync(path.join(releaseRoot, "previous")) !==
      fs.realpathSync(path.join(releaseRoot, originalPreviousSha))
  ) {
    throw new Error("partial promotion failure did not restore both lineage pointers");
  }
  const partialDeployResult = JSON.parse(fs.readFileSync(resultPath, "utf8"));
  const partialDetail = JSON.parse(partialDeployResult.checks[0].detail);
  if (
    partialDeployResult.status !== "rolled_back" ||
    partialDeployResult.rollback.status !== "succeeded" ||
    partialDetail.promoted_current !== false
  ) {
    throw new Error(
      "partial promotion restore was not reported as an exact pointer rollback"
    );
  }

  fs.rmSync(resultPath, { force: true });
  fs.rmSync(promotionEventLog, { force: true });
  fs.unlinkSync(path.join(releaseRoot, "previous"));

  const partialPromotionWithoutPreviousResult = runPartialPromotion();
  if (partialPromotionWithoutPreviousResult.status !== 62) {
    throw new Error(
      `expected partial promotion without previous status 62, got ${partialPromotionWithoutPreviousResult.status}\nstdout:\n${partialPromotionWithoutPreviousResult.stdout}\nstderr:\n${partialPromotionWithoutPreviousResult.stderr}`
    );
  }
  if (
    fs.realpathSync(path.join(releaseRoot, "current")) !==
      fs.realpathSync(path.join(releaseRoot, previousSha)) ||
    fs.lstatSync(path.join(releaseRoot, "previous"), { throwIfNoEntry: false }) !==
      undefined
  ) {
    throw new Error(
      "partial promotion did not restore an originally absent previous pointer"
    );
  }
  const partialWithoutPreviousDeployResult = JSON.parse(
    fs.readFileSync(resultPath, "utf8")
  );
  if (
    partialWithoutPreviousDeployResult.status !== "rolled_back" ||
    partialWithoutPreviousDeployResult.rollback.status !== "succeeded"
  ) {
    throw new Error(
      "absent previous pointer restoration was not reported as succeeded"
    );
  }

  fs.rmSync(resultPath, { force: true });
  fs.rmSync(promotionEventLog, { force: true });
  fs.symlinkSync(
    path.join(releaseRoot, originalPreviousSha),
    path.join(releaseRoot, "previous")
  );

  writeExecutable(
    "deploy/runner/promote-release.sh",
    `#!/usr/bin/env bash
set -euo pipefail
printf 'promote\n' >>"${promotionEventLog}"
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
mkdir -p "\${release_root}/${sha}/deploy/runner"
cat >"\${release_root}/${sha}/deploy/runner/install-release-systemd-units.sh" <<'INSTALL'
#!/usr/bin/env bash
echo "simulated release-local systemd install failure" >&2
exit 55
INSTALL
chmod 0755 "\${release_root}/${sha}/deploy/runner/install-release-systemd-units.sh"
cat >"\${release_root}/${sha}/deploy/runner/smoke-release.sh" <<'SMOKE'
#!/usr/bin/env bash
exit 0
SMOKE
chmod 0755 "\${release_root}/${sha}/deploy/runner/smoke-release.sh"
ln -sfn "\${release_root}/${previousSha}" "\${release_root}/previous"
ln -sfn "\${release_root}/${sha}" "\${release_root}/current"
`
  );
  writeExecutable(
    "deploy/runner/install-release-systemd-units.sh",
    `#!/usr/bin/env bash
echo "stale runner installer must not be used" >&2
exit 64
`
  );
  writeExecutable(
    "deploy/runner/rollback-release.sh",
    `#!/usr/bin/env bash
set -euo pipefail
release_root=""
expected_current_sha=""
expected_previous_sha=""
restore_previous_sha=""
restore_previous_absent=false
while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --release-root)
      release_root="\${2:-}"
      shift 2
      ;;
    --expected-current-sha)
      expected_current_sha="\${2:-}"
      shift 2
      ;;
    --expected-previous-sha)
      expected_previous_sha="\${2:-}"
      shift 2
      ;;
    --restore-previous-sha)
      restore_previous_sha="\${2:-}"
      shift 2
      ;;
    --restore-previous-absent)
      restore_previous_absent=true
      shift
      ;;
    *)
      shift
      ;;
  esac
done
if [[ "$expected_current_sha" != "${sha}" || "$expected_previous_sha" != "${previousSha}" ]]; then
  echo "runner did not bind automatic restore to the captured release lineage" >&2
  exit 72
fi
target="$(readlink -f "\${release_root}/previous")"
ln -sfn "$target" "\${release_root}/current"
if [[ "$restore_previous_sha" == "${originalPreviousSha}" && "$restore_previous_absent" == "false" ]]; then
  ln -sfn "\${release_root}/${originalPreviousSha}" "\${release_root}/previous"
elif [[ -z "$restore_previous_sha" && "$restore_previous_absent" == "true" ]]; then
  rm -f "\${release_root}/previous"
else
  echo "runner did not select exactly one captured previous restore mode" >&2
  exit 73
fi
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
    [runnerPath, "--request-file", requestFile],
    {
      cwd: stateDir,
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
  if (
    fs.realpathSync(path.join(releaseRoot, "current")) !==
      fs.realpathSync(path.join(releaseRoot, previousSha)) ||
    fs.realpathSync(path.join(releaseRoot, "previous")) !==
      fs.realpathSync(path.join(releaseRoot, originalPreviousSha))
  ) {
    throw new Error("install failure did not restore the original release lineage");
  }
  const promotedEvents = fs.readFileSync(promotionEventLog, "utf8").trim();
  if (promotedEvents !== "quiesce\npromote") {
    throw new Error(
      `runner must quiesce before a successful promotion, got ${promotedEvents}`
    );
  }

  const promotedDeployResult = JSON.parse(fs.readFileSync(resultPath, "utf8"));
  if (promotedDeployResult.status !== "rolled_back") {
    throw new Error(`expected rolled_back result, got ${promotedDeployResult.status}`);
  }
  if (
    promotedDeployResult.commit_sha !== sha ||
    promotedDeployResult.runtime_sha !== sha ||
    promotedDeployResult.runtime_artifact_profile !== "huabaosi-production" ||
    promotedDeployResult.deploy_bundle_sha !== sha
  ) {
    throw new Error("rolled_back result did not retain deploy identity fields");
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

  fs.rmSync(resultPath, { force: true });
  fs.rmSync(promotionEventLog, { force: true });
  fs.rmSync(path.join(releaseRoot, "current"), { force: true });
  fs.rmSync(path.join(releaseRoot, "previous"), { force: true });
  fs.symlinkSync(
    path.join(releaseRoot, previousSha),
    path.join(releaseRoot, "current")
  );
  writeExecutable(
    "deploy/runner/rollback-release.sh",
    `#!/usr/bin/env bash
set -euo pipefail
release_root=""
expected_current_sha=""
expected_previous_sha=""
restore_previous_sha=""
restore_previous_absent=false
while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --release-root)
      release_root="\${2:-}"
      shift 2
      ;;
    --expected-current-sha)
      expected_current_sha="\${2:-}"
      shift 2
      ;;
    --expected-previous-sha)
      expected_previous_sha="\${2:-}"
      shift 2
      ;;
    --restore-previous-sha)
      restore_previous_sha="\${2:-}"
      shift 2
      ;;
    --restore-previous-absent)
      restore_previous_absent=true
      shift
      ;;
    *)
      shift
      ;;
  esac
done
if [[ "$expected_current_sha" != "${sha}" || "$expected_previous_sha" != "${previousSha}" || -n "$restore_previous_sha" || "$restore_previous_absent" != "true" ]]; then
  echo "runner did not request restoration of an absent previous pointer" >&2
  exit 73
fi
target="$(readlink -f "\${release_root}/previous")"
ln -sfn "$target" "\${release_root}/current"
rm -f "\${release_root}/previous"
`
  );

  const promotedWithoutPreviousResult = spawnSync(
    "bash",
    [runnerPath, "--request-file", requestFile],
    {
      cwd: stateDir,
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
  if (promotedWithoutPreviousResult.status !== 55) {
    throw new Error(
      `expected post-promotion failure without previous status 55, got ${promotedWithoutPreviousResult.status}\nstdout:\n${promotedWithoutPreviousResult.stdout}\nstderr:\n${promotedWithoutPreviousResult.stderr}`
    );
  }
  if (
    fs.realpathSync(path.join(releaseRoot, "current")) !==
      fs.realpathSync(path.join(releaseRoot, previousSha)) ||
    fs.existsSync(path.join(releaseRoot, "previous"))
  ) {
    throw new Error("post-promotion failure did not restore absent previous lineage");
  }
  const promotedWithoutPreviousDeployResult = JSON.parse(
    fs.readFileSync(resultPath, "utf8")
  );
  if (
    promotedWithoutPreviousDeployResult.status !== "rolled_back" ||
    promotedWithoutPreviousDeployResult.rollback.status !== "succeeded" ||
    promotedWithoutPreviousDeployResult.previous_sha !== ""
  ) {
    throw new Error(
      "post-promotion absent-previous rollback was not reported succeeded"
    );
  }

  const invalidLineageRequest = buildRequest();
  invalidLineageRequest.release_rollback = {
    expected_current_sha: previousSha,
    expected_previous_sha: sha,
  };
  invalidLineageRequest.signature = {
    ...signatureMetadata,
    value: signRequest(invalidLineageRequest, signatureMetadata),
  };
  fs.writeFileSync(
    requestFile,
    `${JSON.stringify(invalidLineageRequest, null, 2)}\n`,
    "utf8"
  );
  fs.rmSync(resultPath, { force: true });
  fs.rmSync(promotionEventLog, { force: true });
  const invalidLineageResult = spawnSync(
    "bash",
    [runnerPath, "--request-file", requestFile],
    {
      cwd: stateDir,
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
  if (invalidLineageResult.status !== 1) {
    throw new Error(
      `expected signed non-previous rollback to fail, got ${invalidLineageResult.status}\nstdout:\n${invalidLineageResult.stdout}\nstderr:\n${invalidLineageResult.stderr}`
    );
  }
  if (fs.existsSync(promotionEventLog)) {
    throw new Error("invalid rollback lineage reached quiesce or promotion");
  }
  const invalidLineageDeployResult = JSON.parse(fs.readFileSync(resultPath, "utf8"));
  const invalidLineageDetail = JSON.parse(invalidLineageDeployResult.checks[0].detail);
  if (
    invalidLineageDeployResult.status !== "failed" ||
    invalidLineageDetail.failure_stage !== "validate-release-rollback-lineage" ||
    invalidLineageDetail.promoted_current !== false
  ) {
    throw new Error("invalid rollback lineage did not fail before promotion");
  }

  fs.rmSync(stateDir, { recursive: true, force: true });
  fs.rmSync(releaseRoot, { recursive: true, force: true });
  fs.mkdirSync(path.join(stateDir, "results"), { recursive: true });
  fs.mkdirSync(releaseRoot, { recursive: true });
  fs.rmSync(promotionEventLog, { force: true });
  fs.mkdirSync(path.join(releaseRoot, previousSha), { recursive: true });
  fs.mkdirSync(path.join(releaseRoot, originalPreviousSha), { recursive: true });
  fs.symlinkSync(
    path.join(releaseRoot, previousSha),
    path.join(releaseRoot, "current")
  );
  fs.symlinkSync(
    path.join(releaseRoot, originalPreviousSha),
    path.join(releaseRoot, "previous")
  );
  fs.writeFileSync(requestFile, `${JSON.stringify(request, null, 2)}\n`, "utf8");

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
mkdir -p "\${release_root}/${sha}/deploy/runner"
cat >"\${release_root}/${sha}/deploy/runner/install-release-systemd-units.sh" <<'INSTALL'
#!/usr/bin/env bash
exit 0
INSTALL
chmod 0755 "\${release_root}/${sha}/deploy/runner/install-release-systemd-units.sh"
cat >"\${release_root}/${sha}/deploy/runner/smoke-release.sh" <<'SMOKE'
#!/usr/bin/env bash
echo "qintopia_smoke_release_safe_failure=target=qintopia-system-services;phase=is-active;subject=qintopia-agentos-daily-digest-publisher.service" >&2
exit 77
SMOKE
chmod 0755 "\${release_root}/${sha}/deploy/runner/smoke-release.sh"
ln -sfn "\${release_root}/${previousSha}" "\${release_root}/previous"
ln -sfn "\${release_root}/${sha}" "\${release_root}/current"
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

  const smokeFailureResult = spawnSync(
    "bash",
    [runnerPath, "--request-file", requestFile],
    {
      cwd: stateDir,
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
  if (smokeFailureResult.status !== 77) {
    throw new Error(
      `expected runner to return smoke failure status 77, got ${smokeFailureResult.status}\nstdout:\n${smokeFailureResult.stdout}\nstderr:\n${smokeFailureResult.stderr}`
    );
  }
  const smokeFailureDeployResult = JSON.parse(fs.readFileSync(resultPath, "utf8"));
  if (smokeFailureDeployResult.status !== "rolled_back") {
    throw new Error(
      `expected smoke failure rolled_back result, got ${smokeFailureDeployResult.status}`
    );
  }
  const smokeFailureDetail = JSON.parse(smokeFailureDeployResult.checks[0].detail);
  if (
    smokeFailureDetail.failure_stage !== "smoke-release" ||
    smokeFailureDetail.exit_status !== 77 ||
    smokeFailureDetail.safe_failure !==
      "target=qintopia-system-services;phase=is-active;subject=qintopia-agentos-daily-digest-publisher.service"
  ) {
    throw new Error(
      `expected safe smoke failure detail, got ${smokeFailureDeployResult.checks[0].detail}`
    );
  }

  fs.rmSync(stateDir, { recursive: true, force: true });
  fs.rmSync(releaseRoot, { recursive: true, force: true });
  fs.mkdirSync(path.join(stateDir, "results"), { recursive: true });
  fs.mkdirSync(releaseRoot, { recursive: true });

  const qiweRequest = buildRequest("qiwe-production");
  qiweRequest.signature = {
    ...signatureMetadata,
    value: signRequest(qiweRequest, signatureMetadata),
  };
  fs.writeFileSync(requestFile, `${JSON.stringify(qiweRequest, null, 2)}\n`, "utf8");

  writeExecutable(
    "deploy/runner/promote-release.sh",
    `#!/usr/bin/env bash
set -euo pipefail
request_file=""
release_root=""
while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --request-file)
      request_file="\${2:-}"
      shift 2
      ;;
    --release-root)
      release_root="\${2:-}"
      shift 2
      ;;
    *)
      shift
      ;;
  esac
done
mkdir -p "\${release_root}/${sha}"
python3 - "$request_file" "\${release_root}/${sha}/manifest.json" <<'PY'
import json
import sys
with open(sys.argv[1], encoding="utf-8") as fh:
    request = json.load(fh)
with open(sys.argv[2], "w", encoding="utf-8") as fh:
    json.dump(
        {"runtime_artifact_profile": request["runtime_artifact_profile"]},
        fh,
        ensure_ascii=False,
        indent=2,
    )
    fh.write("\\n")
PY
mkdir -p "\${release_root}/${sha}/deploy/runner"
cat >"\${release_root}/${sha}/deploy/runner/install-release-systemd-units.sh" <<'INSTALL'
#!/usr/bin/env bash
echo "release-local installer reached" >"\${QINTOPIA_RELEASE_LOCAL_INSTALL_MARKER:?}"
exit 0
INSTALL
chmod 0755 "\${release_root}/${sha}/deploy/runner/install-release-systemd-units.sh"
cat >"\${release_root}/${sha}/deploy/runner/smoke-release.sh" <<'SMOKE'
#!/usr/bin/env bash
echo "release-local smoke reached" >"\${QINTOPIA_RELEASE_LOCAL_SMOKE_MARKER:?}"
exit 0
SMOKE
chmod 0755 "\${release_root}/${sha}/deploy/runner/smoke-release.sh"
ln -sfn "\${release_root}/${sha}" "\${release_root}/current"
`
  );
  writeExecutable(
    "deploy/runner/install-release-systemd-units.sh",
    `#!/usr/bin/env bash
echo "stale runner installer must not be used" >&2
exit 64
`
  );
  writeExecutable(
    "deploy/runner/rollback-release.sh",
    `#!/usr/bin/env bash
exit 0
`
  );
  writeExecutable(
    "deploy/runner/smoke-release.sh",
    `#!/usr/bin/env bash
echo "stale runner smoke must not be used" >&2
exit 64
`
  );
  const releaseLocalInstallMarker = path.join(tmpRoot, "release-local-install.marker");
  const releaseLocalSmokeMarker = path.join(tmpRoot, "release-local-smoke.marker");

  const qiweResult = spawnSync("bash", [runnerPath, "--request-file", requestFile], {
    cwd: stateDir,
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
      QINTOPIA_RELEASE_LOCAL_INSTALL_MARKER: releaseLocalInstallMarker,
      QINTOPIA_RELEASE_LOCAL_SMOKE_MARKER: releaseLocalSmokeMarker,
    },
    encoding: "utf8",
  });

  if (qiweResult.status !== 0) {
    throw new Error(
      `expected qiwe runner success, got ${qiweResult.status}\nstdout:\n${qiweResult.stdout}\nstderr:\n${qiweResult.stderr}`
    );
  }

  const qiweDeployResult = JSON.parse(fs.readFileSync(resultPath, "utf8"));
  if (qiweDeployResult.status !== "succeeded") {
    throw new Error(`expected succeeded result, got ${qiweDeployResult.status}`);
  }
  if (!fs.existsSync(releaseLocalInstallMarker)) {
    throw new Error("runner did not execute release-local installer after promotion");
  }
  if (!fs.existsSync(releaseLocalSmokeMarker)) {
    throw new Error("runner did not execute release-local smoke after promotion");
  }
  if (
    qiweDeployResult.commit_sha !== sha ||
    qiweDeployResult.runtime_sha !== sha ||
    qiweDeployResult.runtime_artifact_profile !== "qiwe-production" ||
    qiweDeployResult.deploy_bundle_sha !== sha
  ) {
    throw new Error("qiwe succeeded result did not retain deploy identity fields");
  }
  const qiweManifest = JSON.parse(
    fs.readFileSync(path.join(releaseRoot, sha, "manifest.json"), "utf8")
  );
  if (qiweManifest.runtime_artifact_profile !== "qiwe-production") {
    throw new Error(
      "runner success path did not preserve qiwe runtime_artifact_profile"
    );
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Deploy runner promotion failure behavior test passed.");
