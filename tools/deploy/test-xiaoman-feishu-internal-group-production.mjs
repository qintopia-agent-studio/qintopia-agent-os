#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpParent = fs.existsSync("/private/tmp") ? "/private/tmp" : os.tmpdir();
const tmpRoot = fs.mkdtempSync(
  path.join(tmpParent, "qintopia-xiaoman-internal-group-")
);
const scriptNames = [
  "xiaoman-feishu-internal-group-production-observation-smoke.sh",
  "activate-xiaoman-feishu-internal-group-production.sh",
  "rollback-xiaoman-feishu-internal-group-production.sh",
];
const sourceDir = path.join(repoRoot, "deploy", "sidecar", "scripts");

const shellDoubleQuoted = (value) =>
  `"${String(value)
    .replaceAll("\\", "\\\\")
    .replaceAll('"', '\\"')
    .replaceAll("$", "\\$")
    .replaceAll("`", "\\`")}"`;
const writeExecutable = (filePath, content) => {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, content, "utf8");
  fs.chmodSync(filePath, 0o755);
};
const writeEnv = (filePath, entries) => {
  fs.writeFileSync(
    filePath,
    `${Object.entries(entries)
      .map(([key, value]) => `${key}=${value}`)
      .join("\n")}\n`,
    "utf8"
  );
  fs.chmodSync(filePath, 0o600);
};

