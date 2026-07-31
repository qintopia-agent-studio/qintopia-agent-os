#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpParent = fs.existsSync("/private/tmp") ? "/private/tmp" : os.tmpdir();
const tmpRoot = fs.mkdtempSync(
  path.join(tmpParent, "qintopia-xiaoman-poster-activation-")
);
const sourceActivation = path.join(
  repoRoot,
  "deploy/sidecar/scripts/activate-xiaoman-feishu-poster-production.sh"
);
const sourceRollback = path.join(
  repoRoot,
  "deploy/sidecar/scripts/rollback-xiaoman-feishu-poster-production.sh"
);

const shellDoubleQuoted = (value) =>
  `"${String(value).replaceAll("\\", "\\\\").replaceAll('"', '\\"').replaceAll("$", "\\$").replaceAll("`", "\\`")}"`;
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
    throw new Error("python3 is required for the activation fixture");
  }
  const fixtureDir = path.join(tmpRoot, "scripts");
  const activation = path.join(
    fixtureDir,
    "activate-xiaoman-feishu-poster-production.sh"
  );
  const rollback = path.join(
    fixtureDir,
    "rollback-xiaoman-feishu-poster-production.sh"
  );
  const systemctl = path.join(tmpRoot, "systemctl");
  const runuser = path.join(tmpRoot, "runuser");
  const logPath = path.join(tmpRoot, "commands.log");
  const restartFailureMarker = path.join(tmpRoot, "fail-xiaoman-restart");
  const sidecarEnv = path.join(tmpRoot, "message-sidecar.env");
  const hermesEnv = path.join(tmpRoot, "xiaoman.env");
  const releasePlugin = path.join(tmpRoot, "release", "skills", "qintopia-tools");
  const profilePlugin = path.join(tmpRoot, "profile", "plugins", "qintopia-tools");

  fs.mkdirSync(releasePlugin, { recursive: true });
  fs.writeFileSync(path.join(releasePlugin, "__init__.py"), "# fixture\n", "utf8");
  fs.mkdirSync(path.dirname(profilePlugin), { recursive: true });
  fs.symlinkSync(releasePlugin, profilePlugin);
  fs.writeFileSync(logPath, "", "utf8");

  writeExecutable(
    systemctl,
    `#!/usr/bin/env bash
set -euo pipefail
printf 'systemctl %s\n' "$*" >>${shellDoubleQuoted(logPath)}
if [[ "$*" == "show --property=NextElapseUSecMonotonic --value qintopia-agentos-xiaoman-feishu-poster-delivery.timer" ]]; then
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
printf 'runuser %s\n' "$*" >>${shellDoubleQuoted(logPath)}
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
    [
      'ENV_FILE="/etc/qintopia/message-sidecar.env"',
      `ENV_FILE=${shellDoubleQuoted(sidecarEnv)}`,
    ],
    [
      'HERMES_ENV_FILE="/home/ubuntu/.hermes/profiles/xiaoman/.env"',
      `HERMES_ENV_FILE=${shellDoubleQuoted(hermesEnv)}`,
    ],
    [
      'HERMES_PLUGIN_PATH="/home/ubuntu/.hermes/profiles/xiaoman/plugins/qintopia-tools"',
      `HERMES_PLUGIN_PATH=${shellDoubleQuoted(profilePlugin)}`,
    ],
    [
      'RELEASE_PLUGIN_PATH="/home/ubuntu/qintopia-agent-os-releases/current/skills/qintopia-tools/variants/xiaoman"',
      `RELEASE_PLUGIN_PATH=${shellDoubleQuoted(releasePlugin)}`,
    ],
    ['RUNUSER_BIN="/usr/sbin/runuser"', `RUNUSER_BIN=${shellDoubleQuoted(runuser)}`],
    ['PYTHON_BIN="/usr/bin/python3"', `PYTHON_BIN=${shellDoubleQuoted(python)}`],
  ]);
  const copyFixture = (source, target) => {
    let content = fs.readFileSync(source, "utf8");
    for (const forbidden of [
      "TEST_MODE",
      "_TEST_MODE",
      "SYSTEMCTL:-",
      "RUNUSER_BIN:-",
    ]) {
      if (content.includes(forbidden)) {
        throw new Error(
          `${path.basename(source)} contains unsafe test override ${forbidden}`
        );
      }
    }
    for (const [from, to] of replacements) {
      if (content.includes(from)) {
        content = content.replace(from, to);
      }
    }
    if (content.includes('SYSTEMCTL="/usr/bin/systemctl"')) {
      throw new Error("fixture failed to replace fixed systemctl path");
    }
    writeExecutable(target, content);
  };
  copyFixture(sourceActivation, activation);
  copyFixture(sourceRollback, rollback);

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
  const enabledSidecar = {
    QINTOPIA_XIAOMAN_FEISHU_POSTER_ENABLED: "1",
    QINTOPIA_XIAOMAN_FEISHU_CALLBACK_ENCRYPT_KEY: "fixture-callback-key",
  };
  const enabledHermes = {
    QINTOPIA_XIAOMAN_POSTER_REVIEW_HOOK_ENABLE: "1",
    QINTOPIA_XIAOMAN_FEISHU_CALLBACK_ENCRYPT_KEY: "fixture-callback-key",
  };

  writeEnv(sidecarEnv, enabledSidecar);
  writeEnv(hermesEnv, enabledHermes);
  let result = run(activation);
  if (result.status === 0 || commandLog() !== "") {
    throw new Error("activation must fail before side effects without owner approval");
  }

  resetLog();
  const mismatchedPlugin = path.join(tmpRoot, "mismatched-release-plugin");
  fs.mkdirSync(mismatchedPlugin, { recursive: true });
  fs.unlinkSync(profilePlugin);
  fs.symlinkSync(mismatchedPlugin, profilePlugin);
  result = run(activation, {
    QINTOPIA_XIAOMAN_FEISHU_POSTER_PRODUCTION_ACTIVATION:
      "approved-production-xiaoman-feishu-poster-return",
  });
  if (result.status === 0 || commandLog() !== "") {
    throw new Error(
      "mismatched plugin link must fail before systemd or gateway side effects"
    );
  }
  fs.unlinkSync(profilePlugin);
  fs.symlinkSync(releasePlugin, profilePlugin);

  resetLog();
  writeEnv(hermesEnv, {
    ...enabledHermes,
    QINTOPIA_XIAOMAN_FEISHU_CALLBACK_ENCRYPT_KEY: "wrong-key",
  });
  result = run(activation, {
    QINTOPIA_XIAOMAN_FEISHU_POSTER_PRODUCTION_ACTIVATION:
      "approved-production-xiaoman-feishu-poster-return",
  });
  if (
    result.status === 0 ||
    commandLog() !== "" ||
    `${result.stdout}\n${result.stderr}`.includes("fixture-callback-key")
  ) {
    throw new Error(
      "mismatched callback keys must fail before side effects or disclosure"
    );
  }

  writeEnv(hermesEnv, enabledHermes);
  resetLog();
  fs.writeFileSync(restartFailureMarker, "fail\n", "utf8");
  result = run(activation, {
    QINTOPIA_XIAOMAN_FEISHU_POSTER_PRODUCTION_ACTIVATION:
      "approved-production-xiaoman-feishu-poster-return",
  });
  const restartFailureLog = commandLog();
  if (
    result.status === 0 ||
    !restartFailureLog.includes(
      "systemctl start qintopia-agentos-xiaoman-feishu-poster-preflight.service"
    ) ||
    !restartFailureLog.includes(
      "systemctl --user restart hermes-gateway-xiaoman.service"
    ) ||
    restartFailureLog.includes("systemctl enable") ||
    restartFailureLog.includes(
      "systemctl restart qintopia-agentos-xiaoman-feishu-poster-delivery.timer"
    )
  ) {
    throw new Error(
      "failed Xiaoman restart must not enable or restart poster workflow units"
    );
  }
  fs.unlinkSync(restartFailureMarker);

  resetLog();
  result = run(activation, {
    QINTOPIA_XIAOMAN_FEISHU_POSTER_PRODUCTION_ACTIVATION:
      "approved-production-xiaoman-feishu-poster-return",
  });
  if (result.status !== 0) {
    throw new Error(`activation failed\n${result.stdout}\n${result.stderr}`);
  }
  const activationLog = commandLog();
  const expectedOrder = [
    "systemctl start qintopia-agentos-xiaoman-feishu-poster-preflight.service",
    "systemctl --user restart hermes-gateway-xiaoman.service",
    "systemctl --user is-active --quiet hermes-gateway-xiaoman.service",
    "systemctl enable --now qintopia-agentos-operations-intake.service",
    "systemctl enable --now qintopia-agentos-xiaoman-poster-review-callback.service",
    "systemctl enable --now qintopia-agentos-xiaoman-poster-notification-starter.timer",
    "systemctl enable qintopia-agentos-xiaoman-feishu-poster-delivery.timer",
  ];
  let priorIndex = -1;
  for (const command of expectedOrder) {
    const index = activationLog.indexOf(command);
    if (index <= priorIndex) {
      throw new Error(`activation command missing or out of order: ${command}`);
    }
    priorIndex = index;
  }
  if (activationLog.includes("must-not-reach-runuser")) {
    throw new Error("activation leaked ambient environment to runuser");
  }

  resetLog();
  result = run(rollback);
  if (result.status === 0 || commandLog() !== "") {
    throw new Error("rollback must fail before side effects without owner approval");
  }

  writeEnv(sidecarEnv, { QINTOPIA_XIAOMAN_FEISHU_POSTER_ENABLED: "0" });
  writeEnv(hermesEnv, { QINTOPIA_XIAOMAN_POSTER_REVIEW_HOOK_ENABLE: "0" });
  resetLog();
  result = run(rollback, {
    QINTOPIA_XIAOMAN_FEISHU_POSTER_PRODUCTION_ROLLBACK:
      "approved-production-xiaoman-feishu-poster-return-rollback",
  });
  if (result.status !== 0) {
    throw new Error(`rollback failed\n${result.stdout}\n${result.stderr}`);
  }
  const rollbackLog = commandLog();
  for (const command of [
    "systemctl disable --now qintopia-agentos-xiaoman-feishu-poster-delivery.timer",
    "systemctl disable --now qintopia-agentos-xiaoman-poster-review-callback.service",
    "systemctl --user restart hermes-gateway-xiaoman.service",
    "systemctl --user is-active --quiet hermes-gateway-xiaoman.service",
  ]) {
    if (!rollbackLog.includes(command)) {
      throw new Error(`rollback command missing: ${command}`);
    }
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Xiaoman Feishu poster production activation test passed.");
