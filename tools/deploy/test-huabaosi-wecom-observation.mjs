#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "qintopia-wecom-observation-"));
const script = path.join(
  repoRoot,
  "deploy/sidecar/scripts/huabaosi-wecom-gateway-observation-smoke.sh"
);
const expectedDropInPath =
  "/home/ubuntu/.config/systemd/user/hermes-gateway-huabaosi.service.d/env.conf";
const expectedEnvironmentFile =
  "/home/ubuntu/.hermes/profiles/huabaosi/.env (ignore_errors=no)";

const writeExecutable = (filePath, content) => {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, content, "utf8");
  fs.chmodSync(filePath, 0o755);
};

try {
  const commandLog = path.join(tmpRoot, "commands.log");
  const profileDir = path.join(tmpRoot, "profiles", "huabaosi");
  const profileConfig = path.join(profileDir, "config.yaml");
  const releaseCurrent = path.join(tmpRoot, "qintopia-agent-os-releases", "current");
  const systemctl = path.join(tmpRoot, "bin", "systemctl");
  const journalctl = path.join(tmpRoot, "bin", "journalctl");

  fs.mkdirSync(profileDir, { recursive: true });
  fs.mkdirSync(releaseCurrent, { recursive: true });
  fs.writeFileSync(profileConfig, "busy_input_mode: interrupt\n", "utf8");

  writeExecutable(
    systemctl,
    `#!/usr/bin/env bash
set -euo pipefail
printf 'systemctl %s\\n' "$*" >>"${commandLog}"
if [[ "$1" != "--user" ]]; then
  exit 65
fi
shift
case "$1" in
  is-active)
    printf 'active\\n'
    ;;
  show)
    printf 'WorkingDirectory=%s\\n' "\${FAKE_WORKING_DIRECTORY-${profileDir}}"
    printf 'ExecStart=%s\\n' "\${FAKE_EXEC_START-{ path=/home/ubuntu/.hermes/hermes-agent/venv/bin/python ; argv[]=/home/ubuntu/.hermes/hermes-agent/venv/bin/python -m hermes_cli.main --profile huabaosi gateway run --replace ; ignore_errors=no ; start_time=[n/a] ; stop_time=[n/a] ; pid=0 ; code=(null) ; status=0/0 }}"
    printf 'DropInPaths=%s\\n' "\${FAKE_DROP_IN_PATHS-${expectedDropInPath}}"
    printf 'EnvironmentFiles=%s\\n' "\${FAKE_ENVIRONMENT_FILES-${expectedEnvironmentFile}}"
    ;;
  *)
    exit 64
    ;;
esac
`
  );

  writeExecutable(
    journalctl,
    `#!/usr/bin/env bash
set -euo pipefail
printf 'journalctl %s\\n' "$*" >>"${commandLog}"
if [[ "$1" != "--user" ]]; then
  exit 65
fi
shift
if [[ -n "\${FAKE_JOURNAL_LEAK:-}" ]]; then
  printf '%s\\n' "\${FAKE_JOURNAL_LEAK}"
  exit 0
fi
cat <<'JOURNAL'
[Wecom] filtered internal process output
[Wecom] Send failed: Timeout sending message to WeCom - trying plain-text fallback
[Wecom] Fallback send also failed: Timeout sending message to WeCom
API call failed against the configured custom model provider: Request timed out
private user sentence that must never appear in observation stdout
JOURNAL
`
  );

  const runObservation = (extraEnv = {}) =>
    spawnSync("bash", [script], {
      cwd: repoRoot,
      env: {
        ...process.env,
        QINTOPIA_HUABAOSI_WECOM_OBSERVATION_ENABLE: "1",
        QINTOPIA_HUABAOSI_WECOM_PROFILE_DIR: profileDir,
        QINTOPIA_HUABAOSI_WECOM_PROFILE_CONFIG: profileConfig,
        QINTOPIA_RELEASE_CURRENT_PATH: releaseCurrent,
        SYSTEMCTL: systemctl,
        JOURNALCTL: journalctl,
        ...extraEnv,
      },
      encoding: "utf8",
    });

  fs.writeFileSync(commandLog, "", "utf8");
  const ok = runObservation();
  if (ok.status !== 0) {
    throw new Error(
      `expected WeCom observation to pass\nstdout:\n${ok.stdout}\nstderr:\n${ok.stderr}`
    );
  }
  for (const fragment of [
    "Huabaosi WeCom gateway observation passed",
    "busy_input_mode=interrupt",
    "release_current_present=true",
    "internal_filter_count=1",
    "send_fallback_count=2",
    "api_timeout_count=3",
  ]) {
    if (!ok.stdout.includes(fragment)) {
      throw new Error(`observation stdout is missing ${fragment}`);
    }
  }
  if (ok.stdout.includes("private user sentence")) {
    throw new Error("observation leaked raw journal text");
  }

  const commands = fs.readFileSync(commandLog, "utf8");
  for (const required of [
    "systemctl --user is-active hermes-gateway-huabaosi.service",
    "systemctl --user show hermes-gateway-huabaosi.service",
    "journalctl --user -u hermes-gateway-huabaosi.service",
  ]) {
    if (!commands.includes(required)) {
      throw new Error(`observation did not query the user unit: ${required}`);
    }
  }
  for (const forbidden of ["restart", "reload", "start ", "enable ", "disable "]) {
    if (commands.includes(forbidden)) {
      throw new Error(`observation ran forbidden systemctl action: ${forbidden}`);
    }
  }

  fs.writeFileSync(profileConfig, "name: huabaosi\n", "utf8");
  const missingBusyMode = runObservation();
  if (missingBusyMode.status === 0) {
    throw new Error("expected missing busy_input_mode to fail observation");
  }

  fs.writeFileSync(profileConfig, "busy_input_mode: queue\n", "utf8");
  const leakedValue = "wecom-secret-must-not-appear";
  const secretEnvIgnored = runObservation({
    WECOM_SECRET: leakedValue,
  });
  if (secretEnvIgnored.status !== 0) {
    throw new Error("observation must not read configured WECOM_SECRET values");
  }
  if (`${secretEnvIgnored.stdout}\n${secretEnvIgnored.stderr}`.includes(leakedValue)) {
    throw new Error("observation repeated a configured secret value");
  }

  const leaked = runObservation({
    FAKE_JOURNAL_LEAK: "WECOM_SECRET",
  });
  if (leaked.status === 0) {
    throw new Error("expected fixed sensitive marker in journal to fail observation");
  }
  if (`${leaked.stdout}\n${leaked.stderr}`.includes("WECOM_SECRET")) {
    throw new Error("observation failure repeated the fixed sensitive marker");
  }

  const missingDropIn = runObservation({
    FAKE_DROP_IN_PATHS: "",
  });
  if (missingDropIn.status === 0) {
    throw new Error("expected a missing fixed environment drop-in to fail observation");
  }

  const extraDropIn = runObservation({
    FAKE_DROP_IN_PATHS: `${expectedDropInPath} /etc/systemd/user/hermes-gateway-huabaosi.service.d/override.conf`,
  });
  if (extraDropIn.status === 0) {
    throw new Error("expected an additional service drop-in to fail observation");
  }

  const wrongEnvironmentFile = runObservation({
    FAKE_ENVIRONMENT_FILES: "/tmp/huabaosi.env (ignore_errors=no)",
  });
  if (wrongEnvironmentFile.status === 0) {
    throw new Error("expected an alternate environment file to fail observation");
  }

  const optionalEnvironmentFile = runObservation({
    FAKE_ENVIRONMENT_FILES:
      "/home/ubuntu/.hermes/profiles/huabaosi/.env (ignore_errors=yes)",
  });
  if (optionalEnvironmentFile.status === 0) {
    throw new Error("expected an optional environment file to fail observation");
  }

  const workingDirectoryDrift = runObservation({
    FAKE_WORKING_DIRECTORY: "/tmp/huabaosi",
  });
  if (workingDirectoryDrift.status === 0) {
    throw new Error("expected working-directory drift to fail observation");
  }

  const commandDrift = runObservation({
    FAKE_EXEC_START: "{ path=/bin/false ; argv[]=/bin/false ; ignore_errors=no ; }",
  });
  if (commandDrift.status === 0) {
    throw new Error("expected gateway command drift to fail observation");
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Huabaosi WeCom observation test passed.");
