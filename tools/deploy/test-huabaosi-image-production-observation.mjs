#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "qintopia-image-observation-"));
const script = path.join(
  repoRoot,
  "deploy/sidecar/scripts/huabaosi-image-generation-production-observation-smoke.sh"
);

const writeExecutable = (filePath, content) => {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, content, "utf8");
  fs.chmodSync(filePath, 0o755);
};

try {
  const systemctlLog = path.join(tmpRoot, "systemctl.log");
  const sidecarLog = path.join(tmpRoot, "sidecar.log");
  const systemctl = path.join(tmpRoot, "bin", "systemctl");
  const sidecar = path.join(tmpRoot, "bin", "qintopia-message-sidecar");

  writeExecutable(
    systemctl,
    `#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"${systemctlLog}"
if [[ "\${FAKE_PROVIDER_UNIT_PRESENT:-0}" == "1" && "$1" == "cat" ]]; then
  exit 0
fi
exit 1
`
  );

  writeExecutable(
    sidecar,
    `#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"${sidecarLog}"
case "$1" in
  huabaosi-image-generation-preflight)
    if [[ "\${FAKE_CONFIG_VALID:-0}" == "1" ]]; then
      printf '%s\n' '{"success":true,"worker":"huabaosi-image-generation-worker","action_status":"adapter_config_ready","generation_enabled":false,"adapter_compiled":false,"config_valid":true,"media_allowed_host_count":1,"missing_configuration":[],"safe_for_chat":false}'
      exit 0
    fi
    printf '%s\n' '{"success":false,"worker":"huabaosi-image-generation-worker","action_status":"adapter_not_configured","generation_enabled":false,"adapter_compiled":false,"config_valid":false,"media_allowed_host_count":0,"missing_configuration":["QINTOPIA_HUABAOSI_IMAGE_API_KEY"],"safe_for_chat":false}'
    printf '%s\n' 'image adapter preflight configuration is invalid' >&2
    exit 1
    ;;
  run-huabaosi-image-generation-worker)
    [[ "$2" == "--once" && "$3" == "--dry-run" ]]
    if [[ -n "\${FAKE_STDERR_LEAK_VALUE:-}" ]]; then
      printf '%s\n' "\${FAKE_STDERR_LEAK_VALUE}" >&2
      exit 1
    fi
    printf '{"success":true,"dry_run":true,"apply_requested":false,"fixture_mode":false,"worker":"huabaosi-image-generation-worker","action_status":"no_claimable_image_request","work_item_id":null,"artifact_ids":[],"artifact_preview":null,"safe_for_chat":false,"unexpected":"%s"}\n' "\${FAKE_LEAK_VALUE:-}"
    ;;
  *)
    exit 64
    ;;
esac
`
  );

  const runObservation = (extraEnv = {}) =>
    spawnSync("bash", [script], {
      cwd: repoRoot,
      env: {
        ...process.env,
        QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_OBSERVATION_ENABLE: "1",
        QINTOPIA_SIDECAR_BIN: sidecar,
        QINTOPIA_SIDECAR_ENV_FILE: path.join(tmpRoot, "missing.env"),
        SYSTEMCTL: systemctl,
        ...extraEnv,
      },
      encoding: "utf8",
    });

  for (const configValid of ["0", "1"]) {
    fs.writeFileSync(sidecarLog, "", "utf8");
    const result = runObservation({ FAKE_CONFIG_VALID: configValid });
    if (result.status !== 0) {
      throw new Error(
        `expected disabled observation to pass for config_valid=${configValid}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`
      );
    }
    const log = fs.readFileSync(sidecarLog, "utf8");
    for (const command of [
      "huabaosi-image-generation-preflight",
      "run-huabaosi-image-generation-worker --once --dry-run",
    ]) {
      if (!log.includes(command)) {
        throw new Error(`sidecar log is missing ${command}`);
      }
    }
    if (log.includes("--apply")) {
      throw new Error(
        "production observation must not run image generation with --apply"
      );
    }
  }

  for (const enabledValue of ["1", " 1 "]) {
    const enabled = runObservation({
      QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED: enabledValue,
      QINTOPIA_HUABAOSI_IMAGE_API_KEY: "observation-secret-must-not-appear",
    });
    if (enabled.status === 0) {
      throw new Error(
        `expected generation flag ${JSON.stringify(enabledValue)} to fail production observation`
      );
    }
    if (
      `${enabled.stdout}\n${enabled.stderr}`.includes(
        "observation-secret-must-not-appear"
      )
    ) {
      throw new Error("production observation failure exposed a configured secret");
    }
  }

  const installed = runObservation({ FAKE_PROVIDER_UNIT_PRESENT: "1" });
  if (installed.status === 0) {
    throw new Error("expected installed provider unit to fail production observation");
  }

  const leakedValue = "configured-secret-must-be-redacted";
  const leaked = runObservation({
    QINTOPIA_HUABAOSI_IMAGE_API_KEY: leakedValue,
    FAKE_LEAK_VALUE: leakedValue,
  });
  if (leaked.status === 0) {
    throw new Error("expected configured secret in worker output to fail observation");
  }
  if (`${leaked.stdout}\n${leaked.stderr}`.includes(leakedValue)) {
    throw new Error("observation failure repeated the configured secret");
  }

  const stderrLeakedValue = "stderr-secret-must-be-redacted";
  const stderrLeaked = runObservation({
    QINTOPIA_HUABAOSI_IMAGE_API_KEY: stderrLeakedValue,
    FAKE_STDERR_LEAK_VALUE: stderrLeakedValue,
  });
  if (stderrLeaked.status === 0) {
    throw new Error("expected configured secret in worker stderr to fail observation");
  }
  if (`${stderrLeaked.stdout}\n${stderrLeaked.stderr}`.includes(stderrLeakedValue)) {
    throw new Error("observation failure repeated the configured stderr secret");
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Huabaosi image production observation test passed.");