try {
  const python = spawnSync("which", ["python3"], { encoding: "utf8" }).stdout.trim();
  if (!python) {
    throw new Error("python3 is required for the internal-group fixture");
  }

  const fixtureDir = path.join(tmpRoot, "scripts");
  const systemctl = path.join(tmpRoot, "systemctl");
  const runuser = path.join(tmpRoot, "runuser");
  const logPath = path.join(tmpRoot, "commands.log");
  const groupDeliveryStoppedMarker = path.join(tmpRoot, "group-delivery-stopped");
  const groupDeliveryRestartFailureMarker = path.join(
    tmpRoot,
    "fail-group-delivery-restart"
  );
  const restartFailureMarker = path.join(tmpRoot, "fail-xiaoman-restart");
  const sidecarEnv = path.join(tmpRoot, "message-sidecar.env");
  const hermesEnv = path.join(tmpRoot, "xiaoman.env");
  const releaseSha = "a".repeat(40);
  const releaseRoot = path.join(tmpRoot, releaseSha);
  const releaseCurrent = path.join(tmpRoot, "current");
  const sidecarDir = path.join(releaseRoot, "sidecar");
  const sidecarBin = path.join(sidecarDir, "qintopia-message-sidecar");
  const releasePlugin = path.join(
    releaseRoot,
    "skills",
    "qintopia-tools",
    "variants",
    "xiaoman"
  );
  const profilePlugin = path.join(tmpRoot, "profile", "plugins", "qintopia-tools");

  fs.mkdirSync(sidecarDir, { recursive: true });
  writeExecutable(sidecarBin, "#!/usr/bin/env bash\nexit 0\n");
  fs.writeFileSync(
    path.join(sidecarDir, "artifact-manifest.json"),
    `${JSON.stringify(
      {
        commit_sha: releaseSha,
        validation: {
          artifact_profile: "huabaosi-production",
          cargo_features: [
            "huabaosi-production-adapter",
            "huabaosi-feishu-mirror-adapter",
            "xiaoman-feishu-poster-adapter",
          ],
        },
      },
      null,
      2
    )}\n`,
    "utf8"
  );
  fs.mkdirSync(releasePlugin, { recursive: true });
  fs.writeFileSync(path.join(releasePlugin, "__init__.py"), "# fixture\n", "utf8");
  fs.mkdirSync(path.dirname(profilePlugin), { recursive: true });
  fs.symlinkSync(releasePlugin, profilePlugin);
  fs.symlinkSync(releaseRoot, releaseCurrent);
  fs.writeFileSync(logPath, "", "utf8");

  writeExecutable(
    systemctl,
    `#!/usr/bin/env bash
set -euo pipefail
printf 'systemctl %s\\n' "$*" >>${shellDoubleQuoted(logPath)}
if [[ "$*" == "disable --now qintopia-agentos-xiaoman-feishu-internal-group-poster-delivery.timer" ]]; then
  touch ${shellDoubleQuoted(groupDeliveryStoppedMarker)}
  exit 0
fi
if [[ "$*" == "enable qintopia-agentos-xiaoman-feishu-internal-group-poster-delivery.timer" ]]; then
  rm -f ${shellDoubleQuoted(groupDeliveryStoppedMarker)}
  exit 0
fi
if [[ -f ${shellDoubleQuoted(groupDeliveryRestartFailureMarker)} && "$*" == "restart qintopia-agentos-xiaoman-feishu-internal-group-poster-delivery.timer" ]]; then
  exit 65
fi
if [[ -f ${shellDoubleQuoted(groupDeliveryStoppedMarker)} && "$*" == "is-enabled --quiet qintopia-agentos-xiaoman-feishu-internal-group-poster-delivery.timer" ]]; then
  exit 1
fi
if [[ -f ${shellDoubleQuoted(groupDeliveryStoppedMarker)} && "$*" == "is-active --quiet qintopia-agentos-xiaoman-feishu-internal-group-poster-delivery.timer" ]]; then
  exit 3
fi
if [[ "$*" == "show --property=NextElapseUSecMonotonic --value qintopia-agentos-xiaoman-feishu-poster-delivery.timer" || "$*" == "show --property=NextElapseUSecMonotonic --value qintopia-agentos-xiaoman-feishu-internal-group-poster-delivery.timer" ]]; then
  echo "2min"
fi
exit 0
`
  );
  writeExecutable(
    runuser,
    `#!/usr/bin/env bash
set -euo pipefail
if [[ -n "\${QINTOPIA_UNRELATED_RUNTIME_SECRET:-}" ]]; then
  echo "ambient secret reached runuser" >&2
  exit 23
fi
printf 'runuser %s\\n' "$*" >>${shellDoubleQuoted(logPath)}
case "$*" in
  *"systemctl --user restart hermes-gateway-xiaoman.service"*)
    if [[ -f ${shellDoubleQuoted(restartFailureMarker)} ]]; then
      exit 65
    fi
    exit 0
    ;;
  *"systemctl --user is-active --quiet hermes-gateway-xiaoman.service"*) exit 0 ;;
  *) exit 64 ;;
esac
`
  );

  const replacements = new Map([
    ['SYSTEMCTL="/usr/bin/systemctl"', `SYSTEMCTL=${shellDoubleQuoted(systemctl)}`],
    ['PYTHON_BIN="/usr/bin/python3"', `PYTHON_BIN=${shellDoubleQuoted(python)}`],
    ['RUNUSER_BIN="/usr/sbin/runuser"', `RUNUSER_BIN=${shellDoubleQuoted(runuser)}`],
    [
      'RELEASE_CURRENT_DIR="/home/ubuntu/qintopia-agent-os-releases/current"',
      `RELEASE_CURRENT_DIR=${shellDoubleQuoted(releaseCurrent)}`,
    ],
    [
      'SIDECAR_ENV_FILE="/etc/qintopia/message-sidecar.env"',
      `SIDECAR_ENV_FILE=${shellDoubleQuoted(sidecarEnv)}`,
    ],
    [
      'HERMES_ENV_FILE="/home/ubuntu/.hermes/profiles/xiaoman/.env"',
      `HERMES_ENV_FILE=${shellDoubleQuoted(hermesEnv)}`,
    ],
    [
      'HERMES_PLUGIN_PATH="/home/ubuntu/.hermes/profiles/xiaoman/plugins/qintopia-tools"',
      `HERMES_PLUGIN_PATH=${shellDoubleQuoted(profilePlugin)}`,
    ],
  ]);
  for (const scriptName of scriptNames) {
    const sourcePath = path.join(sourceDir, scriptName);
    let content = fs.readFileSync(sourcePath, "utf8");
    for (const forbidden of [
      "TEST_MODE",
      "_TEST_MODE",
      "SYSTEMCTL:-",
      "RUNUSER_BIN:-",
      "source ",
      "eval ",
      "curl ",
      "psql ",
    ]) {
      if (content.includes(forbidden)) {
        throw new Error(
          `${scriptName} contains unsafe production fragment ${forbidden}`
        );
      }
    }
    for (const [from, to] of replacements) {
      content = content.replaceAll(from, to);
    }
    writeExecutable(path.join(fixtureDir, scriptName), content);
  }

  const observation = path.join(
    fixtureDir,
    "xiaoman-feishu-internal-group-production-observation-smoke.sh"
  );
  const activation = path.join(
    fixtureDir,
    "activate-xiaoman-feishu-internal-group-production.sh"
  );
  const rollback = path.join(
    fixtureDir,
    "rollback-xiaoman-feishu-internal-group-production.sh"
  );
  const ingressKey = "fixture-ingress-key-that-is-longer-than-32-bytes";
  const callbackKey = "fixture-callback-key";
  const commonSidecar = {
    QINTOPIA_XIAOMAN_FEISHU_POSTER_ENABLED: "1",
    QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE: "1",
    QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY: ingressKey,
    QINTOPIA_XIAOMAN_FEISHU_CALLBACK_ENCRYPT_KEY: callbackKey,
    QINTOPIA_XIAOMAN_FEISHU_BOT_OPEN_ID: "ou_xiaoman_bot",
    QINTOPIA_XIAOMAN_FEISHU_INGRESS_ALLOWED_CHAT_IDS: "oc_private,oc_internal",
    QINTOPIA_XIAOMAN_FEISHU_INGRESS_ALLOWED_USER_IDS: "ou_requester,ou_reviewer",
    QINTOPIA_XIAOMAN_FEISHU_ALLOWED_CHAT_IDS: "oc_private,oc_internal",
    QINTOPIA_XIAOMAN_FEISHU_ALLOWED_USER_IDS: "ou_requester,ou_reviewer",
    QINTOPIA_OPERATIONS_ALLOWED_REVIEWER_IDS: "ou_requester,ou_reviewer",
  };
  const commonHermes = {
    QINTOPIA_XIAOMAN_POSTER_REVIEW_HOOK_ENABLE: "1",
    QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE: "1",
    QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY: ingressKey,
    QINTOPIA_XIAOMAN_FEISHU_CALLBACK_ENCRYPT_KEY: callbackKey,
    QINTOPIA_XIAOMAN_FEISHU_BOT_OPEN_ID: "ou_xiaoman_bot",
  };
  const setGroupState = (state) => {
    writeEnv(sidecarEnv, {
      ...commonSidecar,
      QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED: state,
    });
    writeEnv(hermesEnv, {
      ...commonHermes,
      QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED: state,
    });
  };
  const commonEnv = {
    ...process.env,
    QINTOPIA_UNRELATED_RUNTIME_SECRET: "must-not-reach-runuser",
  };
  const run = (script, extraEnv = {}) =>
    spawnSync("bash", [script], {
      cwd: repoRoot,
      env: { ...commonEnv, ...extraEnv },
      encoding: "utf8",
    });
  const resetLog = () => fs.writeFileSync(logPath, "", "utf8");
  const commandLog = () => fs.readFileSync(logPath, "utf8");

  setGroupState("1");
  fs.writeFileSync(groupDeliveryStoppedMarker, "stopped\n", "utf8");
  let result = run(observation);
  if (result.status !== 0 || commandLog() !== "") {
    throw new Error("disabled observation entrypoint must skip without systemd access");
  }

  resetLog();
  result = run(observation, {
    QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_PRODUCTION_OBSERVATION_ENABLE: "1",
    QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_EXPECTED_STATE: "enabled",
    QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_DELIVERY_EXPECTED_STATE: "stopped",
  });
  if (result.status !== 0) {
    throw new Error(`enabled observation failed\n${result.stdout}\n${result.stderr}`);
  }
  const observedOutput = `${result.stdout}\n${result.stderr}`;
  for (const sensitive of [
    ingressKey,
    callbackKey,
    "oc_private",
    "oc_internal",
    "ou_requester",
    "ou_reviewer",
    tmpRoot,
  ]) {
    if (observedOutput.includes(sensitive)) {
      throw new Error(`observation disclosed sensitive fixture value ${sensitive}`);
    }
  }
  if (
    !observedOutput.includes(
      "xiaoman_feishu_internal_group_production_observation_state=enabled"
    ) ||
    !commandLog().includes(
      "systemctl is-active --quiet qintopia-agentos-xiaoman-feishu-poster-delivery.timer"
    ) ||
    !commandLog().includes(
      "systemctl is-active --quiet qintopia-agentos-xiaoman-feishu-internal-group-poster-delivery.timer"
    )
  ) {
    throw new Error("enabled observation did not prove the split delivery boundaries");
  }

  resetLog();
  result = run(observation, {
    QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_PRODUCTION_OBSERVATION_ENABLE: "1",
    QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_EXPECTED_STATE: "disabled",
  });
  if (result.status === 0 || commandLog() !== "") {
    throw new Error("state mismatch must fail before systemd inspection");
  }

  resetLog();
  result = run(activation);
  if (result.status === 0 || commandLog() !== "") {
    throw new Error("activation must fail before side effects without owner approval");
  }

  resetLog();
  result = run(activation, {
    QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_PRODUCTION_ACTIVATION:
      "approved-production-xiaoman-feishu-internal-group",
  });
  if (result.status !== 0) {
    throw new Error(`activation failed\n${result.stdout}\n${result.stderr}`);
  }
  const activationLog = commandLog();
  const activationOrder = [
    "systemctl start qintopia-agentos-xiaoman-feishu-internal-group-poster-preflight.service",
    "systemctl --user restart hermes-gateway-xiaoman.service",
    "systemctl restart qintopia-agentos-operations-intake.service",
    "systemctl restart qintopia-agentos-xiaoman-poster-review-callback.service",
    "systemctl enable qintopia-agentos-xiaoman-feishu-internal-group-poster-delivery.timer",
    "systemctl restart qintopia-agentos-xiaoman-feishu-internal-group-poster-delivery.timer",
  ];
  let previousIndex = -1;
  for (const command of activationOrder) {
    const index = activationLog.indexOf(command);
    if (index <= previousIndex) {
      throw new Error(`activation command missing or out of order: ${command}`);
    }
    previousIndex = index;
  }
  if (activationLog.includes("must-not-reach-runuser")) {
    throw new Error("activation leaked ambient environment to runuser");
  }
  for (const forbidden of [
    "systemctl disable --now qintopia-agentos-xiaoman-feishu-poster-delivery.timer",
    "systemctl enable qintopia-agentos-xiaoman-feishu-poster-delivery.timer",
    "systemctl restart qintopia-agentos-xiaoman-feishu-poster-delivery.timer",
  ]) {
    if (activationLog.includes(forbidden)) {
      throw new Error(`group activation mutated direct delivery: ${forbidden}`);
    }
  }

  resetLog();
  fs.writeFileSync(groupDeliveryStoppedMarker, "stopped\n", "utf8");
  fs.writeFileSync(groupDeliveryRestartFailureMarker, "fail\n", "utf8");
  result = run(activation, {
    QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_PRODUCTION_ACTIVATION:
      "approved-production-xiaoman-feishu-internal-group",
  });
  const failedTimerRestartLog = commandLog();
  if (
    result.status === 0 ||
    !failedTimerRestartLog.includes(
      "systemctl enable qintopia-agentos-xiaoman-feishu-internal-group-poster-delivery.timer"
    ) ||
    !failedTimerRestartLog.includes(
      "systemctl restart qintopia-agentos-xiaoman-feishu-internal-group-poster-delivery.timer"
    ) ||
    !failedTimerRestartLog.includes(
      "systemctl disable --now qintopia-agentos-xiaoman-feishu-internal-group-poster-delivery.timer"
    ) ||
    !failedTimerRestartLog.includes(
      "systemctl stop qintopia-agentos-xiaoman-feishu-internal-group-poster-delivery.service"
    ) ||
    failedTimerRestartLog.includes(
      "systemctl disable --now qintopia-agentos-xiaoman-feishu-poster-delivery.timer"
    )
  ) {
    throw new Error(
      "failed group timer restart must clean up group delivery without mutating direct delivery"
    );
  }
  fs.unlinkSync(groupDeliveryRestartFailureMarker);

  resetLog();
  fs.writeFileSync(groupDeliveryStoppedMarker, "stopped\n", "utf8");
  fs.writeFileSync(restartFailureMarker, "fail\n", "utf8");
  result = run(activation, {
    QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_PRODUCTION_ACTIVATION:
      "approved-production-xiaoman-feishu-internal-group",
  });
  const failedActivationLog = commandLog();
  if (
    result.status === 0 ||
    failedActivationLog.includes(
      "systemctl restart qintopia-agentos-operations-intake.service"
    ) ||
    failedActivationLog.includes(
      "systemctl restart qintopia-agentos-xiaoman-feishu-internal-group-poster-delivery.timer"
    ) ||
    failedActivationLog.includes(
      "systemctl restart qintopia-agentos-xiaoman-feishu-poster-delivery.timer"
    )
  ) {
    throw new Error("failed gateway reload must leave delivery stopped");
  }
  fs.unlinkSync(restartFailureMarker);

  resetLog();
  result = run(rollback, {
    QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_PRODUCTION_ROLLBACK:
      "approved-production-xiaoman-feishu-internal-group-rollback",
  });
  const enabledRollbackLog = commandLog();
  if (
    result.status === 0 ||
    enabledRollbackLog !==
      "systemctl disable --now qintopia-agentos-xiaoman-feishu-internal-group-poster-delivery.timer\n"
  ) {
    throw new Error(
      "rollback must stop delivery then reject persistent enabled state before reload"
    );
  }

  setGroupState("0");
  resetLog();
  result = run(rollback);
  if (result.status === 0 || commandLog() !== "") {
    throw new Error("rollback must fail before side effects without owner approval");
  }

  resetLog();
  result = run(rollback, {
    QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_PRODUCTION_ROLLBACK:
      "approved-production-xiaoman-feishu-internal-group-rollback",
  });
  if (result.status !== 0) {
    throw new Error(`rollback failed\n${result.stdout}\n${result.stderr}`);
  }
  const rollbackLog = commandLog();
  for (const command of [
    "systemctl start qintopia-agentos-xiaoman-feishu-poster-preflight.service",
    "systemctl disable --now qintopia-agentos-xiaoman-feishu-internal-group-poster-delivery.timer",
    "systemctl --user restart hermes-gateway-xiaoman.service",
    "systemctl restart qintopia-agentos-operations-intake.service",
    "systemctl restart qintopia-agentos-xiaoman-poster-review-callback.service",
  ]) {
    if (!rollbackLog.includes(command)) {
      throw new Error(`rollback command missing: ${command}`);
    }
  }
  for (const forbidden of [
    "systemctl disable --now qintopia-agentos-xiaoman-feishu-poster-delivery.timer",
    "systemctl enable qintopia-agentos-xiaoman-feishu-poster-delivery.timer",
    "systemctl restart qintopia-agentos-xiaoman-feishu-poster-delivery.timer",
    "systemctl enable qintopia-agentos-xiaoman-feishu-internal-group-poster-delivery.timer",
    "systemctl restart qintopia-agentos-xiaoman-feishu-internal-group-poster-delivery.timer",
  ]) {
    if (rollbackLog.includes(forbidden)) {
      throw new Error(`group rollback crossed its delivery boundary: ${forbidden}`);
    }
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Xiaoman Feishu internal-group production control test passed.");
