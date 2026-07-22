#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "qintopia-xiaoman-observation-"));
const aggregatePreflightPath = path.join(
  repoRoot,
  "deploy/sidecar/scripts/xiaoman-activity-production-preflight-smoke.sh"
);

const writeExecutable = (filePath, content) => {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, content, "utf8");
  fs.chmodSync(filePath, 0o755);
};

const assertPassed = (label, result) => {
  if (result.status !== 0) {
    throw new Error(
      `${label} failed\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`
    );
  }
};

const aggregatePreflight = fs.readFileSync(aggregatePreflightPath, "utf8");
for (const fragment of [
  'CHILD_PATH="/usr/bin:/bin:/usr/sbin:/sbin"',
  'env -i "${child_env[@]}" "$script_path"',
  '"PATH=${CHILD_PATH}"',
  "QINTOPIA_OPERATIONS_GROUP_SEND_READY_TIMER_OBSERVATION_ENABLE",
  "operations-group-send-ready-timer-observation-smoke.sh",
]) {
  if (!aggregatePreflight.includes(fragment)) {
    throw new Error(`aggregate production preflight must include ${fragment}`);
  }
}
for (const fragment of [
  "env QINTOPIA_",
  "QINTOPIA_SIDECAR_ENV_FILE=",
  "SYSTEMCTL=",
  "JOURNALCTL=",
  "_OBSERVATION_TEST_MODE",
]) {
  if (aggregatePreflight.includes(fragment)) {
    throw new Error(`aggregate production preflight must not include ${fragment}`);
  }
}

