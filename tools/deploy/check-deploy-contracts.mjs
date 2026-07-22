#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import YAML from "yaml";

const repoRoot = process.cwd();
const errors = [];

const packages = {
  "deploy/manifests": ["release-manifest.template.yaml", "commit SHA", "artifact SHA"],
  "deploy/rollback": ["rollback", "current", "previous"],
  "deploy/runner": ["deploy request", "release/current", "production environment"],
  "deploy/smoke": ["smoke", "profile", "MCP"],
};

const exists = (relativePath) => fs.existsSync(path.join(repoRoot, relativePath));
const readText = (relativePath) =>
  fs.readFileSync(path.join(repoRoot, relativePath), "utf8");
const addError = (message) => errors.push(message);
const requireFragment = (relativePath, text, fragment) => {
  if (!text.includes(fragment)) {
    addError(`${relativePath}: must include ${fragment}`);
  }
};
const forbidFragment = (relativePath, text, fragment) => {
  if (text.includes(fragment)) {
    addError(`${relativePath}: must not include ${fragment}`);
  }
};

const sidecarAgentsPath = "runtime/sidecar/AGENTS.md";
if (!exists(sidecarAgentsPath)) {
  addError(`${sidecarAgentsPath}: missing sidecar agent rules`);
} else {
  const sidecarAgents = readText(sidecarAgentsPath);
  for (const fragment of [
    "Production sidecar artifacts compile exactly `huabaosi-production-adapter`,",
    "`huabaosi-feishu-mirror-adapter`, and `qiwe-production-adapter`",
    "mixed staging/production builds, and all-features production artifacts remain",
    "QiWe",
    "production apply must still fail before Postgres, callback stdin, or network access",
    "unless the exact owner phrase, production database hash, Feishu delivery config",
    "only the explicit owner activation scripts may enable external timers",
    "The QiWe upload worker and callback processor may compile live helpers only through",
    "`qiwe-staging-adapter` or `qiwe-production-adapter`",
  ]) {
    requireFragment(sidecarAgentsPath, sidecarAgents, fragment);
  }
  for (const fragment of [
    "QiWe live adapters, staging adapters, and",
    "callback processor may compile live helpers for staging",
    "Default and production builds must fail apply before",
  ]) {
    forbidFragment(sidecarAgentsPath, sidecarAgents, fragment);
  }
}

const qiweImageSendPlanPath = "docs/plans/active/xiaoman-qiwe-image-send.md";
if (!exists(qiweImageSendPlanPath)) {
  addError(`${qiweImageSendPlanPath}: missing Xiaoman QiWe image send plan`);
} else {
  const plan = readText(qiweImageSendPlanPath);
  for (const fragment of [
    "Production release artifacts record exactly",
    "`huabaosi-production-adapter`, `huabaosi-feishu-mirror-adapter`, and",
    "`qiwe-production-adapter`, and still reject staging approval, staging databases",
    "the production gate and never falls back to staging approval",
  ]) {
    requireFragment(qiweImageSendPlanPath, plan, fragment);
  }
  for (const fragment of [
    "Production release artifacts must not include any QiWe live adapter",
    "Production release artifacts continue to record exactly `huabaosi-production-adapter` and `huabaosi-feishu-mirror-adapter`",
    "default and production binaries fail apply before Postgres or network access",
  ]) {
    forbidFragment(qiweImageSendPlanPath, plan, fragment);
  }
}

const qiweImageSendAdapterWorkerPlanPath =
  "docs/plans/active/qiwe-image-send-adapter-worker.md";
if (!exists(qiweImageSendAdapterWorkerPlanPath)) {
  addError(
    `${qiweImageSendAdapterWorkerPlanPath}: missing QiWe image send adapter worker plan`
  );
} else {
  const plan = readText(qiweImageSendAdapterWorkerPlanPath);
  for (const fragment of [
    "`cargo_features: [huabaosi-production-adapter, huabaosi-feishu-mirror-adapter,",
    "qiwe-production-adapter]`",
    "Production apply must use the production owner phrase",
    "back to staging gates",
  ]) {
    requireFragment(qiweImageSendAdapterWorkerPlanPath, plan, fragment);
  }
  for (const fragment of [
    "compile a QiWe live adapter into the production artifact",
    "`cargo_features: [huabaosi-production-adapter, huabaosi-feishu-mirror-adapter]`",
    "production units absent",
  ]) {
    forbidFragment(qiweImageSendAdapterWorkerPlanPath, plan, fragment);
  }
}

const xiaomanFeishuQiweBoundaryPath =
  "docs/plans/active/xiaoman-feishu-qiwe-delivery-boundary.md";
if (!exists(xiaomanFeishuQiweBoundaryPath)) {
  addError(
    `${xiaomanFeishuQiweBoundaryPath}: missing Xiaoman Feishu-to-QiWe delivery boundary`
  );
} else {
  const plan = readText(xiaomanFeishuQiweBoundaryPath);
  for (const fragment of [
    "Staging requires",
    "`huabaosi-staging-adapter` plus `qiwe-staging-adapter`; production requires",
    "`huabaosi-feishu-mirror-adapter` plus `qiwe-production-adapter` with production",
    "owner/database/Feishu delivery gates",
    "Default, Huabaosi-only, and QiWe-only builds must continue to reject it",
  ]) {
    requireFragment(xiaomanFeishuQiweBoundaryPath, plan, fragment);
  }
  for (const fragment of [
    "production builds continue to reject this route",
    "Default, production, Huabaosi-only, and QiWe-only builds must continue to",
    "reviewed QiWe staging live adapter may claim",
  ]) {
    forbidFragment(xiaomanFeishuQiweBoundaryPath, plan, fragment);
  }
}

const currentRoadmapPath = "docs/plans/active/current-roadmap.md";
if (!exists(currentRoadmapPath)) {
  addError(`${currentRoadmapPath}: missing current roadmap`);
} else {
  const roadmap = readText(currentRoadmapPath);
  for (const fragment of [
    "Only the matched Huabaosi/QiWe live feature pairs may claim this storage type",
    "production requires `huabaosi-feishu-mirror-adapter` plus",
    "`qiwe-production-adapter`",
    "with production owner/database/Feishu delivery gates",
    "Single-feature builds still",
    "fail closed",
  ]) {
    requireFragment(currentRoadmapPath, roadmap, fragment);
  }
  for (const fragment of [
    "Only the combined Huabaosi/QiWe staging feature artifact may claim this storage",
    "production and single-feature builds still fail closed",
  ]) {
    forbidFragment(currentRoadmapPath, roadmap, fragment);
  }
}

const sidecarDeployPath = "deploy/sidecar/scripts/server-deploy.sh";
if (!exists(sidecarDeployPath)) {
  addError(`${sidecarDeployPath}: missing sidecar deploy script`);
} else {
  const sidecarDeploy = readText(sidecarDeployPath);
  for (const fragment of [
    'sudo chown root:ubuntu "$ENV_FILE"',
    'sudo chmod 0640 "$ENV_FILE"',
  ]) {
    requireFragment(sidecarDeployPath, sidecarDeploy, fragment);
  }
  forbidFragment(
    sidecarDeployPath,
    sidecarDeploy,
    'sudo chown ubuntu:ubuntu "$ENV_FILE"'
  );
}

const stagingArtifactProvisionPath =
  "deploy/sidecar/scripts/fetch-staging-sidecar-artifact.sh";
if (!exists(stagingArtifactProvisionPath)) {
  addError(`${stagingArtifactProvisionPath}: missing staging artifact provisioner`);
} else {
  const provisioner = readText(stagingArtifactProvisionPath);
  for (const fragment of [
    "QINTOPIA_STAGING_SIDECAR_PROVISION_APPROVAL",
    "approved-staging-sidecar-provision",
    'repo="qintopia-agent-studio/qintopia-agent-os"',
    'workflow="artifacts.yml"',
    "GITHUB_REPOSITORY override is not allowed",
    "GITHUB_WORKFLOW override is not allowed",
    "validate_timeout_seconds",
    "GITHUB_API_MAX_TIME",
    "GITHUB_DOWNLOAD_MAX_TIME",
    "signed_download_url",
    '--write-out "%{redirect_url}"',
    "GitHub artifact download did not return a signed redirect URL",
    "validate_artifact_zip",
    "artifact zip entry must stay under artifact root",
    "artifact zip entry is not allowlisted",
    "artifact zip entries must exactly match the staging allowlist",
    "qintopia-message-sidecar-staging-linux-x86_64-gnu",
    "huabaosi-image-generation-staging-smoke.sh",
    "qiwe-image-send-staging-smoke.sh",
    "huabaosi-staging-adapter",
    "qiwe-staging-adapter",
    "staging_only",
    "production_eligible",
    "/home/ubuntu/qintopia-agent-os-staging-releases",
    "--artifact-zip is test-only",
    "sha256sum -c SHA256SUMS",
    "qintopia-message-sidecar.tar.gz",
    "os.lstat(path)",
    "stat.S_ISLNK",
    "artifact entry must not be a symlink",
    "artifact entry must not be hardlinked",
    "SHA256SUMS entries must exactly match the staging allowlist",
    "path component is a symlink",
    "path component is group/world writable",
    "path component has unexpected owner",
    'mkdir -m 0755 "$release_root"',
    'mkdir -m 0755 "$release_dir"',
    'mkdir -m 0755 "$sidecar_dir"',
    'rm -rf "$release_dir"',
    'rmdir "$release_root"',
    "sidecar_dir_created=1",
    "release_dir_created=1",
    "release_root_created=1",
    "provision_complete=1",
    "chmod 0555",
  ]) {
    requireFragment(stagingArtifactProvisionPath, provisioner, fragment);
  }
  for (const fragment of [
    'repo="${GITHUB_REPOSITORY',
    'workflow="${GITHUB_WORKFLOW',
    "huabaosi-production-adapter",
    "huabaosi-feishu-mirror-adapter",
    "qiwe-production-adapter",
    "systemctl enable",
    "systemctl start",
    "gh release",
  ]) {
    forbidFragment(stagingArtifactProvisionPath, provisioner, fragment);
  }
}

const deployBundleBuilderPath = "tools/deploy/build-deploy-bundle.mjs";
if (!exists(deployBundleBuilderPath)) {
  addError(`${deployBundleBuilderPath}: missing deploy bundle builder`);
} else {
  const builder = readText(deployBundleBuilderPath);
  for (const fragment of [
    "deploy/sidecar/scripts/fetch-staging-sidecar-artifact.sh",
    "deploy/sidecar/scripts/render-staging-runtime-env.py",
    "deploy/sidecar/scripts/staging-runtime-prerequisite-observation-smoke.sh",
    "deploy/sidecar/scripts/staging-runtime-readiness-evidence-smoke.sh",
    "deploy/sidecar/scripts/staging-runtime-values-observation-smoke.sh",
    "deploy/sidecar/scripts/huabaosi-image-generation-production-canary-smoke.sh",
    "deploy/sidecar/scripts/qiwe-image-send-staging-smoke.sh",
    "deploy/sidecar/scripts/qiwe-image-send-production-observation-smoke.sh",
    "deploy/sidecar/scripts/qiwe-image-callback-bridge-production-observation-smoke.sh",
    "deploy/sidecar/scripts/activate-qiwe-image-callback-bridge-production.sh",
    "deploy/sidecar/scripts/rollback-qiwe-image-callback-bridge-production.sh",
    "docs/operations/message-sidecar-staging-values.template.json",
    "docs/operations/release-acceptance-checklist.md",
    "docs/operations/staging-runtime-provisioning-runbook.md",
    "skills/qintopia-weather/scripts/qintopia-erhua-weather-broadcast.py",
    "runtime/hermes/validate_hermes_python.py",
  ]) {
    requireFragment(deployBundleBuilderPath, builder, fragment);
  }
}

const deployRunnerCheckPath = "tools/deploy/check-deploy-runner.mjs";
if (!exists(deployRunnerCheckPath)) {
  addError(`${deployRunnerCheckPath}: missing deploy runner check`);
} else {
  requireFragment(
    deployRunnerCheckPath,
    readText(deployRunnerCheckPath),
    "tools/deploy/test-huabaosi-image-production-canary.mjs"
  );
  requireFragment(
    deployRunnerCheckPath,
    readText(deployRunnerCheckPath),
    "tools/deploy/test-huabaosi-image-production-canary-evidence.mjs"
  );
}

const stagingValuesTemplatePath =
  "docs/operations/message-sidecar-staging-values.template.json";
if (!exists(stagingValuesTemplatePath)) {
  addError(`${stagingValuesTemplatePath}: missing staging values template`);
} else {
  const template = readText(stagingValuesTemplatePath);
  for (const fragment of [
    "<staging-postgres-url-with-database-name-containing-staging>",
    "<staging-image-provider-api-key>",
    "<huabaosi-generated-image-base-token>",
    "<huabaosi-generated-image-v1-table-id>",
    "QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS",
    "<owner-reviewed-generated-image-and-qiwe-temporary-storage-hosts>",
    "<one-isolated-staging-group-id>",
  ]) {
    requireFragment(stagingValuesTemplatePath, template, fragment);
  }
  for (const fragment of [
    "postgres://",
    "postgresql://",
    "tenant_access_token",
    "xoxb-",
    "Bearer ",
  ]) {
    forbidFragment(stagingValuesTemplatePath, template, fragment);
  }
}

const stagingRuntimeRunbookPath =
  "docs/operations/staging-runtime-provisioning-runbook.md";
if (!exists(stagingRuntimeRunbookPath)) {
  addError(
    `${stagingRuntimeRunbookPath}: missing staging runtime provisioning runbook`
  );
} else {
  const runbook = readText(stagingRuntimeRunbookPath);
  for (const fragment of [
    "message-sidecar-staging-values.template.json",
    "/etc/qintopia/message-sidecar-staging-values.json",
    "/etc/qintopia/message-sidecar-staging.env",
    "server-local values file out of git",
    "approved-staging-runtime-env-provision",
    "approved-staging-sidecar-provision",
    "ready_for_huabaosi_qiwe_staging_smokes",
    "applied as-is",
    "ports outside `1..65535`",
  ]) {
    requireFragment(stagingRuntimeRunbookPath, runbook, fragment);
  }
  for (const fragment of [
    "systemctl enable --now",
    "gh release create",
    "QINTOPIA_SIDECAR_DATABASE_URL=",
    "QIWE_TOKEN=",
    "tenant_access_token",
  ]) {
    forbidFragment(stagingRuntimeRunbookPath, runbook, fragment);
  }
}

const releaseAcceptanceChecklistPath =
  "docs/operations/release-acceptance-checklist.md";
if (!exists(releaseAcceptanceChecklistPath)) {
  addError(`${releaseAcceptanceChecklistPath}: missing release acceptance checklist`);
} else {
  const checklist = readText(releaseAcceptanceChecklistPath);
  for (const fragment of [
    "Release Please validation",
    "exact current PR head",
    "force-updates the branch",
    "draft Release tag points to current `origin/master`",
    "/home/ubuntu/qintopia-agent-os-releases/current",
    "tools/deploy/build-deploy-bundle.mjs",
    "tools/deploy/check-deploy-contracts.mjs",
    "deploy/sidecar/scripts/fetch-staging-sidecar-artifact.sh",
    "deploy/sidecar/scripts/render-staging-runtime-env.py",
    "deploy/sidecar/scripts/staging-runtime-prerequisite-observation-smoke.sh",
    "deploy/sidecar/scripts/staging-runtime-readiness-evidence-smoke.sh",
    "deploy/sidecar/scripts/staging-runtime-values-observation-smoke.sh",
    "docs/operations/message-sidecar-staging-values.template.json",
    "docs/operations/staging-runtime-provisioning-runbook.md",
    "staging artifact fetch helper",
    "Do not create placeholder env files",
    "/etc/qintopia/message-sidecar-staging-values.json",
    "/etc/qintopia/message-sidecar-staging.env",
    "ready_for_huabaosi_qiwe_staging_smokes",
    "Xiaoman Completion Boundary",
    "infrastructure",
    "activation-ready",
    "production-complete",
  ]) {
    requireFragment(releaseAcceptanceChecklistPath, checklist, fragment);
  }
  for (const fragment of [
    "QINTOPIA_SIDECAR_DATABASE_URL=",
    "QIWE_TOKEN=",
    "tenant_access_token",
    "systemctl enable --now",
    "gh release create",
  ]) {
    forbidFragment(releaseAcceptanceChecklistPath, checklist, fragment);
  }
}

