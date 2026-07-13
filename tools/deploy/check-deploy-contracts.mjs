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
    "QINTOPIA_HUABAOSI_IMAGE_STAGING_DATABASE_URL_SHA256",
    "QINTOPIA_HUABAOSI_IMAGE_STAGING_WORK_ITEM_ID",
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

if (errors.length > 0) {
  console.error("Deploy contract check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log("Deploy contract check passed.");
