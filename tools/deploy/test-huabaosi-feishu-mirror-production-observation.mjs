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
const envLine = (key, value) => `${key}=${value}`;

try {
  const fixtureSecrets = [
    "postgres://fixture-user:fixture&password@127.0.0.1:55432/qintopia_observation_fixture",
    "bascn_observation_fixture;base_token",
    "tbl_observation_fixture\\artifact_table",
    "fixture-feishu-tenant-access-token",
    "fixture-feishu-app-secret",
  ];
  const forbiddenChildEnvKeys = [
    "QINTOPIA_SIDECAR_DATABASE_URL",
    "QINTOPIA_HUABAOSI_FEISHU_BASE_TOKEN",
    "QINTOPIA_HUABAOSI_FEISHU_ALLOWED_BASE_TOKENS",
    "QINTOPIA_HUABAOSI_FEISHU_ARTIFACT_TABLE_ID",
    "QINTOPIA_HUABAOSI_FEISHU_ALLOWED_ARTIFACT_TABLE_IDS",
    "QINTOPIA_HUABAOSI_FEISHU_PROFILE_ENV_PATH",
    "QINTOPIA_HUABAOSI_FEISHU_TENANT_ACCESS_TOKEN",
    "FEISHU_APP_SECRET",
    "LARK_APP_SECRET",
  ];
  const scriptText = fs.readFileSync(script, "utf8");
  const launcherStart = scriptText.indexOf("run_sidecar_with_observation_env()");
  const launcherEnd = scriptText.indexOf("tmp_dir=", launcherStart);
  const launcher = scriptText.slice(launcherStart, launcherEnd);
  for (const key of forbiddenChildEnvKeys) {
    if (launcher.includes(key)) {
      throw new Error(`observation child launcher includes forbidden key ${key}`);
    }
  }

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
if env | grep -F -e '${fixtureSecrets.join("' -e '")}' >/dev/null; then
  echo "secret env reached fake sidecar" >&2
  exit 70
fi
case "$1" in
  huabaosi-feishu-artifact-mirror-observation-preflight)
    if [[ "\${QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED:-0}" == "1" ]]; then
      printf '%s\\n' '{"success":true,"worker":"huabaosi-feishu-artifact-mirror-worker","action_status":"observation_enabled_boundary_ready","adapter_compiled":false,"mirror_enabled":true,"config_valid":false,"schema_version":"huabaosi-generated-image-v1","media_allowed_host_count":0,"missing_configuration":[],"external_calls_executed":false,"database_writes_executed":false,"sensitive_fields_redacted":true}'
      exit 0
    fi
    printf '%s\\n' '{"success":true,"worker":"huabaosi-feishu-artifact-mirror-worker","action_status":"observation_disabled_boundary_ready","adapter_compiled":false,"mirror_enabled":false,"config_valid":false,"schema_version":"huabaosi-generated-image-v1","media_allowed_host_count":0,"missing_configuration":[],"external_calls_executed":false,"database_writes_executed":false,"sensitive_fields_redacted":true}'
    exit 0
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
          cargo_features: ["huabaosi-production-adapter"],
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
      envLine("QINTOPIA_SIDECAR_DATABASE_URL", fixtureSecrets[0]),
      envLine(forbiddenChildEnvKeys[1], fixtureSecrets[1]),
      envLine(forbiddenChildEnvKeys[2], fixtureSecrets[1]),
      envLine(forbiddenChildEnvKeys[3], fixtureSecrets[2]),
      envLine(forbiddenChildEnvKeys[4], fixtureSecrets[2]),
      envLine(forbiddenChildEnvKeys[6], fixtureSecrets[3]),
      envLine(forbiddenChildEnvKeys[7], fixtureSecrets[4]),
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
  for (const secret of fixtureSecrets) {
    if (`${secretIgnored.stdout}\n${secretIgnored.stderr}`.includes(secret)) {
      throw new Error("observation repeated a fixture secret in its diagnostic");
    }
  }

  const ignoredCommandSubstitution = run({
    QINTOPIA_SIDECAR_ENV_FILE: maliciousEnv,
  });
  if (ignoredCommandSubstitution.status !== 0) {
    throw new Error(
      `observation rejected an ignored non-allowlisted value\n${ignoredCommandSubstitution.stdout}\n${ignoredCommandSubstitution.stderr}`
    );
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
  if (enabled.status === 0) {
    throw new Error("observation accepted enabled Feishu mirror production state");
  }
  const sidecarCommands = fs.readFileSync(sidecarLog, "utf8");
  if (sidecarCommands !== "") {
    throw new Error("enabled mirror observation must fail before sidecar execution");
  }
  if (
    sidecarCommands.includes("run-huabaosi-feishu-artifact-mirror-worker") ||
    sidecarCommands.includes("--apply")
  ) {
    throw new Error("observation must not execute mirror worker apply or dry-run");
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

  const redactionSentinel = ["feishu", "observation", "redaction", "sentinel"].join(
    "-"
  );
  const leaked = run({
    QINTOPIA_HUABAOSI_FEISHU_APP_SECRET: redactionSentinel,
    FAKE_MIRROR_LEAK: redactionSentinel,
  });
  if (leaked.status !== 0) {
    throw new Error(
      `observation leaked configured secret env to sidecar\n${leaked.stdout}\n${leaked.stderr}`
    );
  }
  if (`${leaked.stdout}\n${leaked.stderr}`.includes(redactionSentinel)) {
    throw new Error("observation repeated a configured secret in its diagnostic");
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Huabaosi Feishu mirror production observation test passed.");