const releaseCurrentModelPath = "docs/operations/release-current-model.md";
if (exists(releaseCurrentModelPath)) {
  const releaseCurrentModel = readText(releaseCurrentModelPath);
  requireFragment(
    releaseCurrentModelPath,
    releaseCurrentModel,
    "release-acceptance-checklist.md"
  );
  requireFragment(
    releaseCurrentModelPath,
    releaseCurrentModel,
    "exact-head Release Please validation"
  );
}

for (const [packagePath, requiredFragments] of Object.entries(packages)) {
  const readmePath = `${packagePath}/README.md`;
  const manifestPath = `${packagePath}/manifest.yaml`;
  if (!exists(readmePath)) {
    addError(`${packagePath}: missing README.md`);
    continue;
  }
  if (!exists(manifestPath)) {
    addError(`${packagePath}: missing manifest.yaml`);
    continue;
  }

  const readme = readText(readmePath);
  for (const fragment of requiredFragments) {
    if (!readme.includes(fragment)) {
      addError(`${readmePath}: must mention ${fragment}`);
    }
  }

  const manifest = YAML.parse(readText(manifestPath));
  if (manifest.id !== packagePath) {
    addError(`${manifestPath}: id must be ${packagePath}`);
  }
  if (manifest.type !== "deploy") {
    addError(`${manifestPath}: type must be deploy`);
  }
}

const xiaomanPreflightPath =
  "deploy/sidecar/scripts/xiaoman-activity-production-preflight-smoke.sh";
if (!exists(xiaomanPreflightPath)) {
  addError(`${xiaomanPreflightPath}: missing Xiaoman production preflight smoke`);
} else {
  const preflight = readText(xiaomanPreflightPath);
  for (const fragment of [
    "QINTOPIA_XIAOMAN_ACTIVITY_PRODUCTION_PREFLIGHT_ENABLE",
    "QINTOPIA_XIAOMAN_ACTIVITY_SIGNAL_TIMER_OBSERVATION_ENABLE",
    "xiaoman-activity-signal-timer-observation-smoke.sh",
    "QINTOPIA_XIAOMAN_LEGACY_CRON_OBSERVATION_ENABLE",
    "xiaoman-legacy-cron-observation-smoke.sh",
    "QINTOPIA_XIAOMAN_ACTIVITY_PROMOTION_STARTER_TIMER_OBSERVATION_ENABLE",
    "xiaoman-activity-promotion-starter-timer-observation-smoke.sh",
    "QINTOPIA_OPERATIONS_DOWNSTREAM_TIMERS_OBSERVATION_ENABLE",
    "operations-downstream-timers-observation-smoke.sh",
    "QINTOPIA_XIAOMAN_ACTIVITY_DOWNSTREAM_OBSERVATION_ENABLE",
    "xiaoman-activity-downstream-observation-smoke.sh",
    "QINTOPIA_XIAOMAN_ACTIVITY_IMAGE_GENERATION_STARTER_OBSERVATION_ENABLE",
    "xiaoman-activity-image-generation-starter-observation-smoke.sh",
    "QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_OBSERVATION_ENABLE",
    "huabaosi-image-generation-production-observation-smoke.sh",
    "QINTOPIA_XIAOMAN_ACTIVITY_SEND_REQUEST_STARTER_OBSERVATION_ENABLE",
    "xiaoman-activity-send-request-starter-observation-smoke.sh",
    "QINTOPIA_OPERATIONS_GROUP_SEND_READY_TIMER_OBSERVATION_ENABLE",
    "operations-group-send-ready-timer-observation-smoke.sh",
    "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_OBSERVATION_ENABLE",
    "qiwe-image-send-production-observation-smoke.sh",
    "QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_OBSERVATION_ENABLE",
    "qiwe-image-callback-bridge-production-observation-smoke.sh",
    'CHILD_PATH="/usr/bin:/bin:/usr/sbin:/sbin"',
    'env -i "${child_env[@]}" "$script_path"',
    '"PATH=${CHILD_PATH}"',
  ]) {
    requireFragment(xiaomanPreflightPath, preflight, fragment);
  }

  for (const fragment of [
    "env QINTOPIA_",
    "QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE=1",
    "QINTOPIA_SIDECAR_ENV_FILE=",
    "SYSTEMCTL=",
    "JOURNALCTL=",
    "_OBSERVATION_TEST_MODE",
    "server-deploy.sh",
    "gh release",
    "release create",
    "release edit",
    "run-group-message-send-worker",
    "send_executed=true",
    "--use-feishu-base",
    "tenant_access_token",
    "QIWE_TOKEN",
    "QIWE_GUID",
  ]) {
    forbidFragment(xiaomanPreflightPath, preflight, fragment);
  }
}

const qiweCallbackBridgeProductionObservationPath =
  "deploy/sidecar/scripts/qiwe-image-callback-bridge-production-observation-smoke.sh";
if (!exists(qiweCallbackBridgeProductionObservationPath)) {
  addError(
    `${qiweCallbackBridgeProductionObservationPath}: missing production observation smoke`
  );
} else {
  const smoke = readText(qiweCallbackBridgeProductionObservationPath);
  for (const fragment of [
    "QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_OBSERVATION_ENABLE",
    "QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_ENABLED",
    "QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_MODE",
    "QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_BIN",
    "QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_ROOT",
    "QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_SHA256",
    "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256",
    "callback bridge production database hash does not match runtime database URL",
    "/home/ubuntu/.hermes/profiles/erhua/.env",
    "/home/ubuntu/.hermes/profiles/erhua/plugins/qiwe-platform",
    "/home/ubuntu/qintopia-agent-os-releases/current/sidecar/qintopia-message-sidecar",
    "skills/qiwe/image_callback_bridge.py",
    "qiwe_image_callback_bridge_production_observation_state",
    "huabaosi-production-adapter",
    "qiwe-production-adapter",
  ]) {
    requireFragment(qiweCallbackBridgeProductionObservationPath, smoke, fragment);
  }
  for (const fragment of [
    "process-qiwe-image-send-callback",
    "--apply",
    "systemctl enable",
    "systemctl start",
    "source ",
    "eval ",
    "tenant_access_token",
    "raw_body",
  ]) {
    forbidFragment(qiweCallbackBridgeProductionObservationPath, smoke, fragment);
  }
}

const qiweCallbackBridgeProductionActivationPath =
  "deploy/sidecar/scripts/activate-qiwe-image-callback-bridge-production.sh";
if (!exists(qiweCallbackBridgeProductionActivationPath)) {
  addError(
    `${qiweCallbackBridgeProductionActivationPath}: missing production activation script`
  );
} else {
  const activation = readText(qiweCallbackBridgeProductionActivationPath);
  for (const fragment of [
    "QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_ACTIVATION",
    "approved-production-qiwe-image-callback-bridge",
    "qiwe-image-callback-bridge-production-observation-smoke.sh",
    'RUNUSER_BIN="/usr/sbin/runuser"',
    'HERMES_SYSTEMD_USER="ubuntu"',
    'HERMES_SERVICE="hermes-gateway-erhua.service"',
    'QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_EXPECTED_STATE="$expected_state"',
    "QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_OBSERVATION_ENABLE=1",
    "run_observation enabled",
    "restart_erhua",
    "systemctl --user restart ${HERMES_SERVICE}",
    "systemctl --user is-active --quiet ${HERMES_SERVICE}",
    "env -i",
    "PATH=/usr/bin:/bin:/usr/sbin:/sbin",
  ]) {
    requireFragment(qiweCallbackBridgeProductionActivationPath, activation, fragment);
  }
  for (const fragment of [
    "systemctl enable",
    "systemctl start",
    "process-qiwe-image-send-callback",
    "--apply",
    "source ",
    "eval ",
    "tenant_access_token",
    "QIWE_TOKEN",
    "QIWE_GUID",
    "QINTOPIA_SIDECAR_DATABASE_URL",
    "TEST_MODE",
    "_TEST_MODE",
    "RUNUSER_BIN:-",
    "QINTOPIA_HERMES_SYSTEMD_USER",
    "QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_HERMES_SERVICE",
    "QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_OBSERVATION_TEST_MODE",
    "QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_OBSERVATION_TEST_ROOT",
    "QINTOPIA_RELEASE_CURRENT_DIR",
    "QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_ENV_FILE",
    "QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PLUGIN_PATH",
  ]) {
    forbidFragment(qiweCallbackBridgeProductionActivationPath, activation, fragment);
  }
}

const qiweCallbackBridgeProductionRollbackPath =
  "deploy/sidecar/scripts/rollback-qiwe-image-callback-bridge-production.sh";
if (!exists(qiweCallbackBridgeProductionRollbackPath)) {
  addError(
    `${qiweCallbackBridgeProductionRollbackPath}: missing production rollback script`
  );
} else {
  const rollback = readText(qiweCallbackBridgeProductionRollbackPath);
  for (const fragment of [
    "QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_ROLLBACK",
    "approved-production-qiwe-image-callback-bridge-rollback",
    "qiwe-image-callback-bridge-production-observation-smoke.sh",
    'RUNUSER_BIN="/usr/sbin/runuser"',
    'HERMES_SYSTEMD_USER="ubuntu"',
    'HERMES_SERVICE="hermes-gateway-erhua.service"',
    'QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_EXPECTED_STATE="$expected_state"',
    "QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_OBSERVATION_ENABLE=1",
    "run_observation disabled",
    "restart_erhua",
    "systemctl --user restart ${HERMES_SERVICE}",
    "systemctl --user is-active --quiet ${HERMES_SERVICE}",
    "env -i",
    "PATH=/usr/bin:/bin:/usr/sbin:/sbin",
  ]) {
    requireFragment(qiweCallbackBridgeProductionRollbackPath, rollback, fragment);
  }
  for (const fragment of [
    "systemctl enable",
    "systemctl start",
    "process-qiwe-image-send-callback",
    "--apply",
    "source ",
    "eval ",
    "tenant_access_token",
    "QIWE_TOKEN",
    "QIWE_GUID",
    "QINTOPIA_SIDECAR_DATABASE_URL",
    "TEST_MODE",
    "_TEST_MODE",
    "RUNUSER_BIN:-",
    "QINTOPIA_HERMES_SYSTEMD_USER",
    "QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_HERMES_SERVICE",
    "QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_OBSERVATION_TEST_MODE",
    "QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_OBSERVATION_TEST_ROOT",
    "QINTOPIA_RELEASE_CURRENT_DIR",
    "QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_ENV_FILE",
    "QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PLUGIN_PATH",
  ]) {
    forbidFragment(qiweCallbackBridgeProductionRollbackPath, rollback, fragment);
  }
}

const xiaomanLegacyCronObservationPath =
  "deploy/sidecar/scripts/xiaoman-legacy-cron-observation-smoke.sh";
if (!exists(xiaomanLegacyCronObservationPath)) {
  addError(
    `${xiaomanLegacyCronObservationPath}: missing Xiaoman legacy cron observation`
  );
} else {
  const smoke = readText(xiaomanLegacyCronObservationPath);
  for (const fragment of [
    "QINTOPIA_XIAOMAN_LEGACY_CRON_OBSERVATION_ENABLE",
    "/home/ubuntu/.hermes/profiles/xiaoman",
    "/home/ubuntu/.hermes/profiles/xiaoman/cron/jobs.json",
    "no_legacy_cron_jobs",
    "cron_decl_count",
    "live_profile_modified",
    "external_calls_executed",
    "Xiaoman legacy cron observation found runtime cron job declarations",
  ]) {
    requireFragment(xiaomanLegacyCronObservationPath, smoke, fragment);
  }
  for (const fragment of [
    "systemctl",
    "rm ",
    "mv ",
    "cp ",
    "source ",
    "eval ",
    "run-",
    "send_executed=true",
    "QIWE_TOKEN",
    "tenant_access_token",
  ]) {
    forbidFragment(xiaomanLegacyCronObservationPath, smoke, fragment);
  }
}

const xiaomanPreflightRecordPath =
  "deploy/smoke/docs/xiaoman-production-preflight-record.md";
if (!exists(xiaomanPreflightRecordPath)) {
  addError(`${xiaomanPreflightRecordPath}: missing Xiaoman preflight record template`);
} else {
  const record = readText(xiaomanPreflightRecordPath);
  for (const fragment of [
    "Do not paste secrets, raw chat logs, Feishu Base",
    "QINTOPIA_XIAOMAN_ACTIVITY_PRODUCTION_PREFLIGHT_ENABLE=1",
    "qintopia-agentos-xiaoman-activity-signal-worker.timer",
    "run-xiaoman-activity-signal-worker --once --apply",
    "qintopia-agentos-xiaoman-activity-promotion-starter-worker.timer",
    "run-xiaoman-activity-promotion-starter-worker --once --apply",
    "qintopia-agentos-operations-evidence-worker.timer",
    "run-evidence-worker --once --apply",
    "qintopia-agentos-operations-visual-worker.timer",
    "run-collaboration-worker --work-item-type visual_asset_request --once --apply",
    "qintopia-agentos-xiaoman-activity-image-generation-starter-worker.timer",
    "run-xiaoman-activity-image-generation-starter-worker --once --apply",
    "Huabaosi provider runtime state",
    "run-huabaosi-image-generation-worker --once --dry-run",
    "qintopia-agentos-xiaoman-activity-send-request-starter-worker.timer",
    "run-xiaoman-activity-send-request-starter-worker --once --apply",
    "Secret and external-send scan",
    "send_executed=true",
    "Production boundary",
    "Eligible Xiaoman `event_signals` preview count",
    "Eligible image-generation request preview count",
    "Eligible awaiting publish group message request count",
    "Pass: production observation can continue without executing external adapters",
    "Hold: one or more timers, commands, previews, or boundary checks failed",
    "Passing this preflight does not approve publishing",
  ]) {
    requireFragment(xiaomanPreflightRecordPath, record, fragment);
  }
}

const xiaomanImageStarterObservationPath =
  "deploy/sidecar/scripts/xiaoman-activity-image-generation-starter-observation-smoke.sh";
if (!exists(xiaomanImageStarterObservationPath)) {
  addError(`${xiaomanImageStarterObservationPath}: missing observation smoke`);
} else {
  const smoke = readText(xiaomanImageStarterObservationPath);
  for (const fragment of [
    "QINTOPIA_XIAOMAN_ACTIVITY_IMAGE_GENERATION_STARTER_OBSERVATION_ENABLE",
    "qintopia-agentos-xiaoman-activity-image-generation-starter-worker.service",
    "qintopia-agentos-xiaoman-activity-image-generation-starter-worker.timer",
    "run-xiaoman-activity-image-generation-starter-worker --once --apply",
    "run-xiaoman-activity-image-generation-starter-worker --check-only",
    "OnBootSec=9min",
    "safe_for_chat",
    "QINTOPIA_XIAOMAN_ACTIVITY_IMAGE_GENERATION_STARTER_TIMER_INTERVAL_EXPECTED:-${QINTOPIA_XIAOMAN_ACTIVITY_IMAGE_GENERATION_STARTER_TIMER_INTERVAL:-2min}",
    "--use-feishu-base",
    "send_executed=true",
  ]) {
    requireFragment(xiaomanImageStarterObservationPath, smoke, fragment);
  }
  for (const fragment of [
    "QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE=1",
    "run-huabaosi-image-generation-worker --once --apply",
    "xiaoman-activity shadow-validate",
  ]) {
    forbidFragment(xiaomanImageStarterObservationPath, smoke, fragment);
  }
}

