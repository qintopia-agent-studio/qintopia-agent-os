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

const aliangProductionActivationPath =
  "deploy/sidecar/scripts/activate-huabaosi-image-generation-production.sh";
if (!exists(aliangProductionActivationPath)) {
  addError(`${aliangProductionActivationPath}: missing production activation command`);
} else {
  const activation = readText(aliangProductionActivationPath);
  for (const fragment of [
    "QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_ACTIVATION",
    "approved-production-image-generation",
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
    "qintopia-agentos-huabaosi-image-generation-worker.service",
    "qintopia-agentos-huabaosi-image-generation-worker.timer",
    '"$SYSTEMCTL" disable --now "$WORKER_TIMER"',
  ]) {
    requireFragment(aliangProductionRollbackPath, rollback, fragment);
  }
  for (const fragment of ["rm -", "source ", "QIWE_", "FEISHU_"]) {
    forbidFragment(aliangProductionRollbackPath, rollback, fragment);
  }
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
    "reject_owner_writable",
    "path_not_executable",
    "path_owner_group_or_world_writable",
    "path_group_or_world_writable",
    "path_is_symlink",
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
    "ready_for_staging_preflight",
    "readiness smoke exposed staging env contents",
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
    "expected exactly one QiWe preflight evidence record",
    "forbidden sensitive fragment",
    "Xiaoman image-send staging evidence test passed.",
  ]) {
    requireFragment(xiaomanImageSendStagingEvidenceTestPath, test, fragment);
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

if (errors.length > 0) {
  console.error("Deploy contract check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log("Deploy contract check passed.");
