#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpRoot = fs.mkdtempSync(
  path.join(os.tmpdir(), "qintopia-wecom-canary-observation-")
);
const script = path.join(
  repoRoot,
  "deploy/sidecar/scripts/huabaosi-wecom-canary-observation-smoke.sh"
);

const writeExecutable = (filePath, content) => {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, content, "utf8");
  fs.chmodSync(filePath, 0o755);
};

try {
  const commandLog = path.join(tmpRoot, "commands.log");
  const systemctl = path.join(tmpRoot, "bin", "systemctl");
  const sidecar = path.join(tmpRoot, "bin", "qintopia-message-sidecar");

  writeExecutable(
    systemctl,
    `#!/usr/bin/env bash
set -euo pipefail
printf 'systemctl %s\\n' "$*" >>"${commandLog}"
case "$1" in
  cat | is-active | is-enabled)
    exit 1
    ;;
  *)
    exit 64
    ;;
esac
`
  );

  writeExecutable(
    sidecar,
    `#!/usr/bin/env bash
set -euo pipefail
printf 'sidecar %s\\n' "$*" >>"${commandLog}"
if [[ "$1" != "huabaosi-wecom-canary-preflight" ]]; then
  exit 64
fi
if [[ -n "\${FAKE_PREFLIGHT_LEAK:-}" ]]; then
  printf '%s\\n' "\${FAKE_PREFLIGHT_LEAK}"
  exit 0
fi
cat <<'JSON'
{
  "success": false,
  "worker": "huabaosi-wecom-canary-gateway",
  "action_status": "staging_adapter_not_compiled",
  "adapter_compiled": false,
  "canary_enabled": false,
  "approval_present": false,
  "config_valid": false,
  "allowed_bot_count": 0,
  "allowed_chat_count": 0,
  "allowed_user_count": 0,
  "missing_configuration": [
    "QINTOPIA_HUABAOSI_WECOM_CANARY_ENDPOINT",
    "QINTOPIA_HUABAOSI_WECOM_CANARY_TOKEN"
  ],
  "protocol": "huabaosi_wecom_canary_https_json_v1",
  "rollback_command": "unset QINTOPIA_HUABAOSI_WECOM_CANARY_ENABLED and keep hermes-gateway-huabaosi.service as the production route",
  "safe_for_chat": false,
  "limitations": [],
  "guardrails": []
}
JSON
`
  );

  const runObservation = (extraEnv = {}) =>
    spawnSync("bash", [script], {
      cwd: repoRoot,
      env: {
        ...process.env,
        QINTOPIA_HUABAOSI_WECOM_CANARY_OBSERVATION_ENABLE: "1",
        QINTOPIA_SIDECAR_BIN: sidecar,
        SYSTEMCTL: systemctl,
        ...extraEnv,
      },
      encoding: "utf8",
    });

  fs.writeFileSync(commandLog, "", "utf8");
  const ok = runObservation();
  if (ok.status !== 0) {
    throw new Error(
      `expected canary observation to pass\nstdout:\n${ok.stdout}\nstderr:\n${ok.stderr}`
    );
  }
  if (!ok.stdout.includes("Huabaosi WeCom canary observation passed")) {
    throw new Error("canary observation stdout is missing pass marker");
  }

  const commands = fs.readFileSync(commandLog, "utf8");
  for (const fragment of [
    "huabaosi-wecom-canary-preflight",
    "systemctl cat qintopia-agentos-huabaosi-wecom-canary-gateway.service",
    "systemctl is-active --quiet qintopia-agentos-huabaosi-wecom-canary-gateway.service",
  ]) {
    if (!commands.includes(fragment)) {
      throw new Error(`canary observation command log is missing ${fragment}`);
    }
  }
  for (const forbidden of [
    "huabaosi-wecom-canary-gateway --apply",
    "restart",
    "enable ",
  ]) {
    if (commands.includes(forbidden)) {
      throw new Error(`canary observation ran forbidden command: ${forbidden}`);
    }
  }

  const secretValue = "canary-secret-value-must-not-appear";
  const secretEnvIgnored = runObservation({
    QINTOPIA_HUABAOSI_WECOM_CANARY_TOKEN: secretValue,
  });
  if (secretEnvIgnored.status !== 0) {
    throw new Error(
      "canary observation should ignore secret env values in output checks"
    );
  }
  if (`${secretEnvIgnored.stdout}\n${secretEnvIgnored.stderr}`.includes(secretValue)) {
    throw new Error("canary observation repeated a configured secret value");
  }

  const splitAllowlistLeak = runObservation({
    QINTOPIA_HUABAOSI_WECOM_CANARY_ALLOWED_BOT_IDS: "bot-one,bot-two",
    FAKE_PREFLIGHT_LEAK: "bot-two",
  });
  if (splitAllowlistLeak.status === 0) {
    throw new Error("expected split allowlist member leak to fail observation");
  }
  if (
    `${splitAllowlistLeak.stdout}\n${splitAllowlistLeak.stderr}`.includes("bot-two")
  ) {
    throw new Error("canary observation failure repeated split allowlist member");
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Huabaosi WeCom canary observation test passed.");