const huabaosiImageProductionObservationPath =
  "deploy/sidecar/scripts/huabaosi-image-generation-production-observation-smoke.sh";
if (!exists(huabaosiImageProductionObservationPath)) {
  addError(`${huabaosiImageProductionObservationPath}: missing observation smoke`);
} else {
  const smoke = readText(huabaosiImageProductionObservationPath);
  for (const fragment of [
    "QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_OBSERVATION_ENABLE",
    "QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED",
    'PROVIDER_SERVICE_NAME="qintopia-agentos-huabaosi-image-generation-worker.service"',
    'PROVIDER_TIMER_NAME="qintopia-agentos-huabaosi-image-generation-worker.timer"',
    "huabaosi-image-generation-preflight",
    "run-huabaosi-image-generation-worker --once --dry-run",
    'worker_stderr="$tmp_dir/worker-preview.stderr"',
    "worker_status=$?",
    'assert_no_sensitive_output "image worker dry-run stderr"',
    "generation_enabled",
    "adapter_compiled",
    "generation_flag//[[:space:]]/",
    "safe_for_chat",
    "contains forbidden sensitive output",
    "--use-feishu-base",
  ]) {
    requireFragment(huabaosiImageProductionObservationPath, smoke, fragment);
  }
  for (const fragment of [
    "run-huabaosi-image-generation-worker --once --apply",
    "generated_image_created",
    "run-group-message-send-worker",
  ]) {
    forbidFragment(huabaosiImageProductionObservationPath, smoke, fragment);
  }
}

const huabaosiImageProductionCanaryPath =
  "deploy/sidecar/scripts/huabaosi-image-generation-production-canary-smoke.sh";
if (!exists(huabaosiImageProductionCanaryPath)) {
  addError(`${huabaosiImageProductionCanaryPath}: missing production canary command`);
} else {
  const canary = readText(huabaosiImageProductionCanaryPath);
  for (const fragment of [
    "QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_ENABLE",
    "approved-production-image-generation-canary",
    'REVIEWER_ID="trainer"',
    'PRODUCTION_ENV_FILE="/etc/qintopia/message-sidecar.env"',
    'PROVIDER_TIMER="qintopia-agentos-huabaosi-image-generation-worker.timer"',
    'PATH="/usr/bin:/bin:/usr/sbin:/sbin"',
    'SYSTEMCTL="/usr/bin/systemctl"',
    "os.path.realpath",
    "stat.S_ISLNK",
    "production canary test mode is forbidden from production release roots",
    "production canary test mode may execute only a temporary fake sidecar",
    'timer_enabled_state="$("$SYSTEMCTL" is-enabled "$PROVIDER_TIMER" 2>/dev/null || true)"',
    '[[ "$timer_enabled_state" != "disabled" ]]',
    "production provider timer must be disabled during one-shot canary",
    'if "$SYSTEMCTL" is-active --quiet "$PROVIDER_TIMER"',
    "production provider timer must be inactive during one-shot canary",
    "production canary sidecar hash does not match",
    "production canary database hash does not match",
    "production environment contains a duplicate canary key",
    "operations-artifact-review-decision --apply",
    '"expected_artifact_type": "poster_brief"',
    '"expected_review_status": "pending"',
    'assert data["artifact_type"] == "poster_brief"',
    'assert data["previous_review_status"] == "pending"',
    'BRIEF_WORK_ITEM_ID="${review_facts[2]}"',
    "run-xiaoman-activity-image-generation-starter-worker --once --apply --work-item-id",
    'assert data["requested_work_item_id"] == sys.argv[1]',
    'assert item["parent_work_item_id"] == sys.argv[1]',
    "run-huabaosi-image-generation-worker --once --work-item-id",
    "huabaosi-feishu-primary-storage-revalidate --artifact-id",
    'assert artifact["review_status"] == "pending"',
    "database_writes_executed",
    "contains sensitive output",
    "one Feishu-backed JPEG remains pending human review",
  ]) {
    requireFragment(huabaosiImageProductionCanaryPath, canary, fragment);
  }
  for (const fragment of [
    'source "$ENV_FILE"',
    "eval ",
    'SYSTEMCTL="systemctl"',
    "systemctl enable",
    "systemctl start",
    'operations-artifact-review-decision --apply --payload-json "{',
    "run-group-message-send-worker",
    "run-qiwe-image-send-worker",
  ]) {
    forbidFragment(huabaosiImageProductionCanaryPath, canary, fragment);
  }
}

const huabaosiWeComGatewayObservationPath =
  "deploy/sidecar/scripts/huabaosi-wecom-gateway-observation-smoke.sh";
if (!exists(huabaosiWeComGatewayObservationPath)) {
  addError(`${huabaosiWeComGatewayObservationPath}: missing observation smoke`);
} else {
  const smoke = readText(huabaosiWeComGatewayObservationPath);
  for (const fragment of [
    "QINTOPIA_HUABAOSI_WECOM_OBSERVATION_ENABLE",
    "hermes-gateway-huabaosi.service",
    '--user is-active "$SERVICE_NAME"',
    '--user show "$SERVICE_NAME" --property=WorkingDirectory --property=ExecStart --property=DropInPaths',
    '--user -u "$SERVICE_NAME"',
    "WorkingDirectory=${PROFILE_DIR}",
    "--profile huabaosi gateway run --replace",
    "DropInPaths=",
    "drop-in overrides",
    "busy_input_mode",
    "QINTOPIA_RELEASE_CURRENT_PATH",
    "internal_filter_count",
    "send_fallback_count",
    "api_timeout_count",
    "contains forbidden sensitive output",
  ]) {
    requireFragment(huabaosiWeComGatewayObservationPath, smoke, fragment);
  }
  for (const fragment of [
    "systemctl restart",
    "systemctl reload",
    "systemctl start",
    "systemctl enable",
    'cat "$SERVICE_NAME"',
    'source "$ENV_FILE"',
    ". /etc/qintopia/message-sidecar.env",
    "run-huabaosi-image-generation-worker",
    "run-group-message-send-worker",
    "--apply",
  ]) {
    forbidFragment(huabaosiWeComGatewayObservationPath, smoke, fragment);
  }
}

const huabaosiWeComCanaryObservationPath =
  "deploy/sidecar/scripts/huabaosi-wecom-canary-observation-smoke.sh";
if (!exists(huabaosiWeComCanaryObservationPath)) {
  addError(`${huabaosiWeComCanaryObservationPath}: missing observation smoke`);
} else {
  const smoke = readText(huabaosiWeComCanaryObservationPath);
  for (const fragment of [
    "QINTOPIA_HUABAOSI_WECOM_CANARY_OBSERVATION_ENABLE",
    "qintopia-agentos-huabaosi-wecom-canary-gateway.service",
    "qintopia-agentos-huabaosi-wecom-canary-gateway.timer",
    "huabaosi-wecom-canary-preflight",
    "canary_enabled",
    "staging_adapter_not_compiled",
    "canary_configuration_not_approved",
    "QINTOPIA_HUABAOSI_WECOM_CANARY_TOKEN",
    "${MONOREPO_ROOT}/sidecar/qintopia-message-sidecar",
    "contains forbidden sensitive output",
  ]) {
    requireFragment(huabaosiWeComCanaryObservationPath, smoke, fragment);
  }
  for (const fragment of [
    "huabaosi-wecom-canary-gateway --apply",
    "systemctl restart",
    "systemctl reload",
    "systemctl start",
    "systemctl enable",
    'source "$ENV_FILE"',
    ". /etc/qintopia/message-sidecar.env",
    "run-huabaosi-image-generation-worker",
    "run-group-message-send-worker",
  ]) {
    forbidFragment(huabaosiWeComCanaryObservationPath, smoke, fragment);
  }
}

for (const observationPath of [
  "deploy/sidecar/scripts/operations-downstream-timers-observation-smoke.sh",
  "deploy/sidecar/scripts/operations-group-send-ready-timer-observation-smoke.sh",
  "deploy/sidecar/scripts/xiaoman-activity-downstream-observation-smoke.sh",
  "deploy/sidecar/scripts/xiaoman-activity-image-generation-starter-observation-smoke.sh",
  "deploy/sidecar/scripts/huabaosi-image-generation-production-observation-smoke.sh",
  "deploy/sidecar/scripts/huabaosi-wecom-canary-observation-smoke.sh",
  "deploy/sidecar/scripts/huabaosi-wecom-gateway-observation-smoke.sh",
  "deploy/sidecar/scripts/xiaoman-activity-promotion-starter-timer-observation-smoke.sh",
  "deploy/sidecar/scripts/xiaoman-activity-send-request-starter-observation-smoke.sh",
  "deploy/sidecar/scripts/xiaoman-activity-signal-timer-observation-smoke.sh",
]) {
  const smoke = exists(observationPath) ? readText(observationPath) : "";
  requireFragment(observationPath, smoke, "contains forbidden sensitive output");
  forbidFragment(observationPath, smoke, "leaked forbidden output: ${token}");
}

const aliangStagingSmokePath =
  "deploy/sidecar/scripts/huabaosi-image-generation-staging-smoke.sh";
const aliangStagingReadinessPath =
  "deploy/sidecar/scripts/huabaosi-image-generation-staging-readiness-smoke.sh";
const stagingRuntimePrerequisiteObservationPath =
  "deploy/sidecar/scripts/staging-runtime-prerequisite-observation-smoke.sh";
const stagingRuntimeValuesObservationPath =
  "deploy/sidecar/scripts/staging-runtime-values-observation-smoke.sh";
if (!exists(stagingRuntimeValuesObservationPath)) {
  addError(
    `${stagingRuntimeValuesObservationPath}: missing staging runtime values observation smoke`
  );
} else {
  const observation = readText(stagingRuntimeValuesObservationPath);
  for (const fragment of [
    "QINTOPIA_STAGING_RUNTIME_VALUES_OBSERVATION_ENABLE",
    "QINTOPIA_STAGING_RUNTIME_VALUES_OBSERVATION_TEST_MODE",
    "/etc/qintopia/message-sidecar-staging-values.json",
    "/etc/qintopia/message-sidecar-staging.env",
    "deploy/sidecar/scripts/render-staging-runtime-env.py",
    "staging_runtime_values_observation=",
    "ready_for_render_validation",
    "server-local values file contents are not read",
    "staging env file contents are not read",
    "renderer is not executed",
    "no Postgres, Huabaosi, Feishu, QiWe, provider, media, service, timer, release, or network action",
    "values_file_present",
    "env_file_already_present",
    "path_parent_is_symlink",
    "path_parent_missing",
    "path_group_or_world_writable",
    "path_group_or_world_readable",
  ]) {
    requireFragment(stagingRuntimeValuesObservationPath, observation, fragment);
  }
  for (const fragment of [
    "systemctl",
    "source ",
    'source "$',
    ". /etc/qintopia",
    "env -i",
    "QINTOPIA_SIDECAR_DATABASE_URL",
    "QINTOPIA_HUABAOSI_IMAGE_API_KEY",
    "QIWE_TOKEN",
    "run-huabaosi-image-generation-worker",
    "run-qiwe-image-send-worker",
    "curl ",
    "psql ",
  ]) {
    forbidFragment(stagingRuntimeValuesObservationPath, observation, fragment);
  }
}
if (!exists(stagingRuntimePrerequisiteObservationPath)) {
  addError(
    `${stagingRuntimePrerequisiteObservationPath}: missing staging runtime prerequisite observation smoke`
  );
} else {
  const observation = readText(stagingRuntimePrerequisiteObservationPath);
  for (const fragment of [
    "QINTOPIA_STAGING_RUNTIME_PREREQUISITE_OBSERVATION_ENABLE",
    "QINTOPIA_STAGING_RUNTIME_PREREQUISITE_OBSERVATION_TEST_MODE",
    "/etc/qintopia/message-sidecar-staging.env",
    "/home/ubuntu/qintopia-agent-os-staging-releases",
    "QINTOPIA_STAGING_RUNTIME_PREREQUISITE_RELEASE_SHA",
    "QINTOPIA_STAGING_RUNTIME_PREREQUISITE_SIDECAR_SHA256",
    "staging_runtime_prerequisite_observation=",
    "ready_for_staging_readiness_smokes",
    "staging env file contents are not read",
    "sidecar binary is not executed",
    "no Postgres, Huabaosi, Feishu, QiWe, provider, media, service, timer, release, or network action",
    "path_is_secure",
    "require_executable",
    "os.access(path, os.X_OK)",
    "path_not_executable",
    "reject_owner_writable",
    "path_owner_group_or_world_writable",
    "path_group_or_world_writable",
    "path_is_symlink",
    "path_parent_is_symlink",
    "path_parent_group_or_world_writable",
    "path_parent_unexpected_owner",
    "sidecar_hash_mismatch",
  ]) {
    requireFragment(stagingRuntimePrerequisiteObservationPath, observation, fragment);
  }
  for (const fragment of [
    "systemctl",
    "source ",
    'source "$',
    ". /etc/qintopia",
    "env -i",
    "QINTOPIA_SIDECAR_DATABASE_URL",
    "QINTOPIA_HUABAOSI_IMAGE_API_KEY",
    "QIWE_TOKEN",
    "run-huabaosi-image-generation-worker",
    "huabaosi-image-generation-preflight",
    "curl ",
    "psql ",
  ]) {
    forbidFragment(stagingRuntimePrerequisiteObservationPath, observation, fragment);
  }
}
if (!exists(aliangStagingReadinessPath)) {
  addError(`${aliangStagingReadinessPath}: missing Huabaosi staging readiness smoke`);
} else {
  const readiness = readText(aliangStagingReadinessPath);
  for (const fragment of [
    "QINTOPIA_HUABAOSI_IMAGE_STAGING_READINESS_ENABLE",
    "QINTOPIA_HUABAOSI_IMAGE_STAGING_APPROVAL",
    "approved-staging-image-generation",
    "/etc/qintopia/message-sidecar-staging.env",
    "/home/ubuntu/qintopia-agent-os-staging-releases",
    "QINTOPIA_HUABAOSI_IMAGE_STAGING_RELEASE_SHA",
    "QINTOPIA_HUABAOSI_IMAGE_STAGING_SIDECAR_SHA256",
    "huabaosi_image_generation_staging_readiness=",
    "ready_for_staging_preflight",
    "staging env file contents are not read",
    "sidecar binary is not executed",
    "no Huabaosi, Postgres, Feishu, QiWe, provider, media, service, or timer action",
    "path_is_secure",
    "require_executable",
    "os.access(path, os.X_OK)",
    "path_not_executable",
    "reject_owner_writable",
    "path_owner_group_or_world_writable",
    "path_group_or_world_writable",
    "path_is_symlink",
    "path_parent_is_symlink",
    "path_parent_group_or_world_writable",
    "path_parent_unexpected_owner",
    "path_unexpected_owner",
    "sidecar_hash_mismatch",
  ]) {
    requireFragment(aliangStagingReadinessPath, readiness, fragment);
  }
  for (const fragment of [
    "systemctl",
    "source ",
    'source "$',
    ". /etc/qintopia",
    "env -i",
    "QINTOPIA_SIDECAR_DATABASE_URL",
    "QINTOPIA_HUABAOSI_IMAGE_API_KEY",
    "run-huabaosi-image-generation-worker",
    "huabaosi-image-generation-preflight",
    "subprocess",
    "curl ",
    "psql ",
  ]) {
    forbidFragment(aliangStagingReadinessPath, readiness, fragment);
  }
}

