#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpRoot = fs.mkdtempSync(
  path.join(os.tmpdir(), "qintopia-erhua-brief-activation-")
);
const sourceActivation = path.join(
  repoRoot,
  "deploy/sidecar/scripts/activate-erhua-morning-brief-production.sh"
);
const sourceRollback = path.join(
  repoRoot,
  "deploy/sidecar/scripts/rollback-erhua-morning-brief-production.sh"
);
const sourceObservation = path.join(
  repoRoot,
  "deploy/sidecar/scripts/erhua-morning-brief-timer-observation-smoke.sh"
);
const sourceConfig = path.join(
  repoRoot,
  "deploy/sidecar/scripts/apply-erhua-morning-brief-production-config.sh"
);
const sourceErhuaCron = path.join(
  repoRoot,
  "deploy/sidecar/scripts/erhua-legacy-cron-observation-smoke.sh"
);
const sourceXiaomanCron = path.join(
  repoRoot,
  "deploy/sidecar/scripts/xiaoman-legacy-cron-observation-smoke.sh"
);

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
const replaceAll = (content, replacements) => {
  for (const [from, to] of replacements) {
    content = content.replaceAll(from, to);
  }
  return content;
};

try {
  const python = spawnSync("which", ["python3"], { encoding: "utf8" }).stdout.trim();
  if (!python) {
    throw new Error("python3 is required for the activation fixture");
  }

  const scriptsDir = path.join(tmpRoot, "scripts");
  const activation = path.join(
    scriptsDir,
    "activate-erhua-morning-brief-production.sh"
  );
  const rollback = path.join(scriptsDir, "rollback-erhua-morning-brief-production.sh");
  const observation = path.join(
    scriptsDir,
    "erhua-morning-brief-timer-observation-smoke.sh"
  );
  const config = path.join(
    scriptsDir,
    "apply-erhua-morning-brief-production-config.sh"
  );
  const erhuaCron = path.join(scriptsDir, "erhua-legacy-cron-observation-smoke.sh");
  const xiaomanCron = path.join(scriptsDir, "xiaoman-legacy-cron-observation-smoke.sh");
  const systemctl = path.join(tmpRoot, "systemctl");
  const journalctl = path.join(tmpRoot, "journalctl");
  const logPath = path.join(tmpRoot, "commands.log");
  const statePath = path.join(tmpRoot, "timer-state");
  const activePath = path.join(tmpRoot, "timer-active");
  const sidecarEnv = path.join(tmpRoot, "message-sidecar.env");
  const releaseSha = "0123456789abcdef0123456789abcdef01234567";
  const commitSha = "1".repeat(40);
  const runtimeSha = "2".repeat(40);
  const deployBundleSha = "3".repeat(40);
  const releaseRoot = path.join(tmpRoot, "releases");
  const releaseDir = path.join(releaseRoot, releaseSha);
  const releaseCurrent = path.join(releaseRoot, "current");
  const fakeHermesVenv = path.join(tmpRoot, "home", ".hermes", "hermes-agent", "venv");
  const fakeHermesPython = path.join(fakeHermesVenv, "bin", "python");
  const erhuaProfile = path.join(tmpRoot, "profiles", "erhua");
  const xiaomanProfile = path.join(tmpRoot, "profiles", "xiaoman");

  fs.mkdirSync(path.dirname(fakeHermesPython), { recursive: true });
  writeExecutable(fakeHermesPython, "#!/usr/bin/env bash\nexit 0\n");
  fs.mkdirSync(path.join(releaseDir, "runtime", "hermes"), { recursive: true });
  fs.writeFileSync(
    path.join(releaseDir, "runtime", "hermes", "validate_hermes_python.py"),
    "import sys\nsys.exit(0)\n",
    "utf8"
  );
  const registryFixture = path.join(
    releaseDir,
    "runtime",
    "hermes",
    "cron",
    "reviewed-cron-jobs.json"
  );
  fs.mkdirSync(path.dirname(registryFixture), { recursive: true });
  fs.writeFileSync(
    registryFixture,
    JSON.stringify({ schema_version: 1, reviewed_jobs: [] }, null, 2),
    "utf8"
  );
  fs.mkdirSync(releaseRoot, { recursive: true });
  fs.writeFileSync(
    path.join(releaseDir, "manifest.json"),
    `${JSON.stringify(
      {
        release_sha: releaseSha,
        commit_sha: commitSha,
        runtime_sha: runtimeSha,
        deploy_bundle_sha: deployBundleSha,
      },
      null,
      2
    )}\n`,
    "utf8"
  );
  fs.symlinkSync(releaseDir, releaseCurrent);
  fs.mkdirSync(erhuaProfile, { recursive: true });
  fs.mkdirSync(xiaomanProfile, { recursive: true });

  const fixedReplacements = [
    ["/usr/bin/systemctl", systemctl],
    ["/usr/bin/journalctl", journalctl],
    ["/usr/bin/python3", python],
    ["/etc/qintopia/message-sidecar.env", sidecarEnv],
    ["/home/ubuntu/qintopia-agent-os-releases/current", releaseCurrent],
    ["/home/ubuntu/.hermes/hermes-agent/venv/bin/python", fakeHermesPython],
    ["/home/ubuntu/.hermes/hermes-agent/venv", fakeHermesVenv],
  ];
  for (const sourcePath of [
    sourceActivation,
    sourceRollback,
    sourceObservation,
    sourceConfig,
    sourceErhuaCron,
    sourceXiaomanCron,
  ]) {
    const source = fs.readFileSync(sourcePath, "utf8");
    if (source.includes('SYSTEMCTL="${SYSTEMCTL:-systemctl}"')) {
      throw new Error(`${path.basename(sourcePath)} must not allow systemctl override`);
    }
    if (source.includes("QINTOPIA_SIDECAR_ENV_FILE")) {
      throw new Error(`${path.basename(sourcePath)} must not allow env-file override`);
    }
  }
  writeExecutable(
    activation,
    replaceAll(fs.readFileSync(sourceActivation, "utf8"), fixedReplacements)
  );
  writeExecutable(
    rollback,
    replaceAll(fs.readFileSync(sourceRollback, "utf8"), fixedReplacements)
  );
  writeExecutable(
    observation,
    replaceAll(fs.readFileSync(sourceObservation, "utf8"), fixedReplacements)
  );
  writeExecutable(
    config,
    replaceAll(fs.readFileSync(sourceConfig, "utf8"), fixedReplacements)
  );
  writeExecutable(
    erhuaCron,
    replaceAll(fs.readFileSync(sourceErhuaCron, "utf8"), [
      ["/home/ubuntu/.hermes/profiles/erhua", erhuaProfile],
      [
        "/home/ubuntu/qintopia-agent-os-releases/current/runtime/hermes/cron/reviewed-cron-jobs.json",
        registryFixture,
      ],
    ])
  );
  writeExecutable(
    xiaomanCron,
    replaceAll(fs.readFileSync(sourceXiaomanCron, "utf8"), [
      ["/home/ubuntu/.hermes/profiles/xiaoman", xiaomanProfile],
      [
        "/home/ubuntu/qintopia-agent-os-releases/current/runtime/hermes/cron/reviewed-cron-jobs.json",
        registryFixture,
      ],
    ])
  );

  writeExecutable(
    systemctl,
    `#!/usr/bin/env bash
set -euo pipefail
printf '%s\\n' "$*" >>${shellDoubleQuoted(logPath)}
fake_hermes_python=${shellDoubleQuoted(fakeHermesPython)}
fake_env_file=${shellDoubleQuoted(sidecarEnv)}
timer="qintopia-agentos-erhua-morning-brief.timer"
service="qintopia-agentos-erhua-morning-brief.service"
case "$1" in
  daemon-reload) exit 0 ;;
  enable)
    if [[ "$2" != "$timer" ]]; then exit 64; fi
    echo enabled >${shellDoubleQuoted(statePath)}
    exit 0
    ;;
  restart)
    if [[ "$2" != "$timer" ]]; then exit 64; fi
    echo enabled >${shellDoubleQuoted(statePath)}
    echo active >${shellDoubleQuoted(activePath)}
    exit 0
    ;;
  disable)
    if [[ "$2" != "--now" || "$3" != "$timer" ]]; then exit 64; fi
    echo disabled >${shellDoubleQuoted(statePath)}
    echo inactive >${shellDoubleQuoted(activePath)}
    exit 0
    ;;
  stop)
    if [[ "$2" != "$service" ]]; then exit 64; fi
    exit 0
    ;;
  reset-failed)
    if [[ "$2" != "$service" ]]; then exit 64; fi
    exit 0
    ;;
  is-enabled)
    state="$(cat ${shellDoubleQuoted(statePath)} 2>/dev/null || echo disabled)"
    if [[ "\${2:-}" == "--quiet" ]]; then
      [[ "$state" == "enabled" ]]
      exit $?
    fi
    echo "$state"
    [[ "$state" == "enabled" ]]
    exit $?
    ;;
  is-active)
    active="$(cat ${shellDoubleQuoted(activePath)} 2>/dev/null || echo inactive)"
    if [[ "\${2:-}" == "--quiet" ]]; then
      [[ "$active" == "active" ]]
      exit $?
    fi
    echo "$active"
    [[ "$active" == "active" ]]
    exit $?
    ;;
  show)
    printf '%s\\n' "\${FAKE_TIMER_NEXT_ELAPSE:-Sun 2026-08-09 08:10:00 CST}"
    exit 0
    ;;
  cat)
    if [[ "$2" == "$service" ]]; then
      cat <<'UNIT'
[Service]
WorkingDirectory=/home/ubuntu/qintopia-agent-os-releases/0123456789abcdef0123456789abcdef01234567
UNIT
      printf 'EnvironmentFile=%s\\n' "$fake_env_file"
      printf 'ExecStart=/usr/bin/env QINTOPIA_DEPLOYED_COMMIT_SHA=0123456789abcdef0123456789abcdef01234567 QINTOPIA_ERHUA_MORNING_BRIEF_PYTHON=%s /home/ubuntu/qintopia-agent-os-releases/0123456789abcdef0123456789abcdef01234567/deploy/sidecar/scripts/erhua-morning-brief-worker.sh\\n' "$fake_hermes_python"
      exit 0
    fi
    if [[ "$2" == "$timer" ]]; then
      cat <<'UNIT'
[Timer]
OnCalendar=*-*-* 08:10:00
Persistent=true
Unit=qintopia-agentos-erhua-morning-brief.service
UNIT
      exit 0
    fi
    exit 64
    ;;
  list-timers)
    echo "NEXT LEFT LAST PASSED UNIT ACTIVATES"
    echo "2026-08-09 08:10:00 1h - - qintopia-agentos-erhua-morning-brief.timer qintopia-agentos-erhua-morning-brief.service"
    exit 0
    ;;
  *) exit 64 ;;
esac
`
  );
  writeExecutable(
    journalctl,
    `#!/usr/bin/env bash
set -euo pipefail
printf 'journalctl %s\\n' "$*" >>${shellDoubleQuoted(logPath)}
printf '%s\\n' "no sensitive Erhua morning brief journal entries"
`
  );

  const runtimeEnv = {
    QINTOPIA_SIDECAR_DATABASE_URL:
      "postgres://fixture-user:fixture-password@127.0.0.1/qintopia",
  };
  const autoPublishLines = [
    "QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_PUBLISH_ENABLED=1",
    "QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_PUBLISH_APPROVAL=approved-production-erhua-morning-brief-auto-publish",
    "QINTOPIA_ERHUA_MORNING_BRIEF_TARGET_GROUP_ID=fixture-group-1",
    "QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_REVIEWER_ID=fixture-reviewer",
    "QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_CONFIRMER_ID=fixture-confirmer",
    "QINTOPIA_QIWE_TEXT_SEND_ENABLED=1",
    "QINTOPIA_QIWE_TEXT_SEND_PRODUCTION_APPROVAL=approved-production-qiwe-text-send",
    `QINTOPIA_QIWE_TEXT_SEND_PRODUCTION_DATABASE_URL_SHA256=${"a".repeat(64)}`,
    "QIWE_API_URL=https://manager.qiweapi.com/qiwe/api/qw/doApi",
    "QIWE_TOKEN=fixture-token",
    "QIWE_GUID=fixture-guid",
    "QINTOPIA_QIWE_IMAGE_SEND_ALLOWED_HOSTS=manager.qiweapi.com",
    "QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS=fixture-group-1,fixture-group-2",
  ];
  const run = (script, extraEnv = {}, args = []) =>
    spawnSync("bash", [script, ...args], {
      cwd: repoRoot,
      env: {
        ...process.env,
        QINTOPIA_UNRELATED_RUNTIME_SECRET: "must-not-reach-observation",
        QINTOPIA_ERHUA_MORNING_BRIEF_COMMIT_SHA: commitSha,
        QINTOPIA_ERHUA_MORNING_BRIEF_RUNTIME_SHA: runtimeSha,
        QINTOPIA_ERHUA_MORNING_BRIEF_DEPLOY_BUNDLE_SHA: deployBundleSha,
        QINTOPIA_ERHUA_MORNING_BRIEF_RELEASE_SHA: releaseSha,
        ...extraEnv,
      },
      encoding: "utf8",
    });
  const resetLog = (state = "disabled", active = "inactive") => {
    fs.writeFileSync(logPath, "", "utf8");
    fs.writeFileSync(statePath, state, "utf8");
    fs.writeFileSync(activePath, active, "utf8");
  };
  const commandLog = () => fs.readFileSync(logPath, "utf8");

  writeEnv(sidecarEnv, runtimeEnv);
  resetLog();
  let result = run(activation);
  if (result.status === 0 || commandLog() !== "") {
    throw new Error("activation must fail before side effects without owner approval");
  }

  writeEnv(sidecarEnv, runtimeEnv);
  resetLog();
  result = run(activation, {
    QINTOPIA_ERHUA_MORNING_BRIEF_ACTIVATION: "approved-production-erhua-morning-brief",
  });
  if (result.status === 0 || commandLog() !== "") {
    throw new Error(
      "activation must fail before side effects without persistent config"
    );
  }

  writeEnv(sidecarEnv, runtimeEnv);
  result = run(config);
  if (result.status === 0) {
    throw new Error("config must require explicit owner approval");
  }
  result = run(config, {
    QINTOPIA_ERHUA_MORNING_BRIEF_PRODUCTION_CONFIG:
      "approved-production-erhua-morning-brief-config",
  });
  if (result.status === 0) {
    throw new Error("config must require an explicit mode");
  }
  result = run(
    config,
    {
      QINTOPIA_ERHUA_MORNING_BRIEF_PRODUCTION_CONFIG:
        "approved-production-erhua-morning-brief-config",
    },
    ["--enable"]
  );
  if (result.status !== 0) {
    throw new Error(`config failed\n${result.stdout}\n${result.stderr}`);
  }
  const configuredEnv = fs.readFileSync(sidecarEnv, "utf8");
  for (const line of [
    "QINTOPIA_ERHUA_MORNING_BRIEF_ENABLED=1",
    "QINTOPIA_ERHUA_MORNING_BRIEF_PRODUCTION_APPROVAL=approved-production-erhua-morning-brief",
    "QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE=1",
    "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE=1",
    "QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE=1",
    "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE=1",
  ]) {
    if (!configuredEnv.includes(line)) {
      throw new Error(`config did not write ${line}`);
    }
  }

  resetLog();
  result = run(activation, {
    QINTOPIA_ERHUA_MORNING_BRIEF_ACTIVATION: "approved-production-erhua-morning-brief",
  });
  if (result.status === 0 || commandLog() !== "") {
    throw new Error(
      "activation must fail before side effects without auto-publish config"
    );
  }
  fs.appendFileSync(sidecarEnv, `\n${autoPublishLines.join("\n")}\n`, "utf8");

  resetLog();
  result = run(activation, {
    QINTOPIA_ERHUA_MORNING_BRIEF_ACTIVATION: "approved-production-erhua-morning-brief",
  });
  if (result.status !== 0) {
    throw new Error(
      `activation failed\n${result.stdout}\n${result.stderr}\ncommands:\n${commandLog()}`
    );
  }
  const activationLog = commandLog();
  for (const command of [
    "daemon-reload",
    "enable qintopia-agentos-erhua-morning-brief.timer",
    "restart qintopia-agentos-erhua-morning-brief.timer",
    "is-enabled --quiet qintopia-agentos-erhua-morning-brief.timer",
    "is-active --quiet qintopia-agentos-erhua-morning-brief.timer",
    "show --property=NextElapseUSecRealtime --value qintopia-agentos-erhua-morning-brief.timer",
    "show --property=ActiveEnterTimestamp --value qintopia-agentos-erhua-morning-brief.timer",
    "journalctl -u qintopia-agentos-erhua-morning-brief.service --since Sun 2026-08-09 08:10:00 CST -n 80 --no-pager",
  ]) {
    if (!activationLog.includes(command)) {
      throw new Error(`activation is missing systemctl command: ${command}`);
    }
  }
  if (activationLog.includes(runtimeEnv.QINTOPIA_SIDECAR_DATABASE_URL)) {
    throw new Error("activation leaked database URL");
  }

  resetLog();
  result = run(activation, {
    QINTOPIA_ERHUA_MORNING_BRIEF_ACTIVATION: "approved-production-erhua-morning-brief",
    FAKE_TIMER_NEXT_ELAPSE: "infinity",
  });
  if (
    result.status === 0 ||
    !commandLog().includes("disable --now qintopia-agentos-erhua-morning-brief.timer")
  ) {
    throw new Error("activation accepted a timer without a future realtime trigger");
  }

  resetLog("enabled", "active");
  result = run(rollback);
  if (result.status === 0 || commandLog() !== "") {
    throw new Error("rollback must fail before side effects without owner approval");
  }

  resetLog("enabled", "active");
  result = run(rollback, {
    QINTOPIA_ERHUA_MORNING_BRIEF_ROLLBACK:
      "approved-production-erhua-morning-brief-rollback",
  });
  if (result.status === 0 || commandLog() !== "") {
    throw new Error(
      "rollback must fail before side effects until persistent config is disabled"
    );
  }

  result = run(
    config,
    {
      QINTOPIA_ERHUA_MORNING_BRIEF_PRODUCTION_CONFIG:
        "approved-production-erhua-morning-brief-config",
    },
    ["--disable"]
  );
  if (result.status !== 0) {
    throw new Error(`disable config failed\n${result.stdout}\n${result.stderr}`);
  }

  resetLog("enabled", "active");
  result = run(rollback, {
    QINTOPIA_ERHUA_MORNING_BRIEF_ROLLBACK:
      "approved-production-erhua-morning-brief-rollback",
  });
  if (result.status !== 0) {
    throw new Error(`rollback failed\n${result.stdout}\n${result.stderr}`);
  }
  const rollbackLog = commandLog();
  for (const command of [
    "disable --now qintopia-agentos-erhua-morning-brief.timer",
    "stop qintopia-agentos-erhua-morning-brief.service",
    "reset-failed qintopia-agentos-erhua-morning-brief.service",
  ]) {
    if (!rollbackLog.includes(command)) {
      throw new Error(`rollback is missing systemctl command: ${command}`);
    }
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Erhua morning brief production activation test passed.");
