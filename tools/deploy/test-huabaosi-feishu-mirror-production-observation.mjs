#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpRoot = fs.mkdtempSync(
  path.join(os.tmpdir(), "qintopia-feishu-mirror-observation-")
);
const script = path.join(
  repoRoot,
  "deploy/sidecar/scripts/huabaosi-feishu-artifact-mirror-production-observation-smoke.sh"
);

const writeExecutable = (filePath, content) => {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, content, "utf8");
  fs.chmodSync(filePath, 0o755);
};

try {
  const systemctlLog = path.join(tmpRoot, "systemctl.log");
  const sidecarLog = path.join(tmpRoot, "sidecar.log");
  const commandSubstitutionMarker = path.join(tmpRoot, "command-substitution-ran");
  const systemctl = path.join(tmpRoot, "bin", "systemctl");
  const releaseSha = "0123456789abcdef0123456789abcdef01234567";
  const releaseRoot = path.join(tmpRoot, "releases");
  const releaseDir = path.join(releaseRoot, releaseSha);
  const currentRelease = path.join(releaseRoot, "current");
  const sidecar = path.join(releaseDir, "sidecar", "qintopia-message-sidecar");

  writeExecutable(
    systemctl,
    `#!/usr/bin/env bash
set -euo pipefail
printf '%s\\n' "$*" >>"${systemctlLog}"
if [[ "\${FAKE_MIRROR_UNIT_PRESENT:-0}" == "1" && ( "$1" == "cat" || "$1" == "start" ) ]]; then exit 0; fi
if [[ "\${FAKE_MIRROR_TIMER_ENABLED:-0}" == "1" && "$1" == "is-enabled" ]]; then exit 0; fi
if [[ "\${FAKE_MIRROR_TIMER_ACTIVE:-0}" == "1" && "$1" == "is-active" ]]; then exit 0; fi
exit 1
`
  );
  writeExecutable(
    sidecar,
    `#!/usr/bin/env bash
set -euo pipefail
printf '%s\\n' "$*" >>"${sidecarLog}"
if [[ "\${QINTOPIA_SIDECAR_DATABASE_URL:-}" == "postgres://qintopia:change-me@127.0.0.1:55432/qintopia_test" || "\${FAKE_MIRROR_LEAK:-}" == "configured-value-must-not-appear" ]]; then
  echo "non-observation env reached fake sidecar" >&2
  exit 70
fi
case "$1" in
  huabaosi-feishu-artifact-mirror-preflight)
    if [[ "\${QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED:-0}" == "1" ]]; then
      printf '%s\\n' '{"success":true,"worker":"huabaosi-feishu-artifact-mirror-worker","action_status":"adapter_config_ready","adapter_compiled":true,"mirror_enabled":true,"config_valid":true,"schema_version":"huabaosi-generated-image-v1","media_allowed_host_count":1,"missing_configuration":[],"external_calls_executed":false,"database_writes_executed":false,"sensitive_fields_redacted":true}'
      exit 0
    fi
    printf '%s\\n' '{"success":false,"worker":"huabaosi-feishu-artifact-mirror-worker","action_status":"mirror_disabled","adapter_compiled":true,"mirror_enabled":false,"config_valid":false,"schema_version":"huabaosi-generated-image-v1","media_allowed_host_count":0,"missing_configuration":["QINTOPIA_HUABAOSI_FEISHU_BASE_TOKEN"],"external_calls_executed":false,"database_writes_executed":false,"sensitive_fields_redacted":true}'
    exit 1
    ;;
  run-huabaosi-feishu-artifact-mirror-worker)
    [[ "$2" == "--once" && "$3" == "--dry-run" ]]
    printf '{"success":true,"dry_run":true,"apply_requested":false,"fixture_mode":false,"worker":"huabaosi-feishu-artifact-mirror-worker","action_status":"no_mirrorable_generated_images","artifact_id":null,"work_item_id":null,"workflow_root_id":null,"review_status":null,"schema_version":"huabaosi-generated-image-v1","external_calls_executed":false,"database_writes_executed":false,"sensitive_fields_redacted":true,"guardrails":[],"unexpected":"%s"}\\n' "\${FAKE_MIRROR_LEAK:-}"
    ;;
  *) exit 64 ;;
esac
`
  );
  fs.writeFileSync(
    path.join(releaseDir, "sidecar", "artifact-manifest.json"),
    `${JSON.stringify(
      {
        commit_sha: releaseSha,
        validation: {
          cargo_features: [
            "huabaosi-production-adapter",
            "huabaosi-feishu-mirror-adapter",
          ],
        },
      },
      null,
      2
    )}\n`,
    "utf8"
  );
  fs.symlinkSync(releaseDir, currentRelease);
  fs.writeFileSync(sidecarLog, "", "utf8");

  const enabledEnv = path.join(tmpRoot, "enabled.env");
  const secretEnv = path.join(tmpRoot, "secret.env");
  const maliciousEnv = path.join(tmpRoot, "malicious.env");
  fs.writeFileSync(enabledEnv, "QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED=1\n", "utf8");
  fs.writeFileSync(
    secretEnv,
    [
      "QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED=0",
      "QINTOPIA_SIDECAR_DATABASE_URL=postgres://qintopia:change-me@127.0.0.1:55432/qintopia_test",
      "FAKE_MIRROR_LEAK=configured-value-must-not-appear",
      "",
    ].join("\n"),
    "utf8"
  );
  fs.writeFileSync(
    maliciousEnv,
    [
      "QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED=0",
      `QINTOPIA_SIDECAR_DATABASE_URL=$(touch ${commandSubstitutionMarker})`,
      "",
    ].join("\n"),
    "utf8"
  );

  const run = (extraEnv = {}) =>
    spawnSync("bash", [script], {
      cwd: repoRoot,
      env: {
        ...process.env,
        QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_OBSERVATION_ENABLE: "1",
        QINTOPIA_RELEASE_CURRENT_DIR: currentRelease,
        QINTOPIA_SIDECAR_BIN: "",
        QINTOPIA_SIDECAR_ENV_FILE: path.join(tmpRoot, "missing.env"),
        SYSTEMCTL: systemctl,
        ...extraEnv,
      },
      encoding: "utf8",
    });

  const missingRelease = spawnSync("bash", [script], {
    cwd: repoRoot,
    env: {
      ...process.env,
      QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_OBSERVATION_ENABLE: "1",
      QINTOPIA_SIDECAR_BIN: "",
      QINTOPIA_RELEASE_CURRENT_DIR: path.join(tmpRoot, "missing-current"),
      QINTOPIA_SIDECAR_ENV_FILE: path.join(tmpRoot, "missing.env"),
      SYSTEMCTL: systemctl,
    },
    encoding: "utf8",
  });
  if (missingRelease.status === 0 || fs.readFileSync(sidecarLog, "utf8") !== "") {
    throw new Error("observation must fail before execution without release/current");
  }

  const mutableBinary = run({
    QINTOPIA_SIDECAR_BIN: path.join(tmpRoot, "bin", "qintopia-message-sidecar"),
  });
  if (mutableBinary.status === 0) {
    throw new Error("observation accepted a sidecar outside release/current");
  }

  const disabled = run();
  if (disabled.status !== 0) {
    throw new Error(
      `disabled observation failed\n${disabled.stdout}\n${disabled.stderr}`
    );
  }
  if (fs.existsSync(commandSubstitutionMarker)) {
    throw new Error("observation executed command substitution from env file");
  }

  fs.writeFileSync(sidecarLog, "", "utf8");
  const secretIgnored = run({
    QINTOPIA_SIDECAR_ENV_FILE: secretEnv,
  });
  if (secretIgnored.status !== 0) {
    throw new Error(
      `secret-env observation should ignore non-observation secrets\n${secretIgnored.stdout}\n${secretIgnored.stderr}`
    );
  }

  const malicious = run({
    QINTOPIA_SIDECAR_ENV_FILE: maliciousEnv,
  });
  if (malicious.status === 0) {
    throw new Error("observation accepted command substitution in the env file");
  }
  if (fs.existsSync(commandSubstitutionMarker)) {
    throw new Error("observation executed command substitution from malicious env");
  }

  const duplicateEnv = path.join(tmpRoot, "duplicate.env");
  fs.writeFileSync(
    duplicateEnv,
    "QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED=0\nQINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED=1\n",
    "utf8"
  );
  const duplicate = run({
    QINTOPIA_SIDECAR_ENV_FILE: duplicateEnv,
  });
  if (duplicate.status === 0) {
    throw new Error("observation accepted a duplicate allowlisted env key");
  }

  fs.writeFileSync(systemctlLog, "", "utf8");
  fs.writeFileSync(sidecarLog, "", "utf8");
  const enabled = run({
    QINTOPIA_SIDECAR_ENV_FILE: enabledEnv,
    FAKE_MIRROR_UNIT_PRESENT: "1",
    FAKE_MIRROR_TIMER_ENABLED: "1",
    FAKE_MIRROR_TIMER_ACTIVE: "1",
  });
  if (enabled.status !== 0) {
    throw new Error(`enabled observation failed\n${enabled.stdout}\n${enabled.stderr}`);
  }
  const sidecarCommands = fs.readFileSync(sidecarLog, "utf8");
  if (sidecarCommands !== "") {
    throw new Error(
      "enabled observation passed production secrets through a child process"
    );
  }
  const systemctlCommands = fs.readFileSync(systemctlLog, "utf8");
  for (const required of [
    "start qintopia-agentos-huabaosi-feishu-artifact-mirror-preflight.service",
    "start qintopia-agentos-huabaosi-feishu-artifact-mirror-observation.service",
  ]) {
    if (!systemctlCommands.includes(required)) {
      throw new Error(`enabled observation did not run ${required}`);
    }
  }
  if (systemctlCommands.includes("--apply")) {
    throw new Error("observation must not execute mirror apply");
  }

  const incomplete = run({
    QINTOPIA_SIDECAR_ENV_FILE: enabledEnv,
    FAKE_MIRROR_UNIT_PRESENT: "1",
    FAKE_MIRROR_TIMER_ENABLED: "1",
    FAKE_MIRROR_TIMER_ACTIVE: "0",
  });
  if (incomplete.status === 0) {
    throw new Error("observation accepted an inactive production timer");
  }

  const leakMarker = "configured-value-must-not-appear";
  const leaked = run({
    QINTOPIA_HUABAOSI_FEISHU_APP_SECRET: leakMarker,
    FAKE_MIRROR_LEAK: leakMarker,
  });
  if (leaked.status !== 0) {
    throw new Error(
      `observation leaked configured secret env to sidecar\n${leaked.stdout}\n${leaked.stderr}`
    );
  }
  if (`${leaked.stdout}\n${leaked.stderr}`.includes(leakMarker)) {
    throw new Error("observation repeated a configured secret in its diagnostic");
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Huabaosi Feishu mirror production observation test passed.");