if (!exists(aliangStagingSmokePath)) {
  addError(`${aliangStagingSmokePath}: missing Huabaosi staging smoke`);
} else {
  const smoke = readText(aliangStagingSmokePath);
  for (const fragment of [
    "QINTOPIA_HUABAOSI_IMAGE_STAGING_SMOKE_ENABLE",
    "QINTOPIA_HUABAOSI_IMAGE_STAGING_APPROVAL",
    "approved-staging-image-generation",
    "QINTOPIA_HUABAOSI_IMAGE_STAGING_ENV_FILE",
    "QINTOPIA_HUABAOSI_IMAGE_STAGING_DATABASE_URL_SHA256",
    "QINTOPIA_HUABAOSI_IMAGE_STAGING_RELEASE_SHA",
    "QINTOPIA_HUABAOSI_IMAGE_STAGING_SIDECAR_SHA256",
    "QINTOPIA_HUABAOSI_IMAGE_STAGING_WORK_ITEM_ID",
    "QINTOPIA_HUABAOSI_IMAGE_STAGING_DATABASE_URL_SHA256 must be a canonical SHA-256",
    "QINTOPIA_HUABAOSI_IMAGE_STAGING_RELEASE_SHA must be a 40-character lowercase hex SHA",
    "QINTOPIA_HUABAOSI_IMAGE_STAGING_SIDECAR_SHA256 must be a canonical SHA-256",
    "QINTOPIA_HUABAOSI_IMAGE_STAGING_SMOKE_TEST_MODE must be 0 or 1",
    "Huabaosi staging smoke must run from /home/ubuntu/qintopia-agent-os-staging-releases/<approved 40-hex sha>",
    "Huabaosi staging smoke test mode may read only a temporary fake env file",
    "QINTOPIA_SIDECAR_BIN is test-only and must not override the fixed staging release sidecar",
    "packaged sidecar/qintopia-message-sidecar or QINTOPIA_SIDECAR_BIN is required for Huabaosi staging smoke",
    "verify_sidecar_binary",
    "staging sidecar binary hash changed before",
    "staging sidecar binary must stay under the fixed staging release root before",
    "staging sidecar binary must come from /home/ubuntu/qintopia-agent-os-staging-releases/<approved 40-hex sha> before",
    "candidate_lstat.st_uid == os.geteuid()",
    "staging sidecar binary, parent directory, and release ancestors must not be writable by the staging runner or by group/world before",
    '(release_root_parent, "directory", True)',
    '(root, "directory", True)',
    "sidecar_binary_sha256",
    "STAGING_ENV_KEYS",
    "IGNORED_STAGING_ENV_KEYS",
    "load_staging_env",
    "QINTOPIA_QIWE_IMAGE_SEND_ENABLED",
    "QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS",
    "staging database URL hash does not match the approved command",
    "CHILD_ENV",
    "add_child_env",
    "add_child_env_if_set",
    "env -i",
    'verify_sidecar_binary "$label spawn"',
    'output="$(env -i "${CHILD_ENV[@]}" "$@" 2>&1)"',
    "assert_no_sensitive_text",
    'payload["adapter_compiled"] is True',
    "huabaosi-image-generation-preflight",
    "run-huabaosi-image-generation-worker",
    "generated_image_created",
    "pending",
    "huabaosi_image_generation_staging_evidence=",
    "emit_sanitized_evidence",
    "payload = json.load(sys.stdin)",
    "database_url_sha256",
    "content_hash",
    "mime_type",
    "storage_backend",
    "feishu-base",
    'urlparse(artifact["artifact_uri"]).scheme',
    "generated image storage boundary is not Feishu Base",
    "QINTOPIA_HUABAOSI_IMAGE_STORAGE_BACKEND",
    "QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED",
    "QINTOPIA_HUABAOSI_FEISHU_MIRROR_APPROVAL",
    "QINTOPIA_HUABAOSI_FEISHU_BASE_TOKEN",
    "QINTOPIA_HUABAOSI_FEISHU_ARTIFACT_TABLE_ID",
    'hashlib.sha256(value.encode("utf-8")).hexdigest()',
    "urlparse(value).path",
  ]) {
    requireFragment(aliangStagingSmokePath, smoke, fragment);
  }

  for (const fragment of [
    "systemctl",
    'source "$ENV_FILE"',
    ". /etc/qintopia/message-sidecar-staging.env",
    "mktemp",
    "preflight_output",
    "worker_output",
    '>"$preflight_output"',
    '>"$worker_output"',
    'python3 - "$QINTOPIA_SIDECAR_DATABASE_URL"',
    "run-group-message-send-worker",
    "--use-feishu-base",
    "send-ready",
    "operations-group-message-confirm",
    "SANITIZED_EVIDENCE_PAYLOAD",
    "json.loads(os.environ",
    "QINTOPIA_HUABAOSI_MEDIA_UPLOAD_ENDPOINT",
    "QINTOPIA_HUABAOSI_MEDIA_PUBLIC_BASE_URL",
  ]) {
    forbidFragment(aliangStagingSmokePath, smoke, fragment);
  }
}

const aliangStagingEvidenceCheckPath =
  "tools/deploy/check-huabaosi-image-staging-evidence.mjs";
if (!exists(aliangStagingEvidenceCheckPath)) {
  addError(
    `${aliangStagingEvidenceCheckPath}: missing Huabaosi staging evidence checker`
  );
} else {
  const checker = readText(aliangStagingEvidenceCheckPath);
  for (const fragment of [
    "huabaosi_image_generation_staging_evidence=",
    "Huabaosi image staging evidence check passed.",
    "expected exactly two Huabaosi staging evidence records",
    "expected one preflight and one generation evidence record",
    "generation evidence does not prove one pending final JPEG",
    "artifact_uri",
    "https?:",
    "storage_backend",
    "feishu-base",
    "staging sidecar binary hash is missing or inconsistent",
  ]) {
    requireFragment(aliangStagingEvidenceCheckPath, checker, fragment);
  }
}

const aliangStagingEvidenceTemplatePath =
  "docs/reports/templates/huabaosi-image-generation-staging-evidence.md";
if (!exists(aliangStagingEvidenceTemplatePath)) {
  addError(
    `${aliangStagingEvidenceTemplatePath}: missing Huabaosi staging evidence template`
  );
} else {
  const template = readText(aliangStagingEvidenceTemplatePath);
  for (const fragment of [
    "node tools/deploy/check-huabaosi-image-staging-evidence.mjs <huabaosi-staging-evidence-output.txt>",
    "Repository commit SHA",
    "Packaged sidecar binary SHA-256",
    "Staging database URL SHA-256",
    "Image request work item UUID",
    "Final JPEG `content_hash`",
    "Review status: `pending`",
    "`adapter_config_ready`",
    "`generated_image_created`",
    "External provider call",
    "Feishu Base write",
    "QiWe send",
    "`database_url_sha256`",
    "`sidecar_binary_sha256`",
    "`content_hash`",
    "`mime_type`: `image/jpeg`",
    "`storage_backend`: `feishu-base`",
    "Complete Huabaosi evidence checker passed",
    "QiWe staging send must wait for manual approval revalidation and combined",
    "Feishu-to-QiWe bridge evidence",
    "no QiWe send, production timer, service, Release publish",
    "Do not record provider endpoint, provider response, API key, token, database URL",
  ]) {
    requireFragment(aliangStagingEvidenceTemplatePath, template, fragment);
  }
  for (const fragment of [
    "QINTOPIA_HUABAOSI_IMAGE_API_KEY=",
    "postgres://",
    "postgresql://",
    "https://",
    "artifact_uri",
    "filename",
    "systemctl enable",
    "systemctl start",
    "gh release",
  ]) {
    forbidFragment(aliangStagingEvidenceTemplatePath, template, fragment);
  }
}

const stagingRuntimeProvisioningRunbookPath =
  "docs/operations/staging-runtime-provisioning-runbook.md";
if (!exists(stagingRuntimeProvisioningRunbookPath)) {
  addError(
    `${stagingRuntimeProvisioningRunbookPath}: missing staging runtime provisioning runbook`
  );
} else {
  const runbook = readText(stagingRuntimeProvisioningRunbookPath);
  for (const fragment of [
    "/etc/qintopia/message-sidecar-staging.env",
    "/home/ubuntu/qintopia-agent-os-staging-releases/<40-hex-sha>/sidecar/qintopia-message-sidecar",
    "QINTOPIA_STAGING_RUNTIME_PREREQUISITE_OBSERVATION_ENABLE=1",
    "staging-runtime-prerequisite-observation-smoke.sh",
    "QINTOPIA_STAGING_RUNTIME_VALUES_OBSERVATION_ENABLE=1",
    "staging-runtime-values-observation-smoke.sh",
    "ready_for_render_validation",
    "QINTOPIA_STAGING_RUNTIME_READINESS_EVIDENCE_ENABLE=1",
    "staging-runtime-readiness-evidence-smoke.sh",
    "staging_runtime_readiness_evidence=",
    "ready_for_huabaosi_qiwe_staging_smokes",
    "render-staging-runtime-env.py",
    "staging_runtime_env_render=",
    "/etc/qintopia/message-sidecar-staging-values.json",
    "approved-staging-runtime-env-provision",
    "mode `0600`",
    "requires exactly one isolated staging group id",
    "docs/reports/2026-07-16-staging-runtime-prerequisite-observation.md",
    "staging release SHA",
    "packaged staging sidecar SHA-256",
    "staging database URL SHA-256",
    "Huabaosi staging keys",
    "Downstream QiWe staging keys",
    "QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED",
    "QINTOPIA_HUABAOSI_IMAGE_API_KEY",
    "QINTOPIA_HUABAOSI_IMAGE_STORAGE_BACKEND=feishu-base",
    "QINTOPIA_HUABAOSI_FEISHU_SCHEMA_VERSION=huabaosi-generated-image-v1",
    "QINTOPIA_QIWE_IMAGE_SEND_ENABLED",
    "QIWE_TOKEN",
    "QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS",
    "staging env file already contains the downstream QiWe keys",
    "ignore those keys and must not pass them to its child sidecar process",
    "invalid assignment syntax still fail closed",
    "QINTOPIA_STAGING_SIDECAR_PROVISION_APPROVAL=approved-staging-sidecar-provision",
    "fetch-staging-sidecar-artifact.sh",
    "--sha '<approved staging release sha>'",
    "do not provision the older `37fff8bf...` staging binary",
    "bridge exercise",
    "first build or fetch a successful staging-only artifact",
    "release SHA and record its reviewed sidecar SHA-256",
    "Previous staging artifact evidence, retained only as historical proof",
    "deploy bundle contains `deploy/sidecar/scripts/fetch-staging-sidecar-artifact.sh`",
    "no checked path component is a symlink",
    "no checked path component is group- or world-writable",
    "staging database URL hash is absent",
    "node tools/deploy/check-huabaosi-image-staging-evidence.mjs",
    "docs/reports/templates/huabaosi-image-generation-staging-evidence.md",
    "This runbook is not production enablement",
    "enable a listener",
  ]) {
    requireFragment(stagingRuntimeProvisioningRunbookPath, runbook, fragment);
  }
  for (const fragment of [
    "QINTOPIA_HUABAOSI_IMAGE_API_KEY=",
    "QIWE_TOKEN=",
    "QIWE_GUID=",
    "postgres://",
    "postgresql://",
    "systemctl enable",
    "systemctl start",
    "gh release",
    'source "$',
    ". /etc/qintopia/message-sidecar-staging.env",
  ]) {
    forbidFragment(stagingRuntimeProvisioningRunbookPath, runbook, fragment);
  }
}

const aliangStagingSmokeTestPath = "tools/deploy/test-huabaosi-image-staging-smoke.mjs";
const aliangProductionCanaryTestPath =
  "tools/deploy/test-huabaosi-image-production-canary.mjs";
const huabaosiProductionCanaryEvidenceCheckPath =
  "tools/deploy/check-huabaosi-image-production-canary-evidence.mjs";
const huabaosiProductionCanaryEvidenceTestPath =
  "tools/deploy/test-huabaosi-image-production-canary-evidence.mjs";
const aliangStagingReadinessTestPath =
  "tools/deploy/test-huabaosi-image-staging-readiness.mjs";
const stagingRuntimePrerequisiteObservationTestPath =
  "tools/deploy/test-staging-runtime-prerequisite-observation.mjs";
const stagingRuntimeValuesObservationTestPath =
  "tools/deploy/test-staging-runtime-values-observation.mjs";
const stagingRuntimeReadinessEvidenceTestPath =
  "tools/deploy/test-staging-runtime-readiness-evidence.mjs";
const stagingRuntimeEnvRenderPath =
  "deploy/sidecar/scripts/render-staging-runtime-env.py";
const stagingRuntimeEnvRenderTestPath =
  "tools/deploy/test-staging-runtime-env-render.mjs";
