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
    "QINTOPIA_XIAOMAN_ACTIVITY_SIGNAL_TIMER_OBSERVATION_ENABLE=1",
    "xiaoman-activity-signal-timer-observation-smoke.sh",
    "QINTOPIA_XIAOMAN_ACTIVITY_PROMOTION_STARTER_TIMER_OBSERVATION_ENABLE=1",
    "xiaoman-activity-promotion-starter-timer-observation-smoke.sh",
    "QINTOPIA_OPERATIONS_DOWNSTREAM_TIMERS_OBSERVATION_ENABLE=1",
    "operations-downstream-timers-observation-smoke.sh",
    "QINTOPIA_XIAOMAN_ACTIVITY_DOWNSTREAM_OBSERVATION_ENABLE=1",
    "xiaoman-activity-downstream-observation-smoke.sh",
    "QINTOPIA_XIAOMAN_ACTIVITY_IMAGE_GENERATION_STARTER_OBSERVATION_ENABLE=1",
    "xiaoman-activity-image-generation-starter-observation-smoke.sh",
    "QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_OBSERVATION_ENABLE=1",
    "huabaosi-image-generation-production-observation-smoke.sh",
    "QINTOPIA_XIAOMAN_ACTIVITY_SEND_REQUEST_STARTER_OBSERVATION_ENABLE=1",
    "xiaoman-activity-send-request-starter-observation-smoke.sh",
  ]) {
    requireFragment(xiaomanPreflightPath, preflight, fragment);
  }

  for (const fragment of [
    "QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE=1",
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
    "Huabaosi provider disabled state",
    "run-huabaosi-image-generation-worker --once --dry-run",
    "qintopia-agentos-xiaoman-activity-send-request-starter-worker.timer",
    "run-xiaoman-activity-send-request-starter-worker --once --apply",
    "Secret and external-send scan",
    "send_executed=true",
    "Production boundary",
    "Eligible Xiaoman `event_signals` preview count",
    "Eligible image-generation request preview count",
    "Eligible awaiting publish group message request count",
    "Pass: production observation can continue without enabling external adapters",
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
if (!exists(aliangStagingSmokePath)) {
  addError(`${aliangStagingSmokePath}: missing Huabaosi staging smoke`);
} else {
  const smoke = readText(aliangStagingSmokePath);
  for (const fragment of [
    "QINTOPIA_HUABAOSI_IMAGE_STAGING_SMOKE_ENABLE",
    "QINTOPIA_HUABAOSI_IMAGE_STAGING_APPROVAL",
    "approved-staging-image-generation",
    "QINTOPIA_HUABAOSI_IMAGE_STAGING_ENV_FILE",
    "QINTOPIA_HUABAOSI_IMAGE_STAGING_WORK_ITEM_ID",
    "--features huabaosi-staging-adapter",
    'payload["adapter_compiled"] is True',
    "huabaosi-image-generation-preflight",
    "run-huabaosi-image-generation-worker",
    "generated_image_created",
    "pending",
    "artifact_uri",
    "QINTOPIA_HUABAOSI_MEDIA_PUBLIC_BASE_URL",
    "urlparse(sys.stdin.read())",
  ]) {
    requireFragment(aliangStagingSmokePath, smoke, fragment);
  }

  for (const fragment of [
    "systemctl",
    'python3 - "$QINTOPIA_SIDECAR_DATABASE_URL"',
    "run-group-message-send-worker",
    "--use-feishu-base",
    "send-ready",
    "operations-group-message-confirm",
  ]) {
    forbidFragment(aliangStagingSmokePath, smoke, fragment);
  }
}

const qiweImageStagingSmokePath =
  "deploy/sidecar/scripts/qiwe-image-send-staging-smoke.sh";
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
    "QINTOPIA_QIWE_IMAGE_STAGING_WORK_ITEM_ID",
    "QINTOPIA_QIWE_IMAGE_STAGING_PHASE",
    "--features qiwe-staging-adapter",
    "qiwe-image-send-staging-preflight",
    "run-qiwe-image-send-worker",
    "process-qiwe-image-send-callback",
    "image_upload_accepted",
    "image_send_completed",
    'payload["external_send_executed"] is True',
    "callback_credential_schema",
    "contains forbidden sensitive output",
  ]) {
    requireFragment(qiweImageStagingSmokePath, smoke, fragment);
  }
  for (const fragment of [
    "systemctl",
    "callback.json",
    "run-group-message-send-worker",
    "operations-group-message-confirm",
    "--use-feishu-base",
  ]) {
    forbidFragment(qiweImageStagingSmokePath, smoke, fragment);
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
