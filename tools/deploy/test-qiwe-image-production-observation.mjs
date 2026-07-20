#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "qintopia-qiwe-observation-"));
const script = path.join(
  repoRoot,
  "deploy/sidecar/scripts/qiwe-image-send-production-observation-smoke.sh"
);

const writeExecutable = (filePath, content) => {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, content, "utf8");
  fs.chmodSync(filePath, 0o755);
};

const envLine = (key, value) => `${key}=${value}`;

try {
  const fixtureSecrets = [
    "postgres://fixture-user:fixture-password@127.0.0.1:55432/qintopia",
    "fixture-qiwe-token-secret",
    "fixture-qiwe-guid-secret",
    "reviewed-group-secret",
    "media.example.secret",
    "fixture-unrelated-secret",
  ];
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
if [[ "\${FAKE_QIWE_UNIT_PRESENT:-0}" == "1" && "$1" == "cat" ]]; then exit 0; fi
if [[ "\${FAKE_QIWE_TIMER_ENABLED:-0}" == "1" && "$1" == "is-enabled" ]]; then exit 0; fi
if [[ "\${FAKE_QIWE_TIMER_ACTIVE:-0}" == "1" && "$1" == "is-active" ]]; then exit 0; fi
exit 1
`
  );
  writeExecutable(
    sidecar,
    `#!/usr/bin/env bash
set -euo pipefail
printf '%s\\n' "$*" >>"${sidecarLog}"
echo "fake sidecar must not run during production observation" >&2
exit 70
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

  const qiweConfigLines = [
    envLine("QINTOPIA_SIDECAR_DATABASE_URL", fixtureSecrets[0]),
    envLine("QIWE_API_URL", "https://manager.qiweapi.com/qiwe/api/qw/doApi"),
    envLine("QIWE_TOKEN", fixtureSecrets[1]),
    envLine("QIWE_GUID", fixtureSecrets[2]),
    envLine("QINTOPIA_QIWE_IMAGE_SEND_ALLOWED_HOSTS", "manager.qiweapi.com"),
    envLine("QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS", fixtureSecrets[4]),
    envLine("QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS", fixtureSecrets[3]),
  ];
  const disabledEnv = path.join(tmpRoot, "disabled.env");
  const enabledEnv = path.join(tmpRoot, "enabled.env");
  const secretEnv = path.join(tmpRoot, "secret.env");
  const maliciousEnv = path.join(tmpRoot, "malicious.env");
  fs.writeFileSync(
    disabledEnv,
    ["QINTOPIA_QIWE_IMAGE_SEND_ENABLED=0", ...qiweConfigLines, ""].join("\n"),
    "utf8"
  );
  fs.writeFileSync(
    enabledEnv,
    ["QINTOPIA_QIWE_IMAGE_SEND_ENABLED=1", ...qiweConfigLines, ""].join("\n"),
    "utf8"
  );
  fs.writeFileSync(
    secretEnv,
    [
      "QINTOPIA_QIWE_IMAGE_SEND_ENABLED=0",
      ...qiweConfigLines,
      envLine("QINTOPIA_HUABAOSI_FEISHU_BASE_TOKEN", fixtureSecrets[5]),
      envLine("FEISHU_APP_SECRET", fixtureSecrets[5]),
      "",
    ].join("\n"),
    "utf8"
  );
  fs.writeFileSync(
    maliciousEnv,
    [
      "QINTOPIA_QIWE_IMAGE_SEND_ENABLED=0",
      ...qiweConfigLines,
      envLine(
        "QINTOPIA_IGNORED_RUNTIME_VALUE",
        `$(touch ${commandSubstitutionMarker})`
      ),
      "",
    ].join("\n"),
    "utf8"
  );

  const run = (extraEnv = {}) =>
    spawnSync("bash", [script], {
      cwd: repoRoot,
      env: {
        ...process.env,
        QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_OBSERVATION_ENABLE: "1",
        QINTOPIA_RELEASE_CURRENT_DIR: currentRelease,
        QINTOPIA_SIDECAR_BIN: "",
        QINTOPIA_SIDECAR_ENV_FILE: disabledEnv,
        SYSTEMCTL: systemctl,
        ...extraEnv,
      },
      encoding: "utf8",
    });

  const missingRelease = run({
    QINTOPIA_RELEASE_CURRENT_DIR: path.join(tmpRoot, "missing-current"),
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

  fs.writeFileSync(sidecarLog, "", "utf8");
  const disabled = run();
  if (disabled.status !== 0) {
    throw new Error(
      `disabled observation failed\n${disabled.stdout}\n${disabled.stderr}`
    );
  }
  const disabledCommands = fs.readFileSync(sidecarLog, "utf8");
  if (disabledCommands !== "") {
    throw new Error("disabled observation must not run the sidecar child process");
  }
  if (
    !disabled.stdout.includes("qiwe_image_send_production_observation_state=disabled")
  ) {
    throw new Error("disabled observation did not report disabled state");
  }

  fs.writeFileSync(sidecarLog, "", "utf8");
  const secretIgnored = run({ QINTOPIA_SIDECAR_ENV_FILE: secretEnv });
  if (secretIgnored.status !== 0) {
    throw new Error(
      `secret-env observation should ignore unrelated secrets\n${secretIgnored.stdout}\n${secretIgnored.stderr}`
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
    [
      "QINTOPIA_QIWE_IMAGE_SEND_ENABLED=0",
      "QINTOPIA_QIWE_IMAGE_SEND_ENABLED=1",
      "",
    ].join("\n"),
    "utf8"
  );
  const duplicate = run({ QINTOPIA_SIDECAR_ENV_FILE: duplicateEnv });
  if (duplicate.status === 0) {
    throw new Error("observation accepted a duplicate allowlisted env key");
  }
  if (!duplicate.stderr.includes("production observation env is invalid")) {
    throw new Error("duplicate allowlisted env key did not fail at env parsing");
  }

  fs.writeFileSync(sidecarLog, "", "utf8");
  const enabled = run({
    QINTOPIA_SIDECAR_ENV_FILE: enabledEnv,
  });
  if (enabled.status === 0 || !enabled.stderr.includes("not approved")) {
    throw new Error(
      "enabled observation must fail before any production send boundary"
    );
  }

  const installedUnit = run({
    FAKE_QIWE_UNIT_PRESENT: "1",
  });
  if (installedUnit.status === 0) {
    throw new Error("observation accepted an installed production apply unit");
  }

  const disabledTimerEnabled = run({
    QINTOPIA_SIDECAR_ENV_FILE: disabledEnv,
    FAKE_QIWE_TIMER_ENABLED: "1",
  });
  if (disabledTimerEnabled.status === 0) {
    throw new Error("disabled observation accepted an enabled production timer");
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("QiWe image production observation test passed.");