if (!exists(stagingRuntimePrerequisiteObservationTestPath)) {
  addError(
    `${stagingRuntimePrerequisiteObservationTestPath}: missing staging runtime prerequisite observation test`
  );
} else {
  const test = readText(stagingRuntimePrerequisiteObservationTestPath);
  for (const fragment of [
    "staging-runtime-prerequisite-observation-smoke.sh",
    "staging_runtime_prerequisite_observation=",
    "ready_for_staging_readiness_smokes",
    "Staging runtime prerequisite observation smoke test passed.",
    "staging-prereq-secret-must-not-appear",
    "env_file_path_parent_is_symlink",
    "owner-executable observation should not fail",
    "non-executable observation should not fail",
    "sidecar_binary_path_not_executable",
    "sidecar_hash_mismatch",
  ]) {
    requireFragment(stagingRuntimePrerequisiteObservationTestPath, test, fragment);
  }
}
if (!exists(stagingRuntimeValuesObservationTestPath)) {
  addError(
    `${stagingRuntimeValuesObservationTestPath}: missing staging runtime values observation test`
  );
} else {
  const test = readText(stagingRuntimeValuesObservationTestPath);
  for (const fragment of [
    "staging-runtime-values-observation-smoke.sh",
    "staging_runtime_values_observation=",
    "ready_for_render_validation",
    "rendered_env_already_present",
    "Staging runtime values observation smoke test passed.",
    "staging-values-secret-must-not-appear",
    "values_file_path_parent_is_symlink",
    "values_file_path_parent_missing",
    "env_file_path_parent_missing",
    "renderer_path_parent_missing",
    "values_file_path_group_or_world_writable",
    "values_file_path_group_or_world_readable",
    "env_file_path_group_or_world_readable",
  ]) {
    requireFragment(stagingRuntimeValuesObservationTestPath, test, fragment);
  }
}
if (!exists(stagingRuntimeReadinessEvidenceTestPath)) {
  addError(
    `${stagingRuntimeReadinessEvidenceTestPath}: missing staging runtime readiness evidence test`
  );
} else {
  const test = readText(stagingRuntimeReadinessEvidenceTestPath);
  for (const fragment of [
    "staging-runtime-readiness-evidence-smoke.sh",
    "staging_runtime_readiness_evidence=",
    "ready_for_huabaosi_qiwe_staging_smokes",
    "staging-runtime-evidence-secret-must-not-appear",
    "QINTOPIA_STAGING_RUNTIME_DATABASE_URL_SHA256",
    "hash mismatch evidence is invalid",
    "expected missing staging database hash to fail",
    "Staging runtime readiness evidence smoke test passed.",
  ]) {
    requireFragment(stagingRuntimeReadinessEvidenceTestPath, test, fragment);
  }
}
if (!exists(stagingRuntimeEnvRenderPath)) {
  addError(`${stagingRuntimeEnvRenderPath}: missing staging runtime env renderer`);
} else {
  const script = readText(stagingRuntimeEnvRenderPath);
  for (const fragment of [
    "staging_runtime_env_render=",
    "approved-staging-runtime-env-provision",
    "/etc/qintopia/message-sidecar-staging.env",
    "message-sidecar-staging-values.json",
    "QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED",
    "QINTOPIA_HUABAOSI_IMAGE_STORAGE_BACKEND",
    "QINTOPIA_HUABAOSI_FEISHU_SCHEMA_VERSION",
    "QINTOPIA_HUABAOSI_FEISHU_ALLOWED_BASE_TOKENS",
    "QINTOPIA_HUABAOSI_FEISHU_ALLOWED_ARTIFACT_TABLE_IDS",
    "QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS",
    "media_host_count",
    "contains a duplicate host entry",
    "contains a port outside 1-65535",
    "QINTOPIA_QIWE_IMAGE_SEND_WEBHOOK_READY",
    "QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS must contain exactly one isolated group",
    "QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA must match QINTOPIA_DEPLOYED_COMMIT_SHA",
    "QINTOPIA_HUABAOSI_FEISHU_DATABASE_URL_SHA256 must match the approved staging database hash",
    "staging database URL hash does not match approved hash",
    "validate_protected_output_boundary",
    "os.lstat(component)",
    "protected output path component must not be a symlink",
    "protected output path component must be root-owned",
    "reject_existing_output",
    "output parent directory must not be a symlink",
    "output mode is 0600 on apply",
    "server-local values file is never printed",
    "no provider, media, Postgres, Feishu, QiWe, service, timer, or release action",
  ]) {
    requireFragment(stagingRuntimeEnvRenderPath, script, fragment);
  }
  for (const fragment of [
    "print(content)",
    "systemctl",
    "gh release",
    "subprocess",
    "requests",
    "urllib.request",
    "QINTOPIA_HUABAOSI_MEDIA_UPLOAD_ENDPOINT",
    "QINTOPIA_HUABAOSI_MEDIA_PUBLIC_BASE_URL",
  ]) {
    forbidFragment(stagingRuntimeEnvRenderPath, script, fragment);
  }
}
if (!exists(stagingRuntimeEnvRenderTestPath)) {
  addError(
    `${stagingRuntimeEnvRenderTestPath}: missing staging runtime env renderer test`
  );
} else {
  const test = readText(stagingRuntimeEnvRenderTestPath);
  for (const fragment of [
    "render-staging-runtime-env.py",
    "render-secret-must-not-appear",
    "staging_runtime_env_render=",
    "staging_env_render_ready",
    "staging_env_written",
    "unsupported keys",
    "hash does not match",
    "release SHA mismatch failure invalid",
    "duplicate host failure invalid",
    "invalid host port failure invalid",
    "QINTOPIA_HUABAOSI_IMAGE_STORAGE_BACKEND",
    "QINTOPIA_HUABAOSI_FEISHU_SCHEMA_VERSION",
    "QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS=media.example.test,cloud.example.test",
    "report.media_host_count !== 2",
    "non-test output guard invalid",
    "symlink parent guard invalid",
    "Staging runtime env render test passed.",
  ]) {
    requireFragment(stagingRuntimeEnvRenderTestPath, test, fragment);
  }
}
if (!exists(aliangStagingReadinessTestPath)) {
  addError(
    `${aliangStagingReadinessTestPath}: missing Huabaosi staging readiness test`
  );
} else {
  const test = readText(aliangStagingReadinessTestPath);
  for (const fragment of [
    "huabaosi-image-generation-staging-readiness-smoke.sh",
    "huabaosi_image_generation_staging_readiness=",
    "Huabaosi image staging readiness smoke test passed.",
    "readiness smoke exposed staging env contents",
    "expected owner-writable sidecar to fail readiness",
    "expected symlink parent path to fail readiness",
    "expected owner-executable sidecar to pass readiness",
    "expected non-executable sidecar to fail readiness",
    "env_file_path_parent_is_symlink",
    "sidecar_binary_path_not_executable",
    "expected sidecar hash mismatch to fail",
  ]) {
    requireFragment(aliangStagingReadinessTestPath, test, fragment);
  }
}

if (!exists(aliangStagingSmokeTestPath)) {
  addError(`${aliangStagingSmokeTestPath}: missing Huabaosi staging smoke test`);
} else {
  const test = readText(aliangStagingSmokeTestPath);
  for (const fragment of [
    "env file command was executed",
    "ambient secret reached child process",
    "staging-qiwe-token-must-not-reach-huabaosi-child",
    "staging database URL hash does not match the approved command",
    "staging env contains an unsupported key",
    "qintopia-agent-os-staging-releases/<approved 40-hex sha>",
    "contains forbidden sensitive output",
    "staging sidecar binary hash changed before",
    "sidecar_binary_sha256",
    "check-huabaosi-image-staging-evidence.mjs",
    "huabaosi_image_generation_staging_evidence=",
    "raw-huabaosi-staging-evidence.txt",
    "Huabaosi image staging smoke test passed.",
  ]) {
    requireFragment(aliangStagingSmokeTestPath, test, fragment);
  }
}

if (!exists(aliangProductionCanaryTestPath)) {
  addError(
    `${aliangProductionCanaryTestPath}: missing Huabaosi production canary test`
  );
} else {
  const test = readText(aliangProductionCanaryTestPath);
  for (const fragment of [
    "huabaosi-image-generation-production-canary-smoke.sh",
    '"reviewer_id":"trainer"',
    "expected five sidecar commands",
    "timer must be disabled during one-shot canary",
    "timer must be inactive during one-shot canary",
    "masked provider timer must block one-shot production canary",
    "static provider timer must block one-shot production canary",
    "test mode must be rejected from production release roots",
    "test mode must reject non-temporary sidecar paths",
    "test mode must reject symlink sidecar paths",
    "test mode must reject symlink env file paths",
    "ambient QiWe credential reached Huabaosi child",
    "invalid production canary brief UUID must fail closed",
    "missing trainer reviewer allowlist entry must fail closed",
    "starter parent work item mismatch must fail before generation",
    "duplicate production canary env key must fail closed",
    "revalidation identity mismatch must block canary completion",
    "sensitive child output must block production canary",
    "one Feishu-backed JPEG remains pending human review",
    "Huabaosi image production canary test passed.",
  ]) {
    requireFragment(aliangProductionCanaryTestPath, test, fragment);
  }
}

if (!exists(huabaosiProductionCanaryEvidenceCheckPath)) {
  addError(
    `${huabaosiProductionCanaryEvidenceCheckPath}: missing Huabaosi production canary evidence checker`
  );
} else {
  const checker = readText(huabaosiProductionCanaryEvidenceCheckPath);
  for (const fragment of [
    "huabaosi_image_generation_production_canary_evidence=",
    "Huabaosi production canary passed: one Feishu-backed JPEG remains pending human review; no generated-image approval, mirror, publish, QiWe, or send was executed",
    "preflight",
    "brief_review",
    "request_intake",
    "generation",
    "revalidation",
    "release_binary_verified",
    "approved_sidecar_sha256_matched",
    "approved_database_url_sha256_matched",
    'reviewer_id !== "trainer"',
    'generation.review_status !== "pending"',
    'generation.storage_backend !== "feishu-base"',
    "revalidation.database_writes_executed !== false",
    "revalidation.external_calls_executed !== true",
    '"artifact_uri"',
    '"provider_response"',
    '"target_group_id"',
    "authenticated same-byte readback",
    "Huabaosi image production canary evidence check passed.",
  ]) {
    requireFragment(huabaosiProductionCanaryEvidenceCheckPath, checker, fragment);
  }
}

if (!exists(huabaosiProductionCanaryEvidenceTestPath)) {
  addError(
    `${huabaosiProductionCanaryEvidenceTestPath}: missing Huabaosi production canary evidence checker test`
  );
} else {
  const test = readText(huabaosiProductionCanaryEvidenceTestPath);
  for (const fragment of [
    "check-huabaosi-image-production-canary-evidence.mjs",
    "hash-mismatch.txt",
    "raw-secret.txt",
    "missing-phase.txt",
    "request-drift.txt",
    "send-leak.txt",
    "missing-completion.txt",
    "mutable-boundary.txt",
    "sidecar-boundary.txt",
    "database-boundary.txt",
    "authenticated same-byte readback",
    "forbidden sensitive fragment",
    "exactly five fixed phase records",
    "does not bind the approved brief",
    "unexpected key",
    "Huabaosi image production canary evidence test passed.",
  ]) {
    requireFragment(huabaosiProductionCanaryEvidenceTestPath, test, fragment);
  }
}

const aliangProductionActivationPath =
  "deploy/sidecar/scripts/activate-huabaosi-image-generation-production.sh";
if (!exists(aliangProductionActivationPath)) {
  addError(`${aliangProductionActivationPath}: missing production activation command`);
} else {
  const activation = readText(aliangProductionActivationPath);
  for (const fragment of [
    "QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_ACTIVATION",
    "approved-production-image-generation",
    'PATH="/usr/bin:/bin:/usr/sbin:/sbin"',
    'SYSTEMCTL="/usr/bin/systemctl"',
    "qintopia-agentos-huabaosi-image-generation-preflight.service",
    "qintopia-agentos-huabaosi-image-generation-worker.timer",
    '"$SYSTEMCTL" start "$PREFLIGHT_SERVICE"',
    '"$SYSTEMCTL" enable --now "$WORKER_TIMER"',
    '"$SYSTEMCTL" is-enabled --quiet "$WORKER_TIMER"',
    '"$SYSTEMCTL" is-active --quiet "$WORKER_TIMER"',
  ]) {
    requireFragment(aliangProductionActivationPath, activation, fragment);
  }
  for (const fragment of [
    "run-huabaosi-image-generation-worker",
    "--apply",
    "source ",
    "QIWE_",
    "FEISHU_",
    'SYSTEMCTL="${SYSTEMCTL:-systemctl}"',
    'SYSTEMCTL="systemctl"',
  ]) {
    forbidFragment(aliangProductionActivationPath, activation, fragment);
  }
}

const aliangProductionRollbackPath =
  "deploy/sidecar/scripts/rollback-huabaosi-image-generation-production.sh";
if (!exists(aliangProductionRollbackPath)) {
  addError(`${aliangProductionRollbackPath}: missing production rollback command`);
} else {
  const rollback = readText(aliangProductionRollbackPath);
  for (const fragment of [
    "QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_ROLLBACK",
    "approved-production-image-generation-rollback",
    'PATH="/usr/bin:/bin:/usr/sbin:/sbin"',
    'SYSTEMCTL="/usr/bin/systemctl"',
    "qintopia-agentos-huabaosi-image-generation-worker.service",
    "qintopia-agentos-huabaosi-image-generation-worker.timer",
    '"$SYSTEMCTL" disable --now "$WORKER_TIMER"',
  ]) {
    requireFragment(aliangProductionRollbackPath, rollback, fragment);
  }
  for (const fragment of [
    "rm -",
    "source ",
    "QIWE_",
    "FEISHU_",
    'SYSTEMCTL="${SYSTEMCTL:-systemctl}"',
    'SYSTEMCTL="systemctl"',
  ]) {
    forbidFragment(aliangProductionRollbackPath, rollback, fragment);
  }
}

const huabaosiFeishuMirrorActivationPath =
  "deploy/sidecar/scripts/activate-huabaosi-feishu-artifact-mirror-production.sh";
if (!exists(huabaosiFeishuMirrorActivationPath)) {
  addError(
    `${huabaosiFeishuMirrorActivationPath}: missing Huabaosi Feishu mirror activation command`
  );
} else {
  const activation = readText(huabaosiFeishuMirrorActivationPath);
  for (const fragment of [
    "QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_ACTIVATION",
    "approved-production-huabaosi-feishu-artifact-mirror",
    'ENV_FILE="/etc/qintopia/message-sidecar.env"',
    'PATH="/usr/bin:/bin:/usr/sbin:/sbin"',
    'SYSTEMCTL="/usr/bin/systemctl"',
    "QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED",
    "qintopia-agentos-huabaosi-feishu-artifact-mirror-preflight.service",
    "qintopia-agentos-huabaosi-feishu-artifact-mirror-worker.timer",
    '"$SYSTEMCTL" start "$PREFLIGHT_SERVICE"',
    '"$SYSTEMCTL" enable --now "$WORKER_TIMER"',
    '"$SYSTEMCTL" is-enabled --quiet "$WORKER_TIMER"',
    '"$SYSTEMCTL" is-active --quiet "$WORKER_TIMER"',
    "requires exactly one persistent enablement flag",
  ]) {
    requireFragment(huabaosiFeishuMirrorActivationPath, activation, fragment);
  }
  for (const fragment of [
    "source ",
    'source "$',
    ". /etc/qintopia",
    "eval ",
    "run-huabaosi-feishu-artifact-mirror-worker",
    "--apply",
    "QIWE_",
    "QINTOPIA_SIDECAR_ENV_FILE",
    'SYSTEMCTL="${SYSTEMCTL:-systemctl}"',
    'SYSTEMCTL="systemctl"',
  ]) {
    forbidFragment(huabaosiFeishuMirrorActivationPath, activation, fragment);
  }
}

const qiweImageSendProductionActivationPath =
  "deploy/sidecar/scripts/activate-qiwe-image-send-production.sh";
if (!exists(qiweImageSendProductionActivationPath)) {
  addError(
    `${qiweImageSendProductionActivationPath}: missing production activation command`
  );
} else {
  const activation = readText(qiweImageSendProductionActivationPath);
  for (const fragment of [
    "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_ACTIVATION",
    "approved-production-qiwe-image-send",
    "QINTOPIA_QIWE_IMAGE_SEND_ENABLED",
    "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_APPROVAL",
    "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256",
    "QINTOPIA_SIDECAR_DATABASE_URL",
    'ENV_FILE="/etc/qintopia/message-sidecar.env"',
    'PATH="/usr/bin:/bin:/usr/sbin:/sbin"',
    'SYSTEMCTL="/usr/bin/systemctl"',
    'SHA256SUM="/usr/bin/sha256sum"',
    "database URL hash does not match the approved production hash",
    "qintopia-agentos-qiwe-image-send-preflight.service",
    "qintopia-agentos-qiwe-image-send-worker.timer",
    '"$SYSTEMCTL" start "$PREFLIGHT_SERVICE"',
    '"$SYSTEMCTL" enable --now "$WORKER_TIMER"',
    '"$SYSTEMCTL" is-enabled --quiet "$WORKER_TIMER"',
    '"$SYSTEMCTL" is-active --quiet "$WORKER_TIMER"',
  ]) {
    requireFragment(qiweImageSendProductionActivationPath, activation, fragment);
  }
  for (const fragment of [
    "run-qiwe-image-send-worker",
    "--apply",
    "source ",
    "eval ",
    "QIWE_TOKEN",
    "QINTOPIA_SIDECAR_ENV_FILE",
    'SYSTEMCTL="${SYSTEMCTL:-systemctl}"',
    'SYSTEMCTL="systemctl"',
    "sha256sum | awk",
  ]) {
    forbidFragment(qiweImageSendProductionActivationPath, activation, fragment);
  }
}

const qiweImageSendProductionRollbackPath =
  "deploy/sidecar/scripts/rollback-qiwe-image-send-production.sh";
if (!exists(qiweImageSendProductionRollbackPath)) {
  addError(
    `${qiweImageSendProductionRollbackPath}: missing production rollback command`
  );
} else {
  const rollback = readText(qiweImageSendProductionRollbackPath);
  for (const fragment of [
    "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_ROLLBACK",
    "approved-production-qiwe-image-send-rollback",
    "QINTOPIA_QIWE_IMAGE_SEND_ENABLED=0",
    'ENV_FILE="/etc/qintopia/message-sidecar.env"',
    'PATH="/usr/bin:/bin:/usr/sbin:/sbin"',
    'SYSTEMCTL="/usr/bin/systemctl"',
    "qintopia-agentos-qiwe-image-send-worker.service",
    "qintopia-agentos-qiwe-image-send-worker.timer",
    '"$SYSTEMCTL" disable --now "$WORKER_TIMER"',
  ]) {
    requireFragment(qiweImageSendProductionRollbackPath, rollback, fragment);
  }
  for (const fragment of [
    "rm -",
    "source ",
    "eval ",
    "QIWE_TOKEN",
    "QINTOPIA_SIDECAR_ENV_FILE",
    'SYSTEMCTL="${SYSTEMCTL:-systemctl}"',
    'SYSTEMCTL="systemctl"',
  ]) {
    forbidFragment(qiweImageSendProductionRollbackPath, rollback, fragment);
  }
}

