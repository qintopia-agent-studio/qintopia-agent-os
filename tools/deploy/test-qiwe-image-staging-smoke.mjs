#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "qintopia-qiwe-staging-"));
const script = path.join(
  repoRoot,
  "deploy/sidecar/scripts/qiwe-image-send-staging-smoke.sh"
);
const evidenceChecker = path.join(
  repoRoot,
  "tools/deploy/check-qiwe-image-staging-evidence.mjs"
);
const workItemId = "7cd7d739-cd77-4b38-97f7-dcd57eb9475a";
const databaseUrl =
  "postgres://staging-user:private-password@127.0.0.1:5432/qintopia_staging";
const databaseHash = crypto.createHash("sha256").update(databaseUrl).digest("hex");
const callbackSecret = "callback-aes-secret-must-not-appear";
const packagedSidecarDir = path.join(repoRoot, "sidecar");
const packagedSidecar = path.join(packagedSidecarDir, "qintopia-message-sidecar");
const createdPackagedSidecarDir = !fs.existsSync(packagedSidecarDir);
let installedPackagedSidecar = false;

const writeExecutable = (filePath, content) => {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, content, "utf8");
  fs.chmodSync(filePath, 0o755);
};

try {
  if (!createdPackagedSidecarDir || fs.existsSync(packagedSidecar)) {
    throw new Error("unexpected packaged sidecar path exists in source checkout");
  }
  const envFile = path.join(tmpRoot, "message-sidecar-staging.env");
  const sidecarLog = path.join(tmpRoot, "sidecar.log");
  fs.writeFileSync(
    envFile,
    [
      "QINTOPIA_QIWE_IMAGE_SEND_ENABLED=1",
      "QINTOPIA_QIWE_IMAGE_SEND_WEBHOOK_READY=1",
      `QINTOPIA_SIDECAR_DATABASE_URL=${databaseUrl}`,
      "QIWE_API_URL=https://manager.qiweapi.com/qiwe/api/qw/doApi",
      "QIWE_TOKEN=fake-qiwe-token-must-not-appear",
      "QIWE_GUID=fake-device-guid-must-not-appear",
      "QINTOPIA_QIWE_IMAGE_SEND_ALLOWED_HOSTS=manager.qiweapi.com",
      "QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS=media.example.test",
      "QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS=isolated-group-must-not-appear",
      "",
    ].join("\n"),
    "utf8"
  );

  writeExecutable(
    packagedSidecar,
    `#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"${sidecarLog}"
case "$1" in
  qiwe-image-send-staging-preflight)
    if IFS= read -r -t 0.1 unexpected_input; then
      exit 65
    fi
    printf '%s\n' '{"success":true,"worker":"qiwe-image-send-adapter","action_status":"staging_adapter_ready","adapter_compiled":true,"send_enabled":true,"owner_approval_valid":true,"config_valid":true,"database_boundary_valid":true,"webhook_ready":true,"allowed_host_count":1,"media_allowed_host_count":1,"allowed_group_count":1,"missing_configuration":[],"protocol":"qiwe_async_url_upload_then_send_image_staging_v1","safe_for_chat":false,"limitations":[],"guardrails":[]}'
    ;;
  run-qiwe-image-send-worker)
    if IFS= read -r -t 0.1 unexpected_input; then
      exit 65
    fi
    [[ "$2" == "--once" && "$3" == "--work-item-id" && "$4" == "${workItemId}" && "$5" == "--apply" ]]
    printf '%s\n' '{"success":true,"dry_run":false,"apply_requested":true,"worker":"qiwe-image-send-adapter","phase":"upload","action_status":"image_upload_accepted","work_item_id":"${workItemId}","external_upload_requested":true,"callback_received":false,"external_send_executed":false,"safe_for_chat":false,"limitations":[],"guardrails":[]}'
    ;;
  process-qiwe-image-send-callback)
    [[ "$2" == "--apply" ]]
    callback="$(cat)"
    [[ "$callback" == *"${callbackSecret}"* ]]
    if [[ -n "\${FAKE_CALLBACK_ECHO_RAW:-}" ]]; then
      printf '%s\n' "$callback" >&2
      exit 70
    fi
    action_status="image_send_completed"
    if [[ -n "\${FAKE_CALLBACK_LEAK_VALUE:-}" ]]; then
      action_status="\${FAKE_CALLBACK_LEAK_VALUE}"
    fi
    printf '{"success":true,"dry_run":false,"apply_requested":true,"worker":"qiwe-image-send-adapter","phase":"callback","action_status":"%s","work_item_id":"${workItemId}","external_upload_requested":false,"callback_received":true,"callback_credential_schema":"fileAesKey+fileId+fileMd5+fileSize+filename","callback_additional_field_count":0,"external_send_executed":true,"safe_for_chat":false,"limitations":[],"guardrails":[]}\n' "$action_status"
    ;;
  *)
    exit 64
    ;;
esac
`
  );
  installedPackagedSidecar = true;
  const sidecarHash = crypto
    .createHash("sha256")
    .update(fs.readFileSync(packagedSidecar))
    .digest("hex");

  const baseEnv = {
    ...process.env,
    QINTOPIA_QIWE_IMAGE_STAGING_SMOKE_ENABLE: "1",
    QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL: "approved-staging-qiwe-image-send",
    QINTOPIA_QIWE_IMAGE_STAGING_ENV_FILE: envFile,
    QINTOPIA_QIWE_IMAGE_STAGING_DATABASE_URL_SHA256: databaseHash,
    QINTOPIA_QIWE_IMAGE_STAGING_SIDECAR_SHA256: sidecarHash,
    QINTOPIA_QIWE_IMAGE_STAGING_WORK_ITEM_ID: workItemId,
  };

  const runSmoke = (phase, extraEnv = {}, input) =>
    spawnSync("bash", [script], {
      cwd: repoRoot,
      env: {
        ...baseEnv,
        QINTOPIA_QIWE_IMAGE_STAGING_PHASE: phase,
        ...extraEnv,
      },
      input,
      encoding: "utf8",
    });

  const parseEvidence = (result) =>
    result.stdout
      .split(/\r?\n/)
      .filter((line) => line.startsWith("qiwe_image_send_staging_evidence="))
      .map((line) =>
        JSON.parse(line.slice("qiwe_image_send_staging_evidence=".length))
      );

  const preflight = runSmoke("preflight", {
    QINTOPIA_QIWE_IMAGE_STAGING_WORK_ITEM_ID: "",
  });
  if (preflight.status !== 0 || !preflight.stdout.includes("preflight passed")) {
    throw new Error(
      `expected preflight phase to pass\nstdout:\n${preflight.stdout}\nstderr:\n${preflight.stderr}`
    );
  }
  const preflightEvidence = parseEvidence(preflight);
  if (
    preflightEvidence.length !== 1 ||
    preflightEvidence[0].action_status !== "staging_adapter_ready" ||
    preflightEvidence[0].allowed_group_count !== 1 ||
    preflightEvidence[0].send_enabled !== true ||
    preflightEvidence[0].webhook_ready !== true ||
    preflightEvidence[0].sidecar_binary_sha256 !== sidecarHash
  ) {
    throw new Error(`preflight evidence is invalid\nstdout:\n${preflight.stdout}`);
  }
  const preflightLog = fs.readFileSync(sidecarLog, "utf8").trim().split(/\r?\n/);
  if (
    preflightLog.length !== 1 ||
    preflightLog[0] !== "qiwe-image-send-staging-preflight"
  ) {
    throw new Error("preflight phase invoked a command beyond staging preflight");
  }
  const preflightEvidenceFile = path.join(tmpRoot, "preflight-evidence.txt");
  fs.writeFileSync(preflightEvidenceFile, preflight.stdout, "utf8");
  const preflightEvidenceCheck = spawnSync(
    "node",
    [evidenceChecker, "--preflight-only", preflightEvidenceFile],
    { cwd: repoRoot, encoding: "utf8" }
  );
  if (preflightEvidenceCheck.status !== 0) {
    throw new Error(
      `expected preflight evidence check to pass\nstdout:\n${preflightEvidenceCheck.stdout}\nstderr:\n${preflightEvidenceCheck.stderr}`
    );
  }
  const unavailableSidecar = path.join(tmpRoot, "qintopia-message-sidecar.hidden");
  fs.renameSync(packagedSidecar, unavailableSidecar);
  try {
    const missingPreflightBinary = runSmoke(
      "preflight",
      {
        QINTOPIA_QIWE_IMAGE_STAGING_WORK_ITEM_ID: "",
      },
      "callback-stream-must-not-be-read"
    );
    if (missingPreflightBinary.status === 0) {
      throw new Error("expected preflight without packaged sidecar to fail");
    }
    if (
      `${missingPreflightBinary.stdout}\n${missingPreflightBinary.stderr}`.includes(
        "callback-stream-must-not-be-read"
      )
    ) {
      throw new Error("missing preflight binary consumed stdin");
    }
  } finally {
    fs.renameSync(unavailableSidecar, packagedSidecar);
  }

  const upload = runSmoke("upload");
  if (upload.status !== 0 || !upload.stdout.includes("awaiting one bounded")) {
    throw new Error(
      `expected upload phase to pass\nstdout:\n${upload.stdout}\nstderr:\n${upload.stderr}`
    );
  }
  const uploadEvidence = parseEvidence(upload);
  if (
    uploadEvidence.length !== 2 ||
    uploadEvidence[0].action_status !== "staging_adapter_ready" ||
    uploadEvidence[0].allowed_group_count !== 1 ||
    uploadEvidence[1].phase !== "upload" ||
    uploadEvidence[1].action_status !== "image_upload_accepted" ||
    uploadEvidence[1].work_item_id !== workItemId ||
    uploadEvidence[1].external_upload_requested !== true ||
    uploadEvidence[1].external_send_executed !== false ||
    uploadEvidence[1].sidecar_binary_sha256 !== sidecarHash
  ) {
    throw new Error(`upload evidence is invalid\nstdout:\n${upload.stdout}`);
  }

  const callbackPayload = JSON.stringify({
    code: 0,
    data: [
      {
        requestId: "private-request-id-must-not-appear",
        cmd: 20000,
        msgData: {
          fileAesKey: callbackSecret,
          fileId: "private-file-id-must-not-appear",
          fileMd5: "98e7c2acf4391f8b4a2bbd39e364c5e3",
          fileSize: 48300,
          filename: "private-poster-name-must-not-appear.jpg",
        },
      },
    ],
  });
  const callback = runSmoke("callback", {}, callbackPayload);
  if (callback.status !== 0 || !callback.stdout.includes("one reviewed image send")) {
    throw new Error(
      `expected callback phase to pass\nstdout:\n${callback.stdout}\nstderr:\n${callback.stderr}`
    );
  }
  const callbackEvidence = parseEvidence(callback);
  if (
    callbackEvidence.length !== 2 ||
    callbackEvidence[0].action_status !== "staging_adapter_ready" ||
    callbackEvidence[1].phase !== "callback" ||
    callbackEvidence[1].action_status !== "image_send_completed" ||
    callbackEvidence[1].work_item_id !== workItemId ||
    callbackEvidence[1].callback_credential_schema !==
      "fileAesKey+fileId+fileMd5+fileSize+filename" ||
    callbackEvidence[1].callback_additional_field_count !== 0 ||
    callbackEvidence[1].external_send_executed !== true ||
    callbackEvidence[1].sidecar_binary_sha256 !== sidecarHash
  ) {
    throw new Error(`callback evidence is invalid\nstdout:\n${callback.stdout}`);
  }
  for (const sensitive of [
    callbackSecret,
    "private-request-id-must-not-appear",
    "private-file-id-must-not-appear",
    "private-poster-name-must-not-appear.jpg",
    databaseUrl,
    "fake-qiwe-token-must-not-appear",
    "isolated-group-must-not-appear",
  ]) {
    if (`${callback.stdout}\n${callback.stderr}`.includes(sensitive)) {
      throw new Error("callback smoke output exposed a sensitive value");
    }
  }
  const completeEvidenceFile = path.join(tmpRoot, "complete-evidence.txt");
  fs.writeFileSync(
    completeEvidenceFile,
    `${upload.stdout}\n${callback.stdout}`,
    "utf8"
  );
  const completeEvidenceCheck = spawnSync(
    "node",
    [evidenceChecker, completeEvidenceFile],
    { cwd: repoRoot, encoding: "utf8" }
  );
  if (completeEvidenceCheck.status !== 0) {
    throw new Error(
      `expected complete evidence check to pass\nstdout:\n${completeEvidenceCheck.stdout}\nstderr:\n${completeEvidenceCheck.stderr}`
    );
  }
  const missingPreflightEvidenceFile = path.join(
    tmpRoot,
    "missing-preflight-evidence.txt"
  );
  fs.writeFileSync(
    missingPreflightEvidenceFile,
    [
      upload.stdout
        .split(/\r?\n/)
        .filter((line) => line.includes('"phase":"upload"'))
        .join("\n"),
      callback.stdout
        .split(/\r?\n/)
        .filter((line) => line.includes('"phase":"callback"'))
        .join("\n"),
      "",
    ].join("\n"),
    "utf8"
  );
  const missingPreflightEvidenceCheck = spawnSync(
    "node",
    [evidenceChecker, missingPreflightEvidenceFile],
    { cwd: repoRoot, encoding: "utf8" }
  );
  if (missingPreflightEvidenceCheck.status === 0) {
    throw new Error("expected complete evidence check to reject missing preflight");
  }
  const rawEvidenceFile = path.join(tmpRoot, "raw-evidence.txt");
  fs.writeFileSync(
    rawEvidenceFile,
    `${completeEvidenceCheck.stdout}\n{"requestId":"private-request-id-must-not-appear"}\n`,
    "utf8"
  );
  const rawEvidenceCheck = spawnSync("node", [evidenceChecker, rawEvidenceFile], {
    cwd: repoRoot,
    encoding: "utf8",
  });
  if (rawEvidenceCheck.status === 0) {
    throw new Error("expected evidence check to reject raw callback fields");
  }

  const invalidPhase = runSmoke("send");
  if (invalidPhase.status === 0) {
    throw new Error("expected an unreviewed staging phase to fail");
  }

  const wrongHash = runSmoke("upload", {
    QINTOPIA_QIWE_IMAGE_STAGING_DATABASE_URL_SHA256: "0".repeat(64),
  });
  if (wrongHash.status === 0) {
    throw new Error("expected a mismatched staging database hash to fail");
  }
  if (`${wrongHash.stdout}\n${wrongHash.stderr}`.includes(databaseUrl)) {
    throw new Error("database hash failure exposed the database URL");
  }

  const wrongSidecarHash = runSmoke("preflight", {
    QINTOPIA_QIWE_IMAGE_STAGING_WORK_ITEM_ID: "",
    QINTOPIA_QIWE_IMAGE_STAGING_SIDECAR_SHA256: "0".repeat(64),
  });
  if (wrongSidecarHash.status === 0) {
    throw new Error("expected a mismatched sidecar binary hash to fail");
  }
  if (`${wrongSidecarHash.stdout}\n${wrongSidecarHash.stderr}`.includes(sidecarHash)) {
    throw new Error("sidecar hash failure exposed the computed hash");
  }

  const leaked = runSmoke(
    "callback",
    { FAKE_CALLBACK_LEAK_VALUE: callbackSecret },
    callbackPayload
  );
  if (leaked.status === 0) {
    throw new Error("expected a callback report with leaked state to fail validation");
  }
  if (`${leaked.stdout}\n${leaked.stderr}`.includes(callbackSecret)) {
    throw new Error("callback validation failure repeated the leaked value");
  }

  const reportDir = path.join(tmpRoot, "callback-reports");
  fs.mkdirSync(reportDir);
  const rawCallbackLeak = runSmoke(
    "callback",
    { FAKE_CALLBACK_ECHO_RAW: "1", TMPDIR: reportDir },
    callbackPayload
  );
  if (rawCallbackLeak.status === 0) {
    throw new Error("expected raw callback stderr to fail the smoke");
  }
  for (const sensitive of [
    callbackSecret,
    "private-request-id-must-not-appear",
    "private-file-id-must-not-appear",
    "private-poster-name-must-not-appear.jpg",
  ]) {
    if (`${rawCallbackLeak.stdout}\n${rawCallbackLeak.stderr}`.includes(sensitive)) {
      throw new Error("raw callback failure escaped protected smoke output");
    }
  }
  if (fs.readdirSync(reportDir).length !== 0) {
    throw new Error("raw callback failure wrote subprocess output to disk");
  }

  const commandMarker = path.join(tmpRoot, "env-command-executed");
  const executableEnvFile = path.join(tmpRoot, "executable-staging.env");
  fs.writeFileSync(
    executableEnvFile,
    `${fs.readFileSync(envFile, "utf8")}touch ${commandMarker}\n`,
    "utf8"
  );
  const executableEnv = runSmoke("upload", {
    QINTOPIA_QIWE_IMAGE_STAGING_ENV_FILE: executableEnvFile,
  });
  if (executableEnv.status === 0 || fs.existsSync(commandMarker)) {
    throw new Error("staging env parser executed shell syntax");
  }
  if (
    `${executableEnv.stdout}\n${executableEnv.stderr}`.includes(
      "fake-qiwe-token-must-not-appear"
    )
  ) {
    throw new Error("staging env parser failure exposed a secret");
  }

  const log = fs.readFileSync(sidecarLog, "utf8");
  for (const command of [
    "qiwe-image-send-staging-preflight",
    `run-qiwe-image-send-worker --once --work-item-id ${workItemId} --apply`,
    "process-qiwe-image-send-callback --apply",
  ]) {
    if (!log.includes(command)) {
      throw new Error(`sidecar command log is missing ${command}`);
    }
  }
  if (log.includes("systemctl") || log.includes("--dry-run")) {
    throw new Error("staging smoke invoked an unexpected command");
  }
} finally {
  if (installedPackagedSidecar) {
    fs.rmSync(packagedSidecar, { force: true });
  }
  if (createdPackagedSidecarDir) {
    fs.rmSync(packagedSidecarDir, { recursive: true, force: true });
  }
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("QiWe image-send staging smoke test passed.");
