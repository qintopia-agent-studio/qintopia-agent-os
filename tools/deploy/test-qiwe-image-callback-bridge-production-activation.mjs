#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpParent = fs.existsSync("/private/tmp") ? "/private/tmp" : "/tmp";
const tmpRoot = fs.mkdtempSync(
  path.join(tmpParent, "qintopia-qiwe-callback-activation-")
);
const activationScript = path.join(
  repoRoot,
  "deploy/sidecar/scripts/activate-qiwe-image-callback-bridge-production.sh"
);
const rollbackScript = path.join(
  repoRoot,
  "deploy/sidecar/scripts/rollback-qiwe-image-callback-bridge-production.sh"
);
const observationScript = path.join(
  repoRoot,
  "deploy/sidecar/scripts/qiwe-image-callback-bridge-production-observation-smoke.sh"
);

const writeExecutable = (filePath, content) => {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, content, "utf8");
  fs.chmodSync(filePath, 0o755);
};
const envLine = (key, value) => `${key}=${value}`;
const sha256 = (value) => crypto.createHash("sha256").update(value).digest("hex");

try {
  const databaseUrl =
    "postgres://fixture-user:fixture-password@127.0.0.1:55432/qintopia";
  const releaseSha = "0123456789abcdef0123456789abcdef01234567";
  const releaseRoot = path.join(tmpRoot, "releases");
  const fixtureScriptDir = path.join(tmpRoot, "scripts");
  const fixtureActivationScript = path.join(
    fixtureScriptDir,
    "activate-qiwe-image-callback-bridge-production.sh"
  );
  const fixtureRollbackScript = path.join(
    fixtureScriptDir,
    "rollback-qiwe-image-callback-bridge-production.sh"
  );
  const fixtureObservationScript = path.join(
    fixtureScriptDir,
    "qiwe-image-callback-bridge-production-observation-smoke.sh"
  );
  const releaseDir = path.join(releaseRoot, releaseSha);
  const currentRelease = path.join(releaseRoot, "current");
  const sidecar = path.join(releaseDir, "sidecar", "qintopia-message-sidecar");
  const bridge = path.join(releaseDir, "skills", "qiwe", "image_callback_bridge.py");
  const plugin = path.join(
    tmpRoot,
    "home/ubuntu/.hermes/profiles/erhua/plugins/qiwe-platform"
  );
  const logPath = path.join(tmpRoot, "runuser.log");
  const runuser = path.join(tmpRoot, "runuser");

  fs.mkdirSync(fixtureScriptDir, { recursive: true });
  for (const [source, target] of [
    [activationScript, fixtureActivationScript],
    [rollbackScript, fixtureRollbackScript],
    [observationScript, fixtureObservationScript],
  ]) {
    fs.copyFileSync(source, target);
    fs.chmodSync(target, 0o755);
  }

  writeExecutable(sidecar, "#!/usr/bin/env bash\nexit 70\n");
  fs.mkdirSync(path.dirname(bridge), { recursive: true });
  fs.writeFileSync(bridge, "# fixture bridge\n", "utf8");
  fs.writeFileSync(
    path.join(releaseDir, "sidecar", "artifact-manifest.json"),
    `${JSON.stringify(
      {
        commit_sha: releaseSha,
        validation: {
          cargo_features: [
            "huabaosi-production-adapter",
            "huabaosi-feishu-mirror-adapter",
            "qiwe-production-adapter",
          ],
        },
      },
      null,
      2
    )}\n`,
    "utf8"
  );
  fs.symlinkSync(releaseDir, currentRelease);
  fs.mkdirSync(path.dirname(plugin), { recursive: true });
  fs.symlinkSync(path.join(currentRelease, "skills", "qiwe"), plugin);

  writeExecutable(
    runuser,
    `#!/usr/bin/env bash
set -euo pipefail
printf '%s\\n' "$*" >>"${logPath}"
case "$*" in
  *"systemctl --user restart hermes-gateway-erhua.service"*|*"systemctl --user is-active --quiet hermes-gateway-erhua.service"*) exit 0 ;;
  *) exit 64 ;;
esac
`
  );

  const sidecarHash = sha256(fs.readFileSync(sidecar));
  const databaseHash = sha256(databaseUrl);
  const enabledEnv = path.join(tmpRoot, "enabled.env");
  const disabledEnv = path.join(tmpRoot, "disabled.env");
  const mismatchEnv = path.join(tmpRoot, "mismatch.env");
  const enabledLines = [
    envLine("QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_ENABLED", "1"),
    envLine("QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_MODE", "production"),
    envLine(
      "QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_BIN",
      path.join(currentRelease, "sidecar", "qintopia-message-sidecar")
    ),
    envLine("QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_ROOT", currentRelease),
    envLine("QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_SHA256", sidecarHash),
    envLine("QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_TIMEOUT_SECONDS", "30"),
    envLine("QINTOPIA_SIDECAR_DATABASE_URL", databaseUrl),
    envLine("QIWE_API_URL", "https://manager.qiweapi.com/qiwe/api/qw/doApi"),
    envLine("QIWE_TOKEN", "fixture-qiwe-token-secret"),
    envLine("QIWE_GUID", "fixture-qiwe-guid-secret"),
    envLine("QINTOPIA_QIWE_IMAGE_SEND_ALLOWED_HOSTS", "manager.qiweapi.com"),
    envLine("QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS", "media.example.test"),
    envLine("QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS", "reviewed-group-secret"),
    envLine("QINTOPIA_QIWE_IMAGE_SEND_ENABLED", "1"),
    envLine("QINTOPIA_QIWE_IMAGE_SEND_WEBHOOK_READY", "1"),
    envLine(
      "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_APPROVAL",
      "approved-production-qiwe-image-send"
    ),
    envLine("QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256", databaseHash),
    "",
  ];
  fs.writeFileSync(enabledEnv, enabledLines.join("\n"), "utf8");
  fs.writeFileSync(
    disabledEnv,
    ["QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_ENABLED=0", ""].join("\n"),
    "utf8"
  );
  fs.writeFileSync(
    mismatchEnv,
    enabledLines
      .map((line) =>
        line.startsWith("QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256=")
          ? envLine(
              "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256",
              "a".repeat(64)
            )
          : line
      )
      .join("\n"),
    "utf8"
  );

  const commonEnv = {
    ...process.env,
    RUNUSER_BIN: runuser,
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_OBSERVATION_TEST_MODE: "1",
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_OBSERVATION_TEST_ROOT: tmpRoot,
    QINTOPIA_RELEASE_CURRENT_DIR: currentRelease,
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PLUGIN_PATH: plugin,
  };
  const run = (script, extraEnv = {}) =>
    spawnSync("bash", [script], {
      cwd: repoRoot,
      env: { ...commonEnv, ...extraEnv },
      encoding: "utf8",
    });

  fs.writeFileSync(logPath, "", "utf8");
  const pollutedProductionScript = run(activationScript, {
    RUNUSER_BIN: runuser,
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_ACTIVATION_TEST_MODE: "1",
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_ACTIVATION_TEST_ROOT: tmpRoot,
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_ACTIVATION:
      "approved-production-qiwe-image-callback-bridge",
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_ENV_FILE: enabledEnv,
  });
  if (
    pollutedProductionScript.status === 0 ||
    fs.readFileSync(logPath, "utf8") !== ""
  ) {
    throw new Error(
      "activation production script must reject test mode before runuser"
    );
  }

  fs.writeFileSync(logPath, "", "utf8");
  const denied = run(fixtureActivationScript, {
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_ACTIVATION_TEST_MODE: "1",
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_ACTIVATION_TEST_ROOT: tmpRoot,
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_ENV_FILE: enabledEnv,
  });
  if (denied.status === 0 || fs.readFileSync(logPath, "utf8") !== "") {
    throw new Error("activation must fail before runuser without owner approval");
  }

  fs.writeFileSync(logPath, "", "utf8");
  const mismatch = run(fixtureActivationScript, {
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_ACTIVATION_TEST_MODE: "1",
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_ACTIVATION_TEST_ROOT: tmpRoot,
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_ACTIVATION:
      "approved-production-qiwe-image-callback-bridge",
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_ENV_FILE: mismatchEnv,
  });
  if (
    mismatch.status === 0 ||
    fs.readFileSync(logPath, "utf8") !== "" ||
    `${mismatch.stdout}\n${mismatch.stderr}`.includes(databaseUrl)
  ) {
    throw new Error("activation must fail closed without restarting or leaking DB URL");
  }

  fs.writeFileSync(logPath, "", "utf8");
  const activated = run(fixtureActivationScript, {
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_ACTIVATION_TEST_MODE: "1",
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_ACTIVATION_TEST_ROOT: tmpRoot,
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_ACTIVATION:
      "approved-production-qiwe-image-callback-bridge",
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_ENV_FILE: enabledEnv,
  });
  if (activated.status !== 0) {
    throw new Error(`activation failed\n${activated.stdout}\n${activated.stderr}`);
  }
  const activationLog = fs.readFileSync(logPath, "utf8");
  for (const command of [
    "systemctl --user restart hermes-gateway-erhua.service",
    "systemctl --user is-active --quiet hermes-gateway-erhua.service",
  ]) {
    if (!activationLog.includes(command)) {
      throw new Error(`activation did not restart/check Erhua: ${command}`);
    }
  }

  fs.writeFileSync(logPath, "", "utf8");
  const pollutedRollbackScript = run(rollbackScript, {
    RUNUSER_BIN: runuser,
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_ROLLBACK_TEST_MODE: "1",
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_ROLLBACK_TEST_ROOT: tmpRoot,
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_ROLLBACK:
      "approved-production-qiwe-image-callback-bridge-rollback",
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_ENV_FILE: disabledEnv,
  });
  if (pollutedRollbackScript.status === 0 || fs.readFileSync(logPath, "utf8") !== "") {
    throw new Error("rollback production script must reject test mode before runuser");
  }

  fs.writeFileSync(logPath, "", "utf8");
  const rolledBack = run(fixtureRollbackScript, {
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_ROLLBACK_TEST_MODE: "1",
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_ROLLBACK_TEST_ROOT: tmpRoot,
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_ROLLBACK:
      "approved-production-qiwe-image-callback-bridge-rollback",
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_ENV_FILE: disabledEnv,
  });
  if (rolledBack.status !== 0) {
    throw new Error(`rollback failed\n${rolledBack.stdout}\n${rolledBack.stderr}`);
  }
  const rollbackLog = fs.readFileSync(logPath, "utf8");
  if (
    !rollbackLog.includes("systemctl --user restart hermes-gateway-erhua.service") ||
    !rollbackLog.includes(
      "systemctl --user is-active --quiet hermes-gateway-erhua.service"
    )
  ) {
    throw new Error("rollback did not restart and verify Erhua");
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("QiWe image callback bridge production activation test passed.");