const huabaosiFeishuMirrorRollbackPath =
  "deploy/sidecar/scripts/rollback-huabaosi-feishu-artifact-mirror-production.sh";
if (exists(huabaosiFeishuMirrorRollbackPath)) {
  const rollback = readText(huabaosiFeishuMirrorRollbackPath);
  requireFragment(
    huabaosiFeishuMirrorRollbackPath,
    rollback,
    'ENV_FILE="/etc/qintopia/message-sidecar.env"'
  );
  requireFragment(
    huabaosiFeishuMirrorRollbackPath,
    rollback,
    'PATH="/usr/bin:/bin:/usr/sbin:/sbin"'
  );
  requireFragment(
    huabaosiFeishuMirrorRollbackPath,
    rollback,
    'SYSTEMCTL="/usr/bin/systemctl"'
  );
  forbidFragment(
    huabaosiFeishuMirrorRollbackPath,
    rollback,
    "QINTOPIA_SIDECAR_ENV_FILE"
  );
  forbidFragment(
    huabaosiFeishuMirrorRollbackPath,
    rollback,
    'SYSTEMCTL="${SYSTEMCTL:-systemctl}"'
  );
  forbidFragment(huabaosiFeishuMirrorRollbackPath, rollback, 'SYSTEMCTL="systemctl"');
}

const renderSystemdUnitsPath = "deploy/sidecar/scripts/render-systemd-units.sh";
if (!exists(renderSystemdUnitsPath)) {
  addError(`${renderSystemdUnitsPath}: missing systemd unit renderer`);
} else {
  const renderer = readText(renderSystemdUnitsPath);
  for (const fragment of [
    "qintopia-agentos-qiwe-image-send-preflight.service",
    "qiwe-image-send-production-preflight",
    "qintopia-agentos-qiwe-image-send-worker.service",
    "run-qiwe-image-send-worker --once --apply",
    "qintopia-agentos-qiwe-image-send-worker.timer",
  ]) {
    requireFragment(renderSystemdUnitsPath, renderer, fragment);
  }
  forbidFragment(renderSystemdUnitsPath, renderer, '"qiwe-image-send-preflight"');
}

const qiweImageStagingSmokePath =
  "deploy/sidecar/scripts/qiwe-image-send-staging-smoke.sh";
const qiweImageStagingReadinessPath =
  "deploy/sidecar/scripts/qiwe-image-send-staging-readiness-smoke.sh";
if (!exists(qiweImageStagingReadinessPath)) {
  addError(`${qiweImageStagingReadinessPath}: missing QiWe staging readiness smoke`);
} else {
  const readiness = readText(qiweImageStagingReadinessPath);
  for (const fragment of [
    "QINTOPIA_QIWE_IMAGE_STAGING_READINESS_ENABLE",
    "QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL",
    "approved-staging-qiwe-image-send",
    "/etc/qintopia/message-sidecar-staging.env",
    "/home/ubuntu/qintopia-agent-os-staging-releases",
    "QINTOPIA_QIWE_IMAGE_STAGING_RELEASE_SHA",
    "QINTOPIA_QIWE_IMAGE_STAGING_SIDECAR_SHA256",
    "qiwe_image_send_staging_readiness=",
    "ready_for_staging_preflight",
    "staging env file contents are not read",
    "sidecar binary is not executed",
    "no QiWe, Postgres, Feishu, provider, media, service, or timer action",
    "path_is_secure",
    "require_executable",
    "os.access(path, os.X_OK)",
    "reject_owner_writable",
    "path_not_executable",
    "path_owner_group_or_world_writable",
    "path_group_or_world_writable",
    "path_is_symlink",
    "path_parent_is_symlink",
    "path_parent_group_or_world_writable",
    "path_parent_unexpected_owner",
    "path_unexpected_owner",
    "sidecar_hash_mismatch",
  ]) {
    requireFragment(qiweImageStagingReadinessPath, readiness, fragment);
  }
  for (const fragment of [
    "systemctl",
    "source ",
    'source "$',
    ". /etc/qintopia",
    "env -i",
    "QINTOPIA_SIDECAR_DATABASE_URL",
    "QIWE_TOKEN",
    "QIWE_GUID",
    "run-qiwe-image-send-worker",
    "process-qiwe-image-send-callback",
    "qiwe-image-send-staging-preflight",
    "subprocess",
    "curl ",
    "psql ",
  ]) {
    forbidFragment(qiweImageStagingReadinessPath, readiness, fragment);
  }
}

const qiweImageProductionObservationPath =
  "deploy/sidecar/scripts/qiwe-image-send-production-observation-smoke.sh";
if (!exists(qiweImageProductionObservationPath)) {
  addError(
    `${qiweImageProductionObservationPath}: missing QiWe production observation smoke`
  );
} else {
  const observation = readText(qiweImageProductionObservationPath);
  for (const fragment of [
    "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_OBSERVATION_ENABLE",
    "/home/ubuntu/qintopia-agent-os-releases/current",
    "/etc/qintopia/message-sidecar.env",
    "sidecar/qintopia-message-sidecar",
    "artifact-manifest.json",
    "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_OBSERVATION_TEST_MODE",
    "requires the fixed production env file",
    "requires the fixed production release/current path",
    "requires the real systemctl command",
    '"huabaosi-production-adapter"',
    '"huabaosi-feishu-mirror-adapter"',
    '"qiwe-production-adapter"',
    "QINTOPIA_QIWE_IMAGE_SEND_ENABLED",
    "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_APPROVAL",
    "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256",
    "parse_send_enablement",
    "expected state must be disabled, enabled, or auto",
    "production timer must be active",
    "production timer must not be active",
    "qiwe_image_send_production_observation_state=",
  ]) {
    requireFragment(qiweImageProductionObservationPath, observation, fragment);
  }
  for (const fragment of [
    "cargo run",
    'source "$',
    ". /etc/qintopia",
    "eval ",
    "env -i",
    "QINTOPIA_SIDECAR_DATABASE_URL",
    "QIWE_API_URL",
    "QIWE_TOKEN",
    "QIWE_GUID",
    "QINTOPIA_QIWE_IMAGE_SEND_ALLOWED_HOSTS",
    "QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS",
    "QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS",
    "run_sidecar_with_observation_env",
    '"$SIDECAR_BIN" qiwe-image-send-preflight',
    '"$SIDECAR_BIN" run-qiwe-image-send-worker',
    "--apply",
    "process-qiwe-image-send-callback",
  ]) {
    forbidFragment(qiweImageProductionObservationPath, observation, fragment);
  }
}

const stagingSidecarArtifactBuilderPath =
  "tools/deploy/build-staging-sidecar-artifact.mjs";
if (!exists(stagingSidecarArtifactBuilderPath)) {
  addError(
    `${stagingSidecarArtifactBuilderPath}: missing staging-only sidecar artifact builder`
  );
} else {
  const builder = readText(stagingSidecarArtifactBuilderPath);
  for (const fragment of [
    "qintopia-message-sidecar",
    "huabaosi-image-generation-staging-smoke.sh",
    "qiwe-image-send-staging-smoke.sh",
    "assertContainedArtifactDirBoundary",
    "resolveApprovedTarget",
    "resolveContainedArtifactDir",
    "staging-${targetTriple}",
    '"huabaosi-staging-adapter"',
    '"qiwe-staging-adapter"',
    '"--no-default-features"',
    '"--features"',
    'cargoFeatures.join(",")',
    "manifestSha256",
    "`${bundleSha256}  ${bundleName}`",
    "`${manifestSha256}  artifact-manifest.json`",
    "staging_only: true",
    "production_eligible: false",
    "staging-sidecar-artifact",
    "refusing to build a staging artifact from a dirty or unreadable git worktree",
    "/home/ubuntu/qintopia-agent-os-staging-releases/<approved 40-hex sha>",
  ]) {
    requireFragment(stagingSidecarArtifactBuilderPath, builder, fragment);
  }
  for (const fragment of [
    "huabaosi-production-adapter",
    "huabaosi-feishu-mirror-adapter",
    "qiwe-production-adapter",
    '"--all-features"',
  ]) {
    forbidFragment(stagingSidecarArtifactBuilderPath, builder, fragment);
  }
}

const productionSidecarArtifactBuilderPath = "tools/deploy/build-sidecar-artifact.mjs";
if (exists(productionSidecarArtifactBuilderPath)) {
  const builder = readText(productionSidecarArtifactBuilderPath);
  for (const fragment of [
    "assertContainedArtifactDirBoundary",
    "resolveApprovedTarget",
    "resolveContainedArtifactDir",
    "manifestSha256",
    '"huabaosi-production-adapter"',
    '"huabaosi-feishu-mirror-adapter"',
    '"qiwe-production-adapter"',
    "`${bundleSha256}  ${bundleName}`",
    "`${manifestSha256}  artifact-manifest.json`",
  ]) {
    requireFragment(productionSidecarArtifactBuilderPath, builder, fragment);
  }
  for (const fragment of ['"qiwe-staging-adapter"', '"huabaosi-staging-adapter"']) {
    forbidFragment(productionSidecarArtifactBuilderPath, builder, fragment);
  }
}

const sidecarArtifactBoundaryHelperPath =
  "tools/deploy/sidecar-artifact-build-boundary.mjs";
if (!exists(sidecarArtifactBoundaryHelperPath)) {
  addError(`${sidecarArtifactBoundaryHelperPath}: missing artifact path safety helper`);
} else {
  const helper = readText(sidecarArtifactBoundaryHelperPath);
  for (const fragment of [
    'const approvedTarget = "linux-x86_64-gnu"',
    "artifactNamePattern.test(artifactName)",
    "QINTOPIA_ARTIFACT_TARGET must be",
    'platform !== "linux"',
    'arch !== "x64"',
    "glibcVersionRuntime",
    "linux x64 GNU runners",
    'artifactName.includes("/")',
    'artifactName.includes("\\\\")',
    'artifactName.split("-").includes("..")',
    "fs.lstatSync(currentPath)",
    "stat.isSymbolicLink()",
    "fs.mkdirSync(resolvedRoot, { recursive: true })",
    "fs.realpathSync.native(currentPath)",
    "artifact output path must match its real path",
    "requireTerminalDirectory",
    "artifact output root must be a directory",
    "path.resolve(outputRoot)",
    "!resolvedDir.startsWith(`${resolvedRoot}${path.sep}`)",
  ]) {
    requireFragment(sidecarArtifactBoundaryHelperPath, helper, fragment);
  }
}

const artifactsWorkflowPath = ".github/workflows/artifacts.yml";
if (exists(artifactsWorkflowPath)) {
  const workflow = readText(artifactsWorkflowPath);
  for (const fragment of [
    "build_staging_sidecar",
    "build-staging-sidecar",
    "staging-sidecar-artifact:",
    "github.event_name == 'workflow_dispatch'",
    "node tools/deploy/build-staging-sidecar-artifact.mjs",
    "qintopia-message-sidecar-staging-linux-x86_64-gnu",
    "dist/sidecar-artifacts/qintopia-message-sidecar-staging-linux-x86_64-gnu",
    "Prune old staging sidecar artifacts",
  ]) {
    requireFragment(artifactsWorkflowPath, workflow, fragment);
  }
  const stagingJobStart = workflow.indexOf("  staging-sidecar-artifact:");
  const stagingJobEnd = workflow.indexOf("  deploy-bundle-artifact:", stagingJobStart);
  const stagingJob = workflow.slice(stagingJobStart, stagingJobEnd);
  if (stagingJob.includes("upload-cos-artifact.sh")) {
    addError(
      `${artifactsWorkflowPath}: staging sidecar artifact job must not upload to COS`
    );
  }
}

if (!exists(qiweImageStagingSmokePath)) {
  addError(`${qiweImageStagingSmokePath}: missing QiWe image-send staging smoke`);
} else {
  const smoke = readText(qiweImageStagingSmokePath);
  for (const fragment of [
    "QINTOPIA_QIWE_IMAGE_STAGING_SMOKE_ENABLE",
    "QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL",
    "approved-staging-qiwe-image-send",
    "QINTOPIA_QIWE_IMAGE_STAGING_ENV_FILE",
    "QINTOPIA_QIWE_IMAGE_STAGING_DATABASE_URL_SHA256",
    "QINTOPIA_QIWE_IMAGE_STAGING_RELEASE_SHA",
    "QINTOPIA_QIWE_IMAGE_STAGING_SIDECAR_SHA256",
    "QINTOPIA_QIWE_IMAGE_STAGING_WORK_ITEM_ID",
    "QINTOPIA_QIWE_IMAGE_STAGING_PHASE",
    'PHASE" != "preflight"',
    "QINTOPIA_QIWE_IMAGE_STAGING_PHASE must be preflight, upload, or callback",
    "QINTOPIA_QIWE_IMAGE_STAGING_SIDECAR_SHA256 must be a canonical SHA-256",
    "QINTOPIA_QIWE_IMAGE_STAGING_RELEASE_SHA must be a 40-character lowercase hex SHA",
    "QINTOPIA_QIWE_IMAGE_STAGING_SMOKE_TEST_MODE must be 0 or 1",
    "QiWe staging smoke must run from /home/ubuntu/qintopia-agent-os-staging-releases/<approved 40-hex sha>",
    "QiWe staging smoke test mode may read only a temporary fake env file",
    "QiWe staging smoke test mode requires a loopback fake database URL",
    "QiWe staging smoke test mode requires a fake loopback or example.test QiWe API host",
    "packaged sidecar/qintopia-message-sidecar is required for QiWe staging smoke",
    "verify_sidecar_binary",
    "packaged sidecar binary hash changed before",
    "packaged sidecar binary must stay under the fixed staging release root before",
    "packaged sidecar binary must come from /home/ubuntu/qintopia-agent-os-staging-releases/<approved 40-hex sha> before",
    "qintopia-agent-os-staging-releases",
    "packaged sidecar binary, parent directory, release root, and staging releases root must not be symlinks before",
    "packaged sidecar release ancestors, parent directory, and binary must keep the expected file types before",
    "packaged sidecar release ancestors, parent directory, and binary must be executable before",
    "packaged sidecar release ancestors, parent directory, and binary must be owned by root or the staging runner user before",
    "packaged sidecar binary and parent directory must not be owner/group/world writable, and release ancestors must not be group/world writable before",
    "unexpected_owner",
    "os.geteuid()",
    "sidecar_binary_sha256",
    "artifact_content_hash",
    "feishu_delivery_bridge_compiled",
    "qiwe-image-send-staging-preflight",
    "run-qiwe-image-send-worker",
    "process-qiwe-image-send-callback",
    "image_upload_accepted",
    "image_send_completed",
    'payload["external_send_executed"] is True',
    "callback_credential_schema",
    "contains forbidden sensitive output",
    "CHILD_ENV",
    "add_child_env",
    "add_child_env_if_set",
    "env -i",
    'verify_sidecar_binary "$label spawn"',
    'output="$(env -i "${CHILD_ENV[@]}" "$@" 2>&1)"',
    'assert_no_sensitive_text "$label output" "$output"',
    "SANITIZED_OUTPUT",
    "qiwe_image_send_staging_evidence=",
    "emit_sanitized_evidence",
    "payload = json.load(sys.stdin)",
    "fileAesKey",
    "fileAeskey",
    "fileId",
    "fileMd5",
    "fileSize",
    "requestId",
    "STAGING_ENV_KEYS",
    "QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS",
    "load_staging_env",
    "qiwe-image-send-staging-preflight </dev/null",
    "--apply </dev/null",
  ]) {
    requireFragment(qiweImageStagingSmokePath, smoke, fragment);
  }
  for (const fragment of [
    "systemctl",
    "callback.json",
    "run-group-message-send-worker",
    "operations-group-message-confirm",
    "--use-feishu-base",
    'source "$ENV_FILE"',
    '>"$stdout_file"',
    '2>"$stderr_file"',
    "mktemp",
    "report_file",
    "preflight_output",
    "phase_output",
    "--features qiwe-staging-adapter",
    "cargo run",
    "source_fallback",
    "QINTOPIA_SIDECAR_BIN",
    "SANITIZED_EVIDENCE_PAYLOAD",
    "json.loads(os.environ",
  ]) {
    forbidFragment(qiweImageStagingSmokePath, smoke, fragment);
  }
}

