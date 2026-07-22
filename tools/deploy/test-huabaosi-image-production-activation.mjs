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
const fixedSystemctl = "/usr/bin/systemctl";

try {
  const logPath = path.join(tmpRoot, "systemctl.log");
  const systemctl = path.join(tmpRoot, "systemctl");
  const activationFixture = path.join(tmpRoot, "activate-production-fixture.sh");
  const rollbackFixture = path.join(tmpRoot, "rollback-production-fixture.sh");

  for (const sourcePath of [activationScript, rollbackScript]) {
    const source = fs.readFileSync(sourcePath, "utf8");
    if (source.includes('SYSTEMCTL="${SYSTEMCTL:-systemctl}"')) {
      throw new Error(
        `${path.basename(sourcePath)} must not allow overriding systemctl`
      );
    }
    if (!source.includes('PATH="/usr/bin:/bin:/usr/sbin:/sbin"')) {
      throw new Error(`${path.basename(sourcePath)} must reset PATH`);
    }
    if (!source.includes(`SYSTEMCTL="${fixedSystemctl}"`)) {
      throw new Error(`${path.basename(sourcePath)} must use the fixed systemctl path`);
    }
  }

  fs.writeFileSync(
    activationFixture,
    fs.readFileSync(activationScript, "utf8").replaceAll(fixedSystemctl, systemctl),
    "utf8"
  );
  fs.writeFileSync(
    rollbackFixture,
    fs.readFileSync(rollbackScript, "utf8").replaceAll(fixedSystemctl, systemctl),
    "utf8"
  );
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
      env: { ...process.env, ...extraEnv },
      encoding: "utf8",
    });

  for (const script of [activationFixture, rollbackFixture]) {
    fs.writeFileSync(logPath, "", "utf8");
    const denied = run(script);
    if (denied.status === 0 || fs.readFileSync(logPath, "utf8") !== "") {
      throw new Error(
        `${path.basename(script)} must fail before systemctl without approval`
      );
    }
  }

  fs.writeFileSync(logPath, "", "utf8");
  const activated = run(activationFixture, {
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
  const rolledBack = run(rollbackFixture, {
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
