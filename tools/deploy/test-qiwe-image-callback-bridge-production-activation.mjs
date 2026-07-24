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
const writeExecutable = (filePath, content) => {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, content, "utf8");
  fs.chmodSync(filePath, 0o755);
};
const envLine = (key, value) => `${key}=${value}`;
const sha256 = (value) => crypto.createHash("sha256").update(value).digest("hex");
const shellDoubleQuoted = (value) =>
  `"${String(value).replaceAll("\\", "\\\\").replaceAll('"', '\\"').replaceAll("$", "\\$").replaceAll("`", "\\`")}"`;

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
  const observationState = path.join(tmpRoot, "observation-state");

  fs.mkdirSync(fixtureScriptDir, { recursive: true });
  const copyProductionScriptFixture = (source, target) => {
    const sourceText = fs.readFileSync(source, "utf8");
    for (const forbidden of ["TEST_MODE", "_TEST_MODE", "RUNUSER_BIN:-"]) {
      if (sourceText.includes(forbidden)) {
        throw new Error(`production script must not contain ${forbidden}`);
      }
    }
    const fixtureText = sourceText.replace(
      'RUNUSER_BIN="/usr/sbin/runuser"',
      `RUNUSER_BIN=${shellDoubleQuoted(runuser)}`
    );
    if (fixtureText === sourceText) {
      throw new Error("fixture script did not replace the fixed runuser path");
    }
    writeExecutable(target, fixtureText);
  };
  copyProductionScriptFixture(activationScript, fixtureActivationScript);
  copyProductionScriptFixture(rollbackScript, fixtureRollbackScript);
  writeExecutable(
    fixtureObservationScript,
    `#!/usr/bin/env bash
set -euo pipefail
if [[ -n "\${QINTOPIA_UNRELATED_RUNTIME_SECRET:-}" ]]; then
  echo "ambient secret reached observation" >&2
  exit 23
fi
if [[ "\${QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_OBSERVATION_ENABLE:-}" != "1" ]]; then
  echo "fixture observation requires enable flag" >&2
  exit 62
fi
expected_state="\${QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_EXPECTED_STATE:-}"
actual_state="$(cat ${shellDoubleQuoted(observationState)})"
if [[ "$actual_state" == "fail" ]]; then
  echo "fixture observation failed closed" >&2
  exit 66
fi
if [[ "$expected_state" != "$actual_state" ]]; then
  echo "fixture observation state mismatch" >&2
  exit 67
fi
echo "fixture observation $actual_state"
`
  );

  writeExecutable(sidecar, "#!/usr/bin/env bash\nexit 70\n");
  fs.mkdirSync(path.dirname(bridge), { recursive: true });
  fs.writeFileSync(bridge, "# fixture bridge\n", "utf8");
  fs.writeFileSync(
    path.join(releaseDir, "sidecar", "artifact-manifest.json"),
    `${JSON.stringify(
      {
        commit_sha: releaseSha,
        validation: {
          artifact_profile: "qiwe-production",
          cargo_features: ["qiwe-production-adapter"],
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
if [[ -n "\${QINTOPIA_UNRELATED_RUNTIME_SECRET:-}" ]]; then
  echo "ambient secret reached runuser" >&2
  exit 23
fi
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
    QINTOPIA_UNRELATED_RUNTIME_SECRET: "must-not-reach-runuser",
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_OBSERVATION_TEST_MODE:
      "must-not-reach-observation",
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_OBSERVATION_TEST_ROOT:
      "must-not-reach-observation",
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
  fs.writeFileSync(observationState, "enabled", "utf8");
  const denied = run(fixtureActivationScript, {
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_ENV_FILE: enabledEnv,
  });
  if (denied.status === 0 || fs.readFileSync(logPath, "utf8") !== "") {
    throw new Error("activation must fail before runuser without owner approval");
  }

  fs.writeFileSync(logPath, "", "utf8");
  fs.writeFileSync(observationState, "fail", "utf8");
  const mismatch = run(fixtureActivationScript, {
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
  fs.writeFileSync(observationState, "enabled", "utf8");
  const activated = run(fixtureActivationScript, {
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
  fs.writeFileSync(observationState, "disabled", "utf8");
  const rolledBack = run(fixtureRollbackScript, {
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