const qiweImageStagingSmokeTestPath = "tools/deploy/test-qiwe-image-staging-smoke.mjs";
if (!exists(qiweImageStagingSmokeTestPath)) {
  addError(`${qiweImageStagingSmokeTestPath}: missing QiWe staging smoke test`);
} else {
  const test = readText(qiweImageStagingSmokeTestPath);
  for (const fragment of [
    "QINTOPIA_UNRELATED_RUNTIME_SECRET",
    "ambient secret reached child process",
    "expected source checkout staging smoke to fail closed",
    "source checkout failure did not enforce fixed release root",
    "tamper-after-preflight",
    "expected sidecar tampering before upload spawn to fail",
    "before QiWe staging upload spawn",
    "QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS=media.example.test",
    "QiWe image-send staging smoke test passed.",
  ]) {
    requireFragment(qiweImageStagingSmokeTestPath, test, fragment);
  }
}

const qiweImageStagingReadinessTestPath =
  "tools/deploy/test-qiwe-image-staging-readiness.mjs";
if (!exists(qiweImageStagingReadinessTestPath)) {
  addError(`${qiweImageStagingReadinessTestPath}: missing QiWe staging readiness test`);
} else {
  const test = readText(qiweImageStagingReadinessTestPath);
  for (const fragment of [
    "QiWe image-send staging readiness smoke test passed.",
    "expected missing readiness inputs to fail",
    "expected owner-writable sidecar to fail readiness",
    "expected non-executable sidecar to fail readiness",
    "expected owner-executable sidecar to pass readiness",
    "expected symlink parent path to fail readiness",
    "ready_for_staging_preflight",
    "readiness smoke exposed staging env contents",
    "env_file_path_parent_is_symlink",
    "sidecar_binary_path_not_executable",
    "sidecar_hash_mismatch",
  ]) {
    requireFragment(qiweImageStagingReadinessTestPath, test, fragment);
  }
}

const qiweImageStagingRunbookPath =
  "docs/operations/qiwe-image-send-staging-runbook.md";
if (!exists(qiweImageStagingRunbookPath)) {
  addError(`${qiweImageStagingRunbookPath}: missing QiWe staging runbook`);
} else {
  const runbook = readText(qiweImageStagingRunbookPath);
  for (const fragment of [
    "QINTOPIA_QIWE_IMAGE_STAGING_READINESS_ENABLE=1",
    "QINTOPIA_QIWE_IMAGE_STAGING_SMOKE_ENABLE=1",
    "QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL=approved-staging-qiwe-image-send",
    "QINTOPIA_QIWE_IMAGE_STAGING_RELEASE_SHA='<approved staging release sha>'",
    "deploy/sidecar/scripts/qiwe-image-send-staging-readiness-smoke.sh",
    "does not read the env file contents",
    "execute the sidecar, connect to",
    "Postgres, call QiWe, or touch services",
    "QINTOPIA_QIWE_IMAGE_STAGING_PHASE=preflight",
    "QINTOPIA_QIWE_IMAGE_STAGING_PHASE=upload",
    "QINTOPIA_QIWE_IMAGE_STAGING_PHASE=callback",
    "QINTOPIA_QIWE_IMAGE_STAGING_ENV_FILE=/etc/qintopia/message-sidecar-staging.env",
    "QINTOPIA_QIWE_IMAGE_STAGING_DATABASE_URL_SHA256='<approved staging database URL sha256>'",
    "QINTOPIA_QIWE_IMAGE_STAGING_RELEASE_SHA='<approved staging release sha>'",
    "QINTOPIA_QIWE_IMAGE_STAGING_RELEASE_SHA='<same approved staging release sha>'",
    "QINTOPIA_QIWE_IMAGE_STAGING_SIDECAR_SHA256='<approved staging sidecar binary sha256>'",
    "QINTOPIA_QIWE_IMAGE_STAGING_WORK_ITEM_ID='<approved send-ready UUID>'",
    "QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS",
    "trusted-staging-callback-source |",
    "callback credential schema id",
    "qiwe_image_send_staging_evidence=<json>",
    "node tools/deploy/check-qiwe-image-staging-evidence.mjs <staging-evidence-output.txt>",
    "node tools/deploy/check-qiwe-image-staging-evidence.mjs --preflight-only <preflight-evidence-output.txt>",
    "node tools/deploy/check-xiaoman-image-send-staging-evidence.mjs <huabaosi-staging-evidence-output.txt> <qiwe-staging-evidence-output.txt>",
    "external_send_executed",
    "artifact_content_hash",
    "QINTOPIA_QIWE_IMAGE_SEND_ENABLED=0",
    "Do not add production listener, service, timer, or release activation",
  ]) {
    requireFragment(qiweImageStagingRunbookPath, runbook, fragment);
  }
  for (const fragment of [
    'source "$ENV_FILE"',
    ". /etc/qintopia/message-sidecar-staging.env",
    "callback.json",
    "QIWE_TOKEN=",
    "QIWE_GUID=",
    "systemctl enable",
    "systemctl start",
    "gh release",
  ]) {
    forbidFragment(qiweImageStagingRunbookPath, runbook, fragment);
  }
}

const qiweImageStagingEvidenceCheckPath =
  "tools/deploy/check-qiwe-image-staging-evidence.mjs";
if (!exists(qiweImageStagingEvidenceCheckPath)) {
  addError(
    `${qiweImageStagingEvidenceCheckPath}: missing QiWe staging evidence checker`
  );
} else {
  const checker = readText(qiweImageStagingEvidenceCheckPath);
  for (const fragment of [
    "qiwe_image_send_staging_evidence=",
    "--preflight-only",
    "complete evidence requires preflight, upload, and callback records",
    "upload and callback work_item_id values differ",
    "upload and callback artifact_content_hash values differ",
    "forbidden sensitive fragment appeared in evidence",
    "unexpected non-evidence line",
    "callback_credential_schema",
    "artifact_content_hash",
    "sidecar_binary_sha256",
    "feishu_delivery_bridge_compiled",
    "external_send_executed",
    "image_send_completed",
    "complete evidence records must use the same sidecar binary hash",
  ]) {
    requireFragment(qiweImageStagingEvidenceCheckPath, checker, fragment);
  }
  for (const fragment of ["fetch(", "systemctl", "process.env.QIWE_TOKEN"]) {
    forbidFragment(qiweImageStagingEvidenceCheckPath, checker, fragment);
  }
}

const xiaomanImageSendStagingEvidenceCheckPath =
  "tools/deploy/check-xiaoman-image-send-staging-evidence.mjs";
if (!exists(xiaomanImageSendStagingEvidenceCheckPath)) {
  addError(
    `${xiaomanImageSendStagingEvidenceCheckPath}: missing Xiaoman image-send staging evidence checker`
  );
} else {
  const checker = readText(xiaomanImageSendStagingEvidenceCheckPath);
  for (const fragment of [
    "huabaosi_image_generation_staging_evidence=",
    "qiwe_image_send_staging_evidence=",
    "Huabaosi content_hash and QiWe artifact_content_hash values differ",
    "QiWe upload and callback artifact_content_hash values differ",
    "Huabaosi and QiWe sidecar_binary_sha256 values differ",
    "QiWe preflight evidence does not prove staging send readiness",
    "Huabaosi preflight evidence does not prove staging adapter readiness",
    "forbidden sensitive fragment",
    "image_send_completed",
    "generated_image_created",
    "Xiaoman image-send staging evidence check passed.",
  ]) {
    requireFragment(xiaomanImageSendStagingEvidenceCheckPath, checker, fragment);
  }
  for (const fragment of ["fetch(", "systemctl", "process.env.QIWE_TOKEN"]) {
    forbidFragment(xiaomanImageSendStagingEvidenceCheckPath, checker, fragment);
  }
}

const xiaomanRealActivityProductionEvidenceCheckPath =
  "tools/deploy/check-xiaoman-real-activity-production-evidence.mjs";
if (!exists(xiaomanRealActivityProductionEvidenceCheckPath)) {
  addError(
    `${xiaomanRealActivityProductionEvidenceCheckPath}: missing Xiaoman real activity production evidence checker`
  );
} else {
  const checker = readText(xiaomanRealActivityProductionEvidenceCheckPath);
  for (const fragment of [
    "xiaoman_real_activity_production_evidence=",
    "signal_intake",
    "image_generation",
    "human_approval",
    "send_ready",
    "qiwe_upload",
    "qiwe_callback_send",
    "sanitized_evidence_retention",
    "artifact_content_hash",
    "callback_credential_schema",
    "raw_secret_fields_retained",
    "release_binary_verified",
    "approved_sidecar_sha256_matched",
    "approved_database_url_sha256_matched",
    "forbidden sensitive fragment",
    "Xiaoman real activity production evidence check passed.",
  ]) {
    requireFragment(xiaomanRealActivityProductionEvidenceCheckPath, checker, fragment);
  }
  for (const fragment of ["fetch(", "systemctl", "process.env.QIWE_TOKEN"]) {
    forbidFragment(xiaomanRealActivityProductionEvidenceCheckPath, checker, fragment);
  }
}

const xiaomanQiweArrivalConfirmationEvidenceCheckPath =
  "tools/deploy/check-xiaoman-qiwe-group-arrival-confirmation-evidence.mjs";
if (!exists(xiaomanQiweArrivalConfirmationEvidenceCheckPath)) {
  addError(
    `${xiaomanQiweArrivalConfirmationEvidenceCheckPath}: missing Xiaoman QiWe group arrival confirmation evidence checker`
  );
} else {
  const checker = readText(xiaomanQiweArrivalConfirmationEvidenceCheckPath);
  for (const fragment of [
    "xiaoman_qiwe_group_arrival_confirmation_evidence=",
    "xiaoman-qiwe-group-arrival-confirmation-evidence-v1",
    "check-xiaoman-real-activity-production-evidence.mjs",
    "human_visible_group_check",
    "community_activity_group",
    "send_ready_work_item_id",
    "generated_image_artifact_id",
    "artifact_content_hash",
    "raw_secret_fields_retained",
    "forbidden sensitive fragment",
    "Xiaoman QiWe group arrival confirmation evidence check passed.",
  ]) {
    requireFragment(xiaomanQiweArrivalConfirmationEvidenceCheckPath, checker, fragment);
  }
  for (const fragment of ["fetch(", "systemctl", "process.env.QIWE_TOKEN"]) {
    forbidFragment(xiaomanQiweArrivalConfirmationEvidenceCheckPath, checker, fragment);
  }
}

const xiaomanProductionCompletionEvidenceCheckPath =
  "tools/deploy/check-xiaoman-production-completion-evidence.mjs";
if (!exists(xiaomanProductionCompletionEvidenceCheckPath)) {
  addError(
    `${xiaomanProductionCompletionEvidenceCheckPath}: missing Xiaoman production completion evidence checker`
  );
} else {
  const checker = readText(xiaomanProductionCompletionEvidenceCheckPath);
  for (const fragment of [
    "xiaoman-production-completion-evidence-v1",
    "check-huabaosi-image-staging-evidence.mjs",
    "check-qiwe-image-staging-evidence.mjs",
    "check-xiaoman-image-send-staging-evidence.mjs",
    "check-xiaoman-real-activity-production-evidence.mjs",
    "check-xiaoman-qiwe-group-arrival-confirmation-evidence.mjs",
    "staging_runtime_readiness_evidence=",
    "huabaosi_image_generation_production_canary_evidence=",
    "--huabaosi-production-canary",
    "--qiwe-group-arrival-confirmation",
    "xiaoman_qiwe_group_arrival_confirmation_evidence=",
    "does not bind to QiWe group arrival evidence",
    "prerequisite",
    "huabaosi_readiness",
    "qiwe_readiness",
    "isUtcSecondTimestamp",
    "qiwe_production_enablement",
    "release_tag",
    "released_commit_sha",
    "included_in_release_sha",
    "release facts do not bind to the deployed production release",
    "listener_service_timer_reviewed",
    "production_feature_boundary_reviewed",
    "huabaosi_production_activation",
    "first_record_evidence_retained",
    "brief_review",
    "request_intake",
    "feishu_primary_storage_revalidated",
    "Huabaosi production canary first record does not bind to real activity image",
    "qiwe_group_arrival_confirmed",
    "release_binary_verified",
    "approved_sidecar_sha256_matched",
    "approved_database_url_sha256_matched",
    "Xiaoman production completion evidence check passed.",
  ]) {
    requireFragment(xiaomanProductionCompletionEvidenceCheckPath, checker, fragment);
  }
  for (const fragment of ["fetch(", "systemctl", "process.env.QIWE_TOKEN"]) {
    forbidFragment(xiaomanProductionCompletionEvidenceCheckPath, checker, fragment);
  }
}

const xiaomanProductionCompletionManifestBuilderPath =
  "tools/deploy/build-xiaoman-production-completion-manifest.mjs";
if (!exists(xiaomanProductionCompletionManifestBuilderPath)) {
  addError(
    `${xiaomanProductionCompletionManifestBuilderPath}: missing Xiaoman production completion manifest builder`
  );
} else {
  const builder = readText(xiaomanProductionCompletionManifestBuilderPath);
  for (const fragment of [
    "xiaoman-production-completion-evidence-v1",
    "check-huabaosi-image-production-canary-evidence.mjs",
    "check-xiaoman-real-activity-production-evidence.mjs",
    "check-xiaoman-qiwe-group-arrival-confirmation-evidence.mjs",
    "assertGithubReleaseFacts(options)",
    "gh",
    "pr",
    "view",
    "api",
    "statusCheckRollup",
    "mergeCommit",
    "Release Please validation",
    "Published GitHub Release",
    "Published Git tag ref",
    "Published annotated Git tag",
    "QiWe production enablement inclusion",
    "releases/tags/${options.releaseTag}",
    "git/ref/tags/${options.releaseTag}",
    "draft",
    "prerelease",
    "compare/${options.qiweProductionEnablementHeadSha}...${options.releasedCommitSha}",
    "huabaosi_image_generation_production_canary_evidence=",
    "xiaoman_real_activity_production_evidence=",
    "xiaoman_qiwe_group_arrival_confirmation_evidence=",
    "--release-please-pr-number",
    "--release-please-head-sha",
    "--release-tag",
    "--released-commit-sha",
    "--qiwe-production-enablement-pr-number",
    "--qiwe-production-enablement-head-sha",
    "--huabaosi-production-canary",
    "--production-real-activity",
    "--qiwe-group-arrival-confirmation",
    "assertNoSensitiveOutput(output)",
    "forbiddenOutputPatterns",
    "released_commit_sha",
    "release_tag",
    "included_in_release_sha",
    "qiwe_group_arrival_confirmed",
    "safeDiagnostic",
    "redacted-sensitive-diagnostic",
    "forbiddenOutputPatterns.some",
    "must be merged in GitHub before manifest generation",
    "does not match GitHub state",
    "published GitHub Release tag does not point to the released commit SHA",
    "is not included in the released commit SHA",
  ]) {
    requireFragment(xiaomanProductionCompletionManifestBuilderPath, builder, fragment);
  }
  for (const fragment of [
    "fetch(",
    "systemctl",
    "process.env.QIWE_TOKEN",
    "process.env.DATABASE_URL",
    "child_process.exec",
    "https://",
    "postgres://",
    "postgresql://",
  ]) {
    forbidFragment(xiaomanProductionCompletionManifestBuilderPath, builder, fragment);
  }
}

