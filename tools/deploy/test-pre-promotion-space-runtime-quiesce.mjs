#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "qintopia-space-quiesce-"));
const sourcePath = path.join(
  repoRoot,
  "deploy",
  "runner",
  "quiesce-space-automation-runtime.sh"
);
const fixtureScript = path.join(tmpRoot, "quiesce-space-automation-runtime.sh");
const fakeSystemctl = path.join(tmpRoot, "systemctl");
const units = {
  timer: "qintopia-agentos-automation-dispatcher.timer",
  dispatcher: "qintopia-agentos-automation-dispatcher.service",
  worker: "qintopia-agentos-space-automation-execution-worker.service",
};

const writeExecutable = (filePath, content) => {
  fs.writeFileSync(filePath, content, "utf8");
  fs.chmodSync(filePath, 0o755);
};

const initializeState = (scenario, stateDir) => {
  fs.mkdirSync(stateDir, { recursive: true });
  if (scenario === "zero-units") {
    return;
  }
  const loadedUnits =
    scenario === "partial-units" ? [units.timer] : Object.values(units);
  for (const unit of loadedUnits) {
    fs.writeFileSync(path.join(stateDir, `${unit}.loaded`), "\n", "utf8");
    fs.writeFileSync(path.join(stateDir, `${unit}.active`), "\n", "utf8");
  }
  for (const unit of [units.timer, units.worker]) {
    if (loadedUnits.includes(unit)) {
      fs.writeFileSync(path.join(stateDir, `${unit}.enabled`), "\n", "utf8");
    }
  }
};

const runScenario = (scenario) => {
  const scenarioRoot = path.join(tmpRoot, scenario);
  const stateDir = path.join(scenarioRoot, "state");
  const logPath = path.join(scenarioRoot, "systemctl.log");
  fs.mkdirSync(scenarioRoot, { recursive: true });
  initializeState(scenario, stateDir);
  const result = spawnSync("bash", [fixtureScript], {
    cwd: repoRoot,
    env: {
      ...process.env,
      FAKE_QUIESCE_SCENARIO: scenario,
      FAKE_QUIESCE_STATE_DIR: stateDir,
      FAKE_QUIESCE_SYSTEMCTL_LOG: logPath,
    },
    encoding: "utf8",
  });
  return {
    ...result,
    log: fs.existsSync(logPath) ? fs.readFileSync(logPath, "utf8") : "",
  };
};

const requireCommands = (scenario, log, commands) => {
  for (const command of commands) {
    if (!log.includes(command)) {
      throw new Error(`${scenario} did not attempt systemctl ${command}`);
    }
  }
};

try {
  const source = fs.readFileSync(sourcePath, "utf8");
  const fixedSystemctl = 'SYSTEMCTL="/usr/bin/systemctl"';
  if (source.split(fixedSystemctl).length !== 2) {
    throw new Error("quiesce script must contain one fixed systemctl assignment");
  }
  writeExecutable(
    fixtureScript,
    source.replace(fixedSystemctl, `SYSTEMCTL=${JSON.stringify(fakeSystemctl)}`)
  );
  writeExecutable(
    fakeSystemctl,
    `#!/usr/bin/env bash
set -euo pipefail

state_dir="${"${FAKE_QUIESCE_STATE_DIR:?}"}"
log_path="${"${FAKE_QUIESCE_SYSTEMCTL_LOG:?}"}"
scenario="${"${FAKE_QUIESCE_SCENARIO:?}"}"
printf '%s\\n' "$*" >>"$log_path"
unit="${"${@: -1}"}"
loaded_path="${"${state_dir}"}/${"${unit}"}.loaded"
active_path="${"${state_dir}"}/${"${unit}"}.active"
enabled_path="${"${state_dir}"}/${"${unit}"}.enabled"

case "${"${1:-}"}" in
  show)
    case "${"${2:-}"}" in
      --property=LoadState)
        if [[ -f "$loaded_path" ]]; then printf 'loaded\\n'; else printf 'not-found\\n'; fi
        ;;
      --property=ActiveState)
        if [[ -f "$active_path" ]]; then printf 'active\\n'; else printf 'inactive\\n'; fi
        ;;
      --property=UnitFileState)
        if [[ -f "$enabled_path" ]]; then printf 'enabled\\n'; else printf 'disabled\\n'; fi
        ;;
      *) exit 64 ;;
    esac
    ;;
  disable)
    if [[ "$scenario" == "disable-failure" && "$unit" == "${units.timer}" ]]; then
      exit 74
    fi
    rm -f "$enabled_path" "$active_path"
    ;;
  stop)
    if [[ "$scenario" == "stop-failure" && "$unit" == "${units.dispatcher}" ]]; then
      exit 75
    fi
    rm -f "$active_path"
    ;;
  reset-failed) ;;
  is-enabled) [[ -f "$enabled_path" ]] ;;
  is-active) [[ -f "$active_path" ]] ;;
  *) exit 64 ;;
esac
`
  );

  const zeroUnits = runScenario("zero-units");
  if (zeroUnits.status !== 0) {
    throw new Error(
      `zero-unit first install must pass, got ${zeroUnits.status}: ${zeroUnits.stderr}`
    );
  }
  if (zeroUnits.log.includes("disable ") || zeroUnits.log.includes("stop ")) {
    throw new Error("zero-unit first install must not mutate nonexistent units");
  }
  requireCommands("zero-units", zeroUnits.log, [
    `show --property=LoadState --value ${units.timer}`,
    `show --property=ActiveState --value ${units.worker}`,
  ]);

  const activeUnits = runScenario("three-active-units");
  if (activeUnits.status !== 0) {
    throw new Error(
      `three active units must quiesce, got ${activeUnits.status}: ${activeUnits.stderr}`
    );
  }
  requireCommands("three-active-units", activeUnits.log, [
    `disable --now ${units.timer}`,
    `disable --now ${units.worker}`,
    `stop ${units.dispatcher}`,
    `stop ${units.worker}`,
    `is-enabled --quiet ${units.timer}`,
    `is-active --quiet ${units.dispatcher}`,
    `show --property=UnitFileState --value ${units.worker}`,
    `show --property=ActiveState --value ${units.worker}`,
  ]);

  const partialUnits = runScenario("partial-units");
  if (partialUnits.status === 0) {
    throw new Error("a partially installed Space runtime must fail closed");
  }
  requireCommands("partial-units", partialUnits.log, [
    `disable --now ${units.timer}`,
    `disable --now ${units.worker}`,
    `stop ${units.dispatcher}`,
    `stop ${units.worker}`,
    `show --property=ActiveState --value ${units.worker}`,
  ]);

  for (const scenario of ["disable-failure", "stop-failure"]) {
    const failedMutation = runScenario(scenario);
    if (failedMutation.status === 0) {
      throw new Error(`${scenario} must fail closed`);
    }
    requireCommands(scenario, failedMutation.log, [
      `disable --now ${units.timer}`,
      `disable --now ${units.worker}`,
      `stop ${units.dispatcher}`,
      `stop ${units.worker}`,
      `reset-failed ${units.dispatcher}`,
      `reset-failed ${units.worker}`,
      `reset-failed ${units.timer}`,
      `show --property=ActiveState --value ${units.worker}`,
      `show --property=UnitFileState --value ${units.worker}`,
    ]);
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Pre-promotion Space runtime quiesce behavior test passed.");
