#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "qintopia-image-activation-"));
const activationScript = path.join(
  repoRoot,
  "deploy/sidecar/scripts/activate-huabaosi-image-generation-production.sh"
);
const rollbackScript = path.join(
  repoRoot,
  "deploy/sidecar/scripts/rollback-huabaosi-image-generation-production.sh"
);

try {
  const logPath = path.join(tmpRoot, "systemctl.log");
  const systemctl = path.join(tmpRoot, "systemctl");
  fs.writeFileSync(
    systemctl,
    `#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"${logPath}"
`,
    "utf8"
  );
  fs.chmodSync(systemctl, 0o755);

  const run = (script, extraEnv = {}) =>
    spawnSync("bash", [script], {
      cwd: repoRoot,
      env: { ...process.env, SYSTEMCTL: systemctl, ...extraEnv },
      encoding: "utf8",
    });

  for (const script of [activationScript, rollbackScript]) {
    fs.writeFileSync(logPath, "", "utf8");
    const denied = run(script);
    if (denied.status === 0 || fs.readFileSync(logPath, "utf8") !== "") {
      throw new Error(
        `${path.basename(script)} must fail before systemctl without approval`
      );
    }
  }

  fs.writeFileSync(logPath, "", "utf8");
  const activated = run(activationScript, {
    QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_ACTIVATION:
      "approved-production-image-generation",
  });
  if (activated.status !== 0) {
    throw new Error(`activation failed\n${activated.stdout}\n${activated.stderr}`);
  }
  const activationLog = fs.readFileSync(logPath, "utf8");
  for (const command of [
    "start qintopia-agentos-huabaosi-image-generation-preflight.service",
    "enable --now qintopia-agentos-huabaosi-image-generation-worker.timer",
    "is-enabled --quiet qintopia-agentos-huabaosi-image-generation-worker.timer",
    "is-active --quiet qintopia-agentos-huabaosi-image-generation-worker.timer",
  ]) {
    if (!activationLog.includes(command)) {
      throw new Error(`activation is missing systemctl command: ${command}`);
    }
  }

  fs.writeFileSync(logPath, "", "utf8");
  const rolledBack = run(rollbackScript, {
    QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_ROLLBACK:
      "approved-production-image-generation-rollback",
  });
  if (rolledBack.status !== 0) {
    throw new Error(`rollback failed\n${rolledBack.stdout}\n${rolledBack.stderr}`);
  }
  const rollbackLog = fs.readFileSync(logPath, "utf8");
  for (const command of [
    "disable --now qintopia-agentos-huabaosi-image-generation-worker.timer",
    "stop qintopia-agentos-huabaosi-image-generation-worker.service",
    "reset-failed qintopia-agentos-huabaosi-image-generation-worker.service",
  ]) {
    if (!rollbackLog.includes(command)) {
      throw new Error(`rollback is missing systemctl command: ${command}`);
    }
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Huabaosi image production activation test passed.");
