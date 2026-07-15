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
const workItemId = "7cd7d739-cd77-4b38-97f7-dcd57eb9475a";
const databaseUrl =
  "postgres://staging-user:private-password@127.0.0.1:5432/qintopia_staging";
const databaseHash = crypto.createHash("sha256").update(databaseUrl).digest("hex");
const callbackSecret = "callback-aes-secret-must-not-appear";

const writeExecutable = (filePath, content) => {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, content, "utf8");
  fs.chmodSync(filePath, 0o755);
};

try {
  const envFile = path.join(tmpRoot, "message-sidecar-staging.env");
  const sidecarLog = path.join(tmpRoot, "sidecar.log");
  const sidecar = path.join(tmpRoot, "bin", "qintopia-message-sidecar");
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
    sidecar,
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

  const baseEnv = {
    ...process.env,
    QINTOPIA_QIWE_IMAGE_STAGING_SMOKE_ENABLE: "1",
    QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL: "approved-staging-qiwe-image-send",
    QINTOPIA_QIWE_IMAGE_STAGING_ENV_FILE: envFile,
    QINTOPIA_QIWE_IMAGE_STAGING_DATABASE_URL_SHA256: databaseHash,
    QINTOPIA_QIWE_IMAGE_STAGING_WORK_ITEM_ID: workItemId,
    QINTOPIA_SIDECAR_BIN: sidecar,
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

  const upload = runSmoke("upload");
  if (upload.status !== 0 || !upload.stdout.includes("awaiting one bounded")) {
    throw new Error(
      `expected upload phase to pass\nstdout:\n${upload.stdout}\nstderr:\n${upload.stderr}`
    );
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
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("QiWe image-send staging smoke test passed.");