const xiaomanTextAnnouncementMvpPath =
  "docs/plans/active/xiaoman-text-announcement-mvp.md";
if (!exists(xiaomanTextAnnouncementMvpPath)) {
  addError(`${xiaomanTextAnnouncementMvpPath}: missing Xiaoman text MVP plan`);
} else {
  const plan = readText(xiaomanTextAnnouncementMvpPath);
  for (const fragment of [
    "No QiWe call, group delivery, publish, or send-ready mutation.",
    "The request",
    "remains `awaiting_publish`",
    "external_send_executed=false",
    "not a production-complete evidence path",
    "does not prove image generation",
    "Huabaosi/Feishu approval, QiWe adapter execution, visible group arrival",
    "production completion retention",
    "check-xiaoman-real-activity-production-evidence.mjs",
    "check-xiaoman-qiwe-group-arrival-confirmation-evidence.mjs",
    "full completion",
    "manifest checker",
    "cannot be reported as Xiaoman production-complete or QiWe group-delivered",
    "evidence.",
  ]) {
    requireFragment(xiaomanTextAnnouncementMvpPath, plan, fragment);
  }
  for (const fragment of [
    "systemctl enable",
    "systemctl start",
    "gh release",
    "QIWE_TOKEN=",
    "QIWE_GUID=",
    "postgres://",
    "postgresql://",
  ]) {
    forbidFragment(xiaomanTextAnnouncementMvpPath, plan, fragment);
  }
}

const xiaomanRealActivityEvidenceRuntimePath =
  "runtime/sidecar/src/xiaoman_real_activity_evidence.rs";
if (!exists(xiaomanRealActivityEvidenceRuntimePath)) {
  addError(
    `${xiaomanRealActivityEvidenceRuntimePath}: missing Xiaoman production evidence exporter`
  );
} else {
  const source = readText(xiaomanRealActivityEvidenceRuntimePath);
  for (const fragment of [
    "xiaoman_real_activity_production_evidence=",
    "QINTOPIA_DEPLOYED_COMMIT_SHA",
    "QINTOPIA_XIAOMAN_REAL_ACTIVITY_PRODUCTION_SIDECAR_SHA256",
    "QINTOPIA_XIAOMAN_REAL_ACTIVITY_PRODUCTION_DATABASE_URL_SHA256",
    "/home/ubuntu/qintopia-agent-os-releases/current",
    "current_exe",
    "canonicalize",
    "owner-approved SHA-256",
    "configured database URL does not match the owner-approved SHA-256",
    "database_url_sha256",
    "release_binary_verified",
    "approved_sidecar_sha256_matched",
    "approved_database_url_sha256_matched",
    "signal_intake",
    "image_generation",
    "human_approval",
    "send_ready",
    "qiwe_upload",
    "qiwe_callback_send",
    "sanitized_evidence_retention",
    "callback_credential_schema",
    "target_group_alias",
    "community_activity_group",
  ]) {
    requireFragment(xiaomanRealActivityEvidenceRuntimePath, source, fragment);
  }
  for (const fragment of [
    "reqwest",
    "HttpClient",
    "systemctl",
    "INSERT INTO",
    "UPDATE qintopia_agent_os",
    'target_group_id":',
    'artifact_uri":',
    'provider_message_id_sha256":',
  ]) {
    forbidFragment(xiaomanRealActivityEvidenceRuntimePath, source, fragment);
  }
}

const qiweImageStagingEvidenceTemplatePath =
  "docs/reports/templates/qiwe-image-send-staging-evidence.md";
if (!exists(qiweImageStagingEvidenceTemplatePath)) {
  addError(
    `${qiweImageStagingEvidenceTemplatePath}: missing QiWe staging evidence template`
  );
} else {
  const template = readText(qiweImageStagingEvidenceTemplatePath);
  for (const fragment of [
    "node tools/deploy/check-qiwe-image-staging-evidence.mjs <staging-evidence-output.txt>",
    "Repository commit SHA",
    "Packaged sidecar binary SHA-256",
    "Staging database URL SHA-256",
    "Work item UUID",
    "Final JPEG `artifact_content_hash`",
    "Target group allowlist: isolated single group confirmed, identifier not recorded.",
    "Rollback owner",
    "Rollback action",
    "External upload requested",
    "External send executed",
    "sidecar_binary_sha256",
    "artifact_content_hash",
    "callback_credential_schema",
    "callback_additional_field_count",
    "Complete evidence checker mode passed",
    "Cross-flow Huabaosi/QiWe hash checker passed",
    "Production enablement PR allowed",
    "Do not record QiWe token, GUID, API secret material, target group id, database URL",
  ]) {
    requireFragment(qiweImageStagingEvidenceTemplatePath, template, fragment);
  }
  for (const fragment of [
    "QIWE_TOKEN=",
    "QIWE_GUID=",
    "postgres://",
    "postgresql://",
    "callback.json",
    "systemctl enable",
    "systemctl start",
    "gh release",
  ]) {
    forbidFragment(qiweImageStagingEvidenceTemplatePath, template, fragment);
  }
}

const xiaomanImageSendStagingEvidenceTemplatePath =
  "docs/reports/templates/xiaoman-image-send-staging-evidence.md";
if (!exists(xiaomanImageSendStagingEvidenceTemplatePath)) {
  addError(
    `${xiaomanImageSendStagingEvidenceTemplatePath}: missing Xiaoman image-send staging evidence template`
  );
} else {
  const template = readText(xiaomanImageSendStagingEvidenceTemplatePath);
  for (const fragment of [
    "node tools/deploy/check-huabaosi-image-staging-evidence.mjs <huabaosi-staging-evidence-output.txt>",
    "node tools/deploy/check-qiwe-image-staging-evidence.mjs <qiwe-staging-evidence-output.txt>",
    "node tools/deploy/check-xiaoman-image-send-staging-evidence.mjs <huabaosi-staging-evidence-output.txt> <qiwe-staging-evidence-output.txt>",
    "Huabaosi image request work item UUID",
    "QiWe send-ready work item UUID",
    "Final JPEG `content_hash`",
    "QiWe `artifact_content_hash`",
    "Huabaosi `sidecar_binary_sha256`",
    "Hash match confirmed by `check-xiaoman-image-send-staging-evidence.mjs`",
    "Huabaosi staging readiness",
    "Huabaosi staging smoke",
    "QiWe staging readiness",
    "QiWe preflight phase",
    "QiWe upload phase",
    "QiWe callback phase",
    "Xiaoman image-send staging evidence check passed.",
    "callback_credential_schema",
    "callback_additional_field_count",
    "external_upload_requested=true",
    "external_send_executed=true",
    "QiWe production enablement PR allowed",
    "no production listener, service, timer, feature build, Feishu write, Release",
    "Do not record QiWe token, GUID, API secret material, target group id, database URL",
  ]) {
    requireFragment(xiaomanImageSendStagingEvidenceTemplatePath, template, fragment);
  }
  for (const fragment of [
    "QIWE_TOKEN=",
    "QIWE_GUID=",
    "postgres://",
    "postgresql://",
    "callback.json",
    "systemctl enable",
    "systemctl start",
    "gh release",
  ]) {
    forbidFragment(xiaomanImageSendStagingEvidenceTemplatePath, template, fragment);
  }
}

const xiaomanRealActivityProductionEvidenceTemplatePath =
  "docs/reports/templates/xiaoman-real-activity-production-evidence.md";
if (!exists(xiaomanRealActivityProductionEvidenceTemplatePath)) {
  addError(
    `${xiaomanRealActivityProductionEvidenceTemplatePath}: missing Xiaoman real activity production evidence template`
  );
} else {
  const template = readText(xiaomanRealActivityProductionEvidenceTemplatePath);
  for (const fragment of [
    "QINTOPIA_XIAOMAN_REAL_ACTIVITY_PRODUCTION_SIDECAR_SHA256='<approved production sidecar binary sha256>'",
    "QINTOPIA_XIAOMAN_REAL_ACTIVITY_PRODUCTION_DATABASE_URL_SHA256='<approved production database URL sha256>'",
    "qintopia-message-sidecar xiaoman-real-activity-production-evidence",
    "--workflow-root-id <completed-xiaoman-activity-root-uuid>",
    "node tools/deploy/check-xiaoman-real-activity-production-evidence.mjs <production-evidence-output.txt>",
    "Production release SHA",
    "Release-local binary verified",
    "Owner-approved sidecar SHA-256 matched",
    "Production database URL SHA-256",
    "Owner-approved database URL SHA-256 matched",
    "Xiaoman source event signal UUID",
    "Generated-image artifact UUID",
    "Send-ready work item UUID",
    "Final JPEG `artifact_content_hash`",
    "QiWe group arrival confirmed by human operator",
    "signal_intake",
    "image_generation",
    "human_approval",
    "send_ready",
    "qiwe_upload",
    "qiwe_callback_send",
    "sanitized_evidence_retention",
    "`release_binary_verified`: `true`",
    "`approved_sidecar_sha256_matched`: `true`",
    "`approved_database_url_sha256_matched`: `true`",
    "Do not record QiWe token, GUID, API secret material, target group id, database URL",
  ]) {
    requireFragment(
      xiaomanRealActivityProductionEvidenceTemplatePath,
      template,
      fragment
    );
  }
  for (const fragment of [
    "QIWE_TOKEN=",
    "QIWE_GUID=",
    "postgres://",
    "postgresql://",
    "callback.json",
    "systemctl enable",
    "systemctl start",
    "gh release",
  ]) {
    forbidFragment(
      xiaomanRealActivityProductionEvidenceTemplatePath,
      template,
      fragment
    );
  }
}

const xiaomanQiweArrivalConfirmationTemplatePath =
  "docs/reports/templates/xiaoman-qiwe-group-arrival-confirmation-evidence.md";
if (!exists(xiaomanQiweArrivalConfirmationTemplatePath)) {
  addError(
    `${xiaomanQiweArrivalConfirmationTemplatePath}: missing Xiaoman QiWe group arrival confirmation evidence template`
  );
} else {
  const template = readText(xiaomanQiweArrivalConfirmationTemplatePath);
  for (const fragment of [
    "xiaoman_qiwe_group_arrival_confirmation_evidence=",
    "xiaoman-qiwe-group-arrival-confirmation-evidence-v1",
    "human_visible_group_check",
    "community_activity_group",
    "send_ready_work_item_id",
    "generated_image_artifact_id",
    "artifact_content_hash",
    "raw_secret_fields_retained",
    "Do not record QiWe token, GUID",
  ]) {
    requireFragment(xiaomanQiweArrivalConfirmationTemplatePath, template, fragment);
  }
  for (const fragment of [
    "QIWE_TOKEN=",
    "QIWE_GUID=",
    "postgres://",
    "postgresql://",
    "callback.json",
    "systemctl enable",
    "systemctl start",
    "gh release",
  ]) {
    forbidFragment(xiaomanQiweArrivalConfirmationTemplatePath, template, fragment);
  }
}

const xiaomanImageSendStagingEvidenceTestPath =
  "tools/deploy/test-xiaoman-image-send-staging-evidence.mjs";
if (!exists(xiaomanImageSendStagingEvidenceTestPath)) {
  addError(
    `${xiaomanImageSendStagingEvidenceTestPath}: missing Xiaoman image-send staging evidence checker test`
  );
} else {
  const test = readText(xiaomanImageSendStagingEvidenceTestPath);
  for (const fragment of [
    "check-xiaoman-image-send-staging-evidence.mjs",
    "Huabaosi content_hash and QiWe artifact_content_hash values differ",
    "Huabaosi and QiWe sidecar_binary_sha256 values differ",
    "expected exactly one QiWe preflight evidence record",
    "forbidden sensitive fragment",
    "Xiaoman image-send staging evidence test passed.",
  ]) {
    requireFragment(xiaomanImageSendStagingEvidenceTestPath, test, fragment);
  }
}

const xiaomanQiweArrivalConfirmationEvidenceTestPath =
  "tools/deploy/test-xiaoman-qiwe-group-arrival-confirmation-evidence.mjs";
if (!exists(xiaomanQiweArrivalConfirmationEvidenceTestPath)) {
  addError(
    `${xiaomanQiweArrivalConfirmationEvidenceTestPath}: missing Xiaoman QiWe group arrival confirmation evidence checker test`
  );
} else {
  const test = readText(xiaomanQiweArrivalConfirmationEvidenceTestPath);
  for (const fragment of [
    "check-xiaoman-qiwe-group-arrival-confirmation-evidence.mjs",
    "does not bind to the real activity send",
    "forbidden sensitive fragment",
    "production real activity evidence failed",
    "Xiaoman QiWe group arrival confirmation evidence test passed.",
  ]) {
    requireFragment(xiaomanQiweArrivalConfirmationEvidenceTestPath, test, fragment);
  }
}

for (const relativePath of [
  "deploy/sidecar/README.md",
  "docs/plans/active/xiaoman-qiwe-image-send.md",
]) {
  const text = readText(relativePath);
  for (const fragment of [
    "QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL=approved-staging-qiwe-image-send",
    "QINTOPIA_QIWE_IMAGE_STAGING_PHASE=preflight",
    "QINTOPIA_QIWE_IMAGE_STAGING_PHASE=upload",
    "QINTOPIA_QIWE_IMAGE_STAGING_PHASE=callback",
    "QINTOPIA_QIWE_IMAGE_STAGING_SIDECAR_SHA256='<approved staging sidecar binary sha256>'",
    "trusted-staging-callback-source |",
  ]) {
    requireFragment(relativePath, text, fragment);
  }
}

const xiaomanProductionCompletionEvidenceTemplatePath =
  "docs/reports/templates/xiaoman-production-completion-evidence.json";
if (!exists(xiaomanProductionCompletionEvidenceTemplatePath)) {
  addError(
    `${xiaomanProductionCompletionEvidenceTemplatePath}: missing Xiaoman production completion evidence manifest template`
  );
} else {
  const template = readText(xiaomanProductionCompletionEvidenceTemplatePath);
  for (const fragment of [
    "xiaoman-production-completion-evidence-v1",
    "release_please_validation",
    "release_tag",
    "released_commit_sha",
    "manual_ci_workflow",
    "release_please_status",
    "qiwe_production_enablement",
    "included_in_release_sha",
    "listener_service_timer_reviewed",
    "observation_reviewed",
    "rollback_reviewed",
    "exact_allowlists_reviewed",
    "production_feature_boundary_reviewed",
    "huabaosi_production_activation",
    "image_generation_observation_passed",
    "feishu_mirror_activation_approved",
    "first_record_evidence_retained",
    "real_activity_confirmation",
    "qiwe_group_arrival_confirmed",
  ]) {
    requireFragment(
      xiaomanProductionCompletionEvidenceTemplatePath,
      template,
      fragment
    );
  }
  for (const fragment of [
    "QIWE_TOKEN",
    "QIWE_GUID",
    "postgres://",
    "postgresql://",
    "https://",
    "target_group_id",
    "artifact_uri",
  ]) {
    forbidFragment(xiaomanProductionCompletionEvidenceTemplatePath, template, fragment);
  }
}

if (errors.length > 0) {
  console.error("Deploy contract check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log("Deploy contract check passed.");
