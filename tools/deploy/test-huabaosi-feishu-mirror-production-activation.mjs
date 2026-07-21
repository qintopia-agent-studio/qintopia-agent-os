#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpRoot = fs.mkdtempSync(
  path.join(os.tmpdir(), "qintopia-feishu-mirror-activation-")
);
const activationScriptSource = path.join(
  repoRoot,
  "deploy/sidecar/scripts/activate-huabaosi-feishu-artifact-mirror-production.sh"
);
const rollbackScriptSource = path.join(
  repoRoot,
  "deploy/sidecar/scripts/rollback-huabaosi-feishu-artifact-mirror-production.sh"
);
const fixedEnvFile = "/etc/qintopia/message-sidecar.env";

try {
  const logPath = path.join(tmpRoot, "systemctl.log");
  const envPath = path.join(tmpRoot, "message-sidecar.env");
  const systemctl = path.join(tmpRoot, "systemctl");
  const activationScript = path.join(tmpRoot, "activate-production-fixture.sh");
  const rollbackScript = path.join(tmpRoot, "rollback-production-fixture.sh");
  for (const sourcePath of [activationScriptSource, rollbackScriptSource]) {
    const source = fs.readFileSync(sourcePath, "utf8");
    if (source.includes("QINTOPIA_SIDECAR_ENV_FILE")) {
      throw new Error(
        `${path.basename(sourcePath)} must not allow overriding the reviewed env file`
      );
    }
    if (!source.includes(`ENV_FILE="${fixedEnvFile}"`)) {
      throw new Error(
        `${path.basename(sourcePath)} must read the fixed reviewed env file`
      );
    }
  }
  fs.writeFileSync(
    activationScript,
    fs.readFileSync(activationScriptSource, "utf8").replaceAll(fixedEnvFile, envPath),
    "utf8"
  );
  fs.writeFileSync(
    rollbackScript,
    fs.readFileSync(rollbackScriptSource, "utf8").replaceAll(fixedEnvFile, envPath),
    "utf8"
  );
  fs.writeFileSync(envPath, "QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED=1\n", "utf8");
  fs.writeFileSync(
    systemctl,
    `#!/usr/bin/env bash
set -euo pipefail
printf '%s\\n' "$*" >>"${logPath}"
if [[ "\${FAKE_PREFLIGHT_FAIL:-0}" == "1" && "$1" == "start" ]]; then
  exit 1
fi
`,
    "utf8"
  );
  fs.chmodSync(systemctl, 0o755);

  const run = (script, extraEnv = {}) =>
    spawnSync("bash", [script], {
      cwd: repoRoot,
      env: {
        ...process.env,
        SYSTEMCTL: systemctl,
        ...extraEnv,
      },
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

  for (const invalidEnablement of [
    "# mirror flag omitted\n",
    "QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED=0\n",
    "QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED=1;\n",
    "QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED=1\nQINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED=1\n",
  ]) {
    fs.writeFileSync(envPath, invalidEnablement, "utf8");
    fs.writeFileSync(logPath, "", "utf8");
    const rejectedEnablement = run(activationScript, {
      QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_ACTIVATION:
        "approved-production-huabaosi-feishu-artifact-mirror",
    });
    if (rejectedEnablement.status === 0 || fs.readFileSync(logPath, "utf8") !== "") {
      throw new Error("activation accepted missing, disabled, or duplicate enablement");
    }
  }

  fs.writeFileSync(envPath, "QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED=1\n", "utf8");
  fs.writeFileSync(logPath, "", "utf8");
  const approvedActivation = run(activationScript, {
    QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_ACTIVATION:
      "approved-production-huabaosi-feishu-artifact-mirror",
  });
  if (approvedActivation.status !== 0) {
    throw new Error(
      `activation failed\n${approvedActivation.stdout}\n${approvedActivation.stderr}`
    );
  }
  const activationLog = fs.readFileSync(logPath, "utf8");
  for (const command of [
    "start qintopia-agentos-huabaosi-feishu-artifact-mirror-preflight.service",
    "enable --now qintopia-agentos-huabaosi-feishu-artifact-mirror-worker.timer",
    "is-enabled --quiet qintopia-agentos-huabaosi-feishu-artifact-mirror-worker.timer",
    "is-active --quiet qintopia-agentos-huabaosi-feishu-artifact-mirror-worker.timer",
  ]) {
    if (!activationLog.includes(command)) {
      throw new Error(`activation is missing systemctl command: ${command}`);
    }
  }
  if (
    !approvedActivation.stdout.includes(
      "Huabaosi Feishu artifact mirror production timer activated"
    )
  ) {
    throw new Error("activation did not report timer activation");
  }

  fs.writeFileSync(logPath, "", "utf8");
  const failedPreflight = run(activationScript, {
    QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_ACTIVATION:
      "approved-production-huabaosi-feishu-artifact-mirror",
    FAKE_PREFLIGHT_FAIL: "1",
  });
  if (
    failedPreflight.status === 0 ||
    fs
      .readFileSync(logPath, "utf8")
      .includes(
        "enable --now qintopia-agentos-huabaosi-feishu-artifact-mirror-worker.timer"
      )
  ) {
    throw new Error("activation must not enable the timer when preflight fails");
  }

  fs.writeFileSync(envPath, "QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED=0\n", "utf8");
  fs.writeFileSync(logPath, "", "utf8");
  const rolledBack = run(rollbackScript, {
    QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_ROLLBACK:
      "approved-production-huabaosi-feishu-artifact-mirror-rollback",
  });
  if (rolledBack.status !== 0) {
    throw new Error(`rollback failed\n${rolledBack.stdout}\n${rolledBack.stderr}`);
  }
  const rollbackLog = fs.readFileSync(logPath, "utf8");
  for (const command of [
    "disable --now qintopia-agentos-huabaosi-feishu-artifact-mirror-worker.timer",
    "stop qintopia-agentos-huabaosi-feishu-artifact-mirror-worker.service",
    "reset-failed qintopia-agentos-huabaosi-feishu-artifact-mirror-worker.service",
  ]) {
    if (!rollbackLog.includes(command)) {
      throw new Error(`rollback is missing systemctl command: ${command}`);
    }
  }

  fs.writeFileSync(envPath, "# mirror flag omitted\n", "utf8");
  fs.writeFileSync(logPath, "", "utf8");
  const missingEnablement = run(rollbackScript, {
    QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_ROLLBACK:
      "approved-production-huabaosi-feishu-artifact-mirror-rollback",
  });
  if (
    missingEnablement.status === 0 ||
    !fs
      .readFileSync(logPath, "utf8")
      .includes(
        "disable --now qintopia-agentos-huabaosi-feishu-artifact-mirror-worker.timer"
      )
  ) {
    throw new Error(
      "rollback must stop the timer but fail when persistent enablement is missing"
    );
  }

  fs.writeFileSync(envPath, "QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED=1\n", "utf8");
  fs.writeFileSync(logPath, "", "utf8");
  const persistentEnablement = run(rollbackScript, {
    QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_ROLLBACK:
      "approved-production-huabaosi-feishu-artifact-mirror-rollback",
  });
  const persistentEnablementLog = fs.readFileSync(logPath, "utf8");
  if (
    persistentEnablement.status === 0 ||
    !persistentEnablementLog.includes(
      "disable --now qintopia-agentos-huabaosi-feishu-artifact-mirror-worker.timer"
    ) ||
    persistentEnablement.stdout.includes("production timer disabled")
  ) {
    throw new Error(
      "rollback must stop the timer but fail until persistent enablement is disabled"
    );
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Huabaosi Feishu mirror production activation test passed.");
