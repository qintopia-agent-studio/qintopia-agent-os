#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "qintopia-qiwe-activation-"));
const activationScript = path.join(
  repoRoot,
  "deploy/sidecar/scripts/activate-qiwe-image-send-production.sh"
);
const rollbackScript = path.join(
  repoRoot,
  "deploy/sidecar/scripts/rollback-qiwe-image-send-production.sh"
);

try {
  const logPath = path.join(tmpRoot, "systemctl.log");
  const envFile = path.join(tmpRoot, "message-sidecar.env");
  const systemctl = path.join(tmpRoot, "systemctl");
  fs.writeFileSync(
    systemctl,
    `#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"${logPath}"
if [[ "\${SYSTEMCTL_FAIL_PREFLIGHT:-0}" == "1" && "$*" == "start qintopia-agentos-qiwe-image-send-preflight.service" ]]; then
  exit 23
fi
`,
    "utf8"
  );
  fs.chmodSync(systemctl, 0o755);

  const writeEnv = (flag) =>
    fs.writeFileSync(
      envFile,
      [
        `QINTOPIA_QIWE_IMAGE_SEND_ENABLED=${flag}`,
        "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_APPROVAL=approved-production-qiwe-image-send",
        `QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256=${"a".repeat(64)}`,
        "",
      ].join("\n"),
      "utf8"
    );
  const run = (script, extraEnv = {}) =>
    spawnSync("bash", [script], {
      cwd: repoRoot,
      env: {
        ...process.env,
        SYSTEMCTL: systemctl,
        QINTOPIA_SIDECAR_ENV_FILE: envFile,
        ...extraEnv,
      },
      encoding: "utf8",
    });

  writeEnv("1");
  for (const script of [activationScript, rollbackScript]) {
    fs.writeFileSync(logPath, "", "utf8");
    const denied = run(script);
    if (denied.status === 0 || fs.readFileSync(logPath, "utf8") !== "") {
      throw new Error(
        `${path.basename(script)} must fail before systemctl without approval`
      );
    }
  }

  writeEnv("0");
  fs.writeFileSync(logPath, "", "utf8");
  const wrongFlag = run(activationScript, {
    QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_ACTIVATION:
      "approved-production-qiwe-image-send",
  });
  if (wrongFlag.status === 0 || fs.readFileSync(logPath, "utf8") !== "") {
    throw new Error("activation must fail before systemctl when env is invalid");
  }

  writeEnv("1");
  fs.writeFileSync(logPath, "", "utf8");
  const preflightRejected = run(activationScript, {
    QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_ACTIVATION:
      "approved-production-qiwe-image-send",
    SYSTEMCTL_FAIL_PREFLIGHT: "1",
  });
  const rejectedActivationLog = fs.readFileSync(logPath, "utf8");
  if (
    preflightRejected.status === 0 ||
    !rejectedActivationLog.includes(
      "start qintopia-agentos-qiwe-image-send-preflight.service"
    ) ||
    rejectedActivationLog.includes(
      "enable --now qintopia-agentos-qiwe-image-send-worker.timer"
    )
  ) {
    throw new Error(
      "activation must stop before enabling the timer when production preflight fails"
    );
  }

  fs.writeFileSync(logPath, "", "utf8");
  const activated = run(activationScript, {
    QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_ACTIVATION:
      "approved-production-qiwe-image-send",
  });
  if (activated.status !== 0) {
    throw new Error(`activation failed\n${activated.stdout}\n${activated.stderr}`);
  }
  const activationLog = fs.readFileSync(logPath, "utf8");
  for (const command of [
    "start qintopia-agentos-qiwe-image-send-preflight.service",
    "enable --now qintopia-agentos-qiwe-image-send-worker.timer",
    "is-enabled --quiet qintopia-agentos-qiwe-image-send-worker.timer",
    "is-active --quiet qintopia-agentos-qiwe-image-send-worker.timer",
  ]) {
    if (!activationLog.includes(command)) {
      throw new Error(`activation is missing systemctl command: ${command}`);
    }
  }

  writeEnv("0");
  fs.writeFileSync(logPath, "", "utf8");
  const rolledBack = run(rollbackScript, {
    QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_ROLLBACK:
      "approved-production-qiwe-image-send-rollback",
  });
  if (rolledBack.status !== 0) {
    throw new Error(`rollback failed\n${rolledBack.stdout}\n${rolledBack.stderr}`);
  }
  const rollbackLog = fs.readFileSync(logPath, "utf8");
  for (const command of [
    "disable --now qintopia-agentos-qiwe-image-send-worker.timer",
    "stop qintopia-agentos-qiwe-image-send-worker.service",
    "reset-failed qintopia-agentos-qiwe-image-send-worker.service",
  ]) {
    if (!rollbackLog.includes(command)) {
      throw new Error(`rollback is missing systemctl command: ${command}`);
    }
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("QiWe image production activation test passed.");