try {
  const binDir = path.join(tmpRoot, "bin");
  const sidecarLog = path.join(tmpRoot, "sidecar.log");
  const sidecar = path.join(binDir, "qintopia-message-sidecar");
  const systemctl = path.join(binDir, "systemctl");
  const journalctl = path.join(binDir, "journalctl");

  writeExecutable(
    sidecar,
    `#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"${sidecarLog}"
case "$1" in
  run-evidence-worker)
    [[ "$2" == "--once" && "$3" == "--dry-run" ]]
    if [[ "\${FAKE_QUEUE_STATE:-empty}" == "preview" ]]; then
      marker="external adapters are disabled"
      [[ "\${FAKE_MISSING_EXTERNAL_MARKER:-0}" == "1" ]] && marker="internal only"
      printf '{"success":true,"worker":"evidence-worker","dry_run":true,"apply_requested":false,"fixture_mode":false,"action_status":"dry_run_ok","artifact_ids":[],"artifact_previews":[{}],"limitations":["%s"],"guardrails":[],"safe_for_chat":false}\n' "$marker"
    else
      printf '%s\n' '{"success":true,"worker":"evidence-worker","dry_run":true,"apply_requested":false,"fixture_mode":false,"action_status":"no_claimable_evidence_request","artifact_ids":[],"artifact_previews":[],"limitations":["no claimable evidence work item was found"],"guardrails":["internal evidence only"],"safe_for_chat":false}'
    fi
    ;;
  run-collaboration-worker)
    [[ "$2" == "--work-item-type" && "$3" == "visual_asset_request" && "$4" == "--once" && "$5" == "--dry-run" ]]
    if [[ "\${FAKE_QUEUE_STATE:-empty}" == "preview" ]]; then
      marker="external publishing is disabled"
      [[ "\${FAKE_MISSING_EXTERNAL_MARKER:-0}" == "1" ]] && marker="internal only"
      printf '{"success":true,"worker":"collaboration-worker","dry_run":true,"apply_requested":false,"fixture_mode":false,"action_status":"dry_run_ok","artifact_ids":[],"artifact_previews":[{}],"limitations":["%s"],"guardrails":[],"safe_for_chat":false}\n' "$marker"
    else
      printf '%s\n' '{"success":true,"worker":"collaboration-worker","dry_run":true,"apply_requested":false,"fixture_mode":false,"action_status":"no_claimable_work_item","artifact_ids":[],"artifact_previews":[],"limitations":["no claimable work item was found"],"guardrails":["internal visual brief only"],"safe_for_chat":false}'
    fi
    ;;
  run-xiaoman-activity-send-request-starter-worker)
    [[ "$2" == "--check-only" ]]
    printf '{"success":true,"worker":"xiaoman-activity-send-request-starter-worker","source":"agentos_work_items","dry_run":true,"check_only":true,"safe_for_chat":false,"action_status":"%s","scanned_count":0,"created_count":0,"existing_count":0,"missing_child_count":0,"work_items":[]}\n' "\${FAKE_SEND_STATUS:-no_eligible_approved_generated_images}"
    ;;
  *)
    exit 64
    ;;
esac
`
  );

  writeExecutable(
    systemctl,
    `#!/usr/bin/env bash
set -euo pipefail
case "$1" in
  is-active)
    printf '%s\n' active
    ;;
  is-enabled)
    printf '%s\n' enabled
    ;;
  cat)
    if [[ "$2" == *.service ]]; then
      printf '%s\n' '[Service]' 'ExecStart=/fake/qintopia-message-sidecar run-xiaoman-activity-send-request-starter-worker --once --apply'
    else
      printf '%s\n' '[Timer]' 'OnBootSec=10min' 'OnUnitActiveSec=2min' 'Unit=qintopia-agentos-xiaoman-activity-send-request-starter-worker.service'
    fi
    ;;
  list-timers)
    printf '%s\n' 'qintopia-agentos-xiaoman-activity-send-request-starter-worker.timer'
    ;;
  *)
    exit 64
    ;;
esac
`
  );

  writeExecutable(journalctl, "#!/usr/bin/env bash\nexit 0\n");

  const commonEnv = {
    ...process.env,
    QINTOPIA_SIDECAR_BIN: sidecar,
    QINTOPIA_SIDECAR_ENV_FILE: path.join(tmpRoot, "missing.env"),
  };
  const downstreamScript = path.join(
    repoRoot,
    "deploy/sidecar/scripts/xiaoman-activity-downstream-observation-smoke.sh"
  );
  const runDownstream = (extraEnv = {}) =>
    spawnSync("bash", [downstreamScript], {
      cwd: repoRoot,
      env: {
        ...commonEnv,
        QINTOPIA_XIAOMAN_ACTIVITY_DOWNSTREAM_OBSERVATION_ENABLE: "1",
        ...extraEnv,
      },
      encoding: "utf8",
    });

  assertPassed("empty downstream observation", runDownstream());
  assertPassed(
    "preview downstream observation",
    runDownstream({ FAKE_QUEUE_STATE: "preview" })
  );
  const missingBoundary = runDownstream({
    FAKE_QUEUE_STATE: "preview",
    FAKE_MISSING_EXTERNAL_MARKER: "1",
  });
  if (missingBoundary.status === 0) {
    throw new Error("preview observation must require an external boundary marker");
  }

  const sendScript = path.join(
    repoRoot,
    "deploy/sidecar/scripts/xiaoman-activity-send-request-starter-observation-smoke.sh"
  );
  const runSend = (actionStatus) =>
    spawnSync("bash", [sendScript], {
      cwd: repoRoot,
      env: {
        ...commonEnv,
        QINTOPIA_XIAOMAN_ACTIVITY_SEND_REQUEST_STARTER_OBSERVATION_ENABLE: "1",
        FAKE_SEND_STATUS: actionStatus,
        SYSTEMCTL: systemctl,
        JOURNALCTL: journalctl,
      },
      encoding: "utf8",
    });

  for (const actionStatus of [
    "no_eligible_approved_generated_images",
    "group_message_requests_preview",
  ]) {
    assertPassed(`send observation ${actionStatus}`, runSend(actionStatus));
  }

  const sidecarCommands = fs.readFileSync(sidecarLog, "utf8");
  if (sidecarCommands.includes("--apply")) {
    throw new Error("production observations must not invoke a sidecar apply command");
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Xiaoman production observation contract test passed.");
