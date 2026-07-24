#!/usr/bin/env node

import fs from "node:fs";
import crypto from "node:crypto";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "qintopia-qiwe-activation-"));
const activationScript = path.join(
  repoRoot,
  "deploy/sidecar/scripts/activate-qiwe-image-send-production.sh"
);
const rollbackScript = path.join(
  repoRoot,
  "deploy/sidecar/scripts/rollback-qiwe-image-send-production.sh"
);
const fixedEnvFile = "/etc/qintopia/message-sidecar.env";
const fixedSystemctl = "/usr/bin/systemctl";
const fixedSha256sum = "/usr/bin/sha256sum";

try {
  const databaseUrl =
    "postgres://fixture-user:fixture-password@127.0.0.1:55432/qintopia";
  const databaseHash = crypto.createHash("sha256").update(databaseUrl).digest("hex");
  const logPath = path.join(tmpRoot, "systemctl.log");
  const envFile = path.join(tmpRoot, "message-sidecar.env");
  const systemctl = path.join(tmpRoot, "systemctl");
  const sha256sum = path.join(tmpRoot, "sha256sum");
  const activationFixture = path.join(tmpRoot, "activate-production-fixture.sh");
  const rollbackFixture = path.join(tmpRoot, "rollback-production-fixture.sh");
  const observationFixture = path.join(
    tmpRoot,
    "qiwe-image-send-production-observation-smoke.sh"
  );

  for (const sourcePath of [activationScript, rollbackScript]) {
    const source = fs.readFileSync(sourcePath, "utf8");
    if (source.includes("QINTOPIA_SIDECAR_ENV_FILE")) {
      throw new Error(
        `${path.basename(sourcePath)} must not allow overriding the reviewed env file`
      );
    }
    if (source.includes('SYSTEMCTL="${SYSTEMCTL:-systemctl}"')) {
      throw new Error(
        `${path.basename(sourcePath)} must not allow overriding systemctl`
      );
    }
    if (!source.includes(`ENV_FILE="${fixedEnvFile}"`)) {
      throw new Error(
        `${path.basename(sourcePath)} must read the fixed reviewed env file`
      );
    }
    if (!source.includes('PATH="/usr/bin:/bin:/usr/sbin:/sbin"')) {
      throw new Error(`${path.basename(sourcePath)} must reset PATH`);
    }
    if (!source.includes(`SYSTEMCTL="${fixedSystemctl}"`)) {
      throw new Error(`${path.basename(sourcePath)} must use the fixed systemctl path`);
    }
  }
  if (
    !fs.readFileSync(activationScript, "utf8").includes(`SHA256SUM="${fixedSha256sum}"`)
  ) {
    throw new Error("activation must use the fixed sha256sum path");
  }
  if (
    !fs
      .readFileSync(activationScript, "utf8")
      .includes("QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_OBSERVATION_ENABLE=1")
  ) {
    throw new Error("activation must run release-local QiWe production observation");
  }

  fs.writeFileSync(
    activationFixture,
    fs
      .readFileSync(activationScript, "utf8")
      .replaceAll(fixedEnvFile, envFile)
      .replaceAll(fixedSystemctl, systemctl)
      .replaceAll(fixedSha256sum, sha256sum),
    "utf8"
  );
  fs.chmodSync(activationFixture, 0o755);
  fs.writeFileSync(
    rollbackFixture,
    fs
      .readFileSync(rollbackScript, "utf8")
      .replaceAll(fixedEnvFile, envFile)
      .replaceAll(fixedSystemctl, systemctl),
    "utf8"
  );
  fs.chmodSync(rollbackFixture, 0o755);
  fs.writeFileSync(
    systemctl,
    `#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"${logPath}"
if [[ "\${SYSTEMCTL_FAIL_PREFLIGHT:-0}" == "1" && "$*" == "start qintopia-agentos-qiwe-image-send-preflight.service" ]]; then
  exit 23
fi
`,
    "utf8"
  );
  fs.chmodSync(systemctl, 0o755);
  fs.writeFileSync(
    observationFixture,
    `#!/usr/bin/env bash
set -euo pipefail
if [[ -n "\${QINTOPIA_UNRELATED_RUNTIME_SECRET:-}" ]]; then
  echo "ambient secret reached observation" >&2
  exit 23
fi
if [[ "\${QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_OBSERVATION_ENABLE:-}" != "1" ]]; then
  echo "missing observation enable flag" >&2
  exit 24
fi
if [[ "\${QINTOPIA_QIWE_IMAGE_SEND_EXPECTED_STATE:-}" != "enabled" ]]; then
  echo "unexpected observation state" >&2
  exit 25
fi
printf '%s\\n' "observation enabled" >>"${logPath}"
`,
    "utf8"
  );
  fs.chmodSync(observationFixture, 0o755);
  fs.writeFileSync(
    sha256sum,
    `#!/usr/bin/env bash
set -euo pipefail
input="$(cat)"
if [[ "$input" != "${databaseUrl}" ]]; then
  exit 2
fi
printf '%s  -\\n' "${databaseHash}"
`,
    "utf8"
  );
  fs.chmodSync(sha256sum, 0o755);

  const writeEnv = (flag) =>
    fs.writeFileSync(
      envFile,
      [
        `QINTOPIA_QIWE_IMAGE_SEND_ENABLED=${flag}`,
        `QINTOPIA_SIDECAR_DATABASE_URL=${databaseUrl}`,
        "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_APPROVAL=approved-production-qiwe-image-send",
        `QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256=${databaseHash}`,
        "",
      ].join("\n"),
      "utf8"
    );
  const run = (script, extraEnv = {}) =>
    spawnSync("bash", [script], {
      cwd: repoRoot,
      env: {
        ...process.env,
        QINTOPIA_UNRELATED_RUNTIME_SECRET: "must-not-reach-observation",
        ...extraEnv,
      },
      encoding: "utf8",
    });

  writeEnv("1");
  for (const script of [activationFixture, rollbackFixture]) {
    fs.writeFileSync(logPath, "", "utf8");
    const denied = run(script);
    if (denied.status === 0 || fs.readFileSync(logPath, "utf8") !== "") {
      throw new Error(
        `${path.basename(script)} must fail before systemctl without approval`
      );
    }
  }

  writeEnv("0");
  fs.writeFileSync(logPath, "", "utf8");
  const wrongFlag = run(activationFixture, {
    QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_ACTIVATION:
      "approved-production-qiwe-image-send",
  });
  if (wrongFlag.status === 0 || fs.readFileSync(logPath, "utf8") !== "") {
    throw new Error("activation must fail before systemctl when env is invalid");
  }

  fs.writeFileSync(
    envFile,
    [
      "QINTOPIA_QIWE_IMAGE_SEND_ENABLED=1",
      `QINTOPIA_SIDECAR_DATABASE_URL=${databaseUrl}`,
      "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_APPROVAL=approved-production-qiwe-image-send",
      `QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256=${databaseHash}`,
      "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256=not-a-hash",
      "",
    ].join("\n"),
    "utf8"
  );
  fs.writeFileSync(logPath, "", "utf8");
  const duplicateDatabaseHash = run(activationFixture, {
    QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_ACTIVATION:
      "approved-production-qiwe-image-send",
  });
  if (duplicateDatabaseHash.status === 0 || fs.readFileSync(logPath, "utf8") !== "") {
    throw new Error(
      "activation must fail before systemctl when production database hash is duplicated"
    );
  }

  fs.writeFileSync(
    envFile,
    [
      "QINTOPIA_QIWE_IMAGE_SEND_ENABLED=1",
      `QINTOPIA_SIDECAR_DATABASE_URL=${databaseUrl}`,
      "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_APPROVAL=approved-production-qiwe-image-send",
      `QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256=${"b".repeat(64)}`,
      "",
    ].join("\n"),
    "utf8"
  );
  fs.writeFileSync(logPath, "", "utf8");
  const databaseHashMismatch = run(activationFixture, {
    QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_ACTIVATION:
      "approved-production-qiwe-image-send",
  });
  if (
    databaseHashMismatch.status === 0 ||
    fs.readFileSync(logPath, "utf8") !== "" ||
    !databaseHashMismatch.stderr.includes(
      "database URL hash does not match the approved production hash"
    ) ||
    `${databaseHashMismatch.stdout}\n${databaseHashMismatch.stderr}`.includes(
      databaseUrl
    )
  ) {
    throw new Error(
      "activation must fail closed without leaking the database URL when the database hash does not match"
    );
  }

  fs.writeFileSync(
    envFile,
    [
      "QINTOPIA_QIWE_IMAGE_SEND_ENABLED=1",
      `QINTOPIA_SIDECAR_DATABASE_URL=${databaseUrl}`,
      "QINTOPIA_SIDECAR_DATABASE_URL=postgres://duplicate-secret@127.0.0.1:55432/qintopia",
      "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_APPROVAL=approved-production-qiwe-image-send",
      `QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256=${databaseHash}`,
      "",
    ].join("\n"),
    "utf8"
  );
  fs.writeFileSync(logPath, "", "utf8");
  const duplicateDatabaseUrl = run(activationFixture, {
    QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_ACTIVATION:
      "approved-production-qiwe-image-send",
  });
  if (
    duplicateDatabaseUrl.status === 0 ||
    fs.readFileSync(logPath, "utf8") !== "" ||
    !duplicateDatabaseUrl.stderr.includes(
      "requires exactly one QINTOPIA_SIDECAR_DATABASE_URL"
    ) ||
    `${duplicateDatabaseUrl.stdout}\n${duplicateDatabaseUrl.stderr}`.includes(
      "duplicate-secret"
    )
  ) {
    throw new Error(
      "activation must fail before systemctl when the database URL env line is duplicated"
    );
  }

  writeEnv("1");
  fs.writeFileSync(logPath, "", "utf8");
  const preflightRejected = run(activationFixture, {
    QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_ACTIVATION:
      "approved-production-qiwe-image-send",
    SYSTEMCTL_FAIL_PREFLIGHT: "1",
  });
  const rejectedActivationLog = fs.readFileSync(logPath, "utf8");
  if (
    preflightRejected.status === 0 ||
    !rejectedActivationLog.includes(
      "start qintopia-agentos-qiwe-image-send-preflight.service"
    ) ||
    rejectedActivationLog.includes(
      "enable --now qintopia-agentos-qiwe-image-send-worker.timer"
    )
  ) {
    throw new Error(
      "activation must stop before enabling the timer when production preflight fails"
    );
  }

  fs.writeFileSync(logPath, "", "utf8");
  const activated = run(activationFixture, {
    QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_ACTIVATION:
      "approved-production-qiwe-image-send",
  });
  if (activated.status !== 0) {
    throw new Error(`activation failed\n${activated.stdout}\n${activated.stderr}`);
  }
  const activationLog = fs.readFileSync(logPath, "utf8");
  for (const command of [
    "start qintopia-agentos-qiwe-image-send-preflight.service",
    "enable --now qintopia-agentos-qiwe-image-send-worker.timer",
    "is-enabled --quiet qintopia-agentos-qiwe-image-send-worker.timer",
    "is-active --quiet qintopia-agentos-qiwe-image-send-worker.timer",
    "observation enabled",
  ]) {
    if (!activationLog.includes(command)) {
      throw new Error(`activation is missing systemctl command: ${command}`);
    }
  }

  writeEnv("0");
  fs.writeFileSync(logPath, "", "utf8");
  const rolledBack = run(rollbackFixture, {
    QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_ROLLBACK:
      "approved-production-qiwe-image-send-rollback",
  });
  if (rolledBack.status !== 0) {
    throw new Error(`rollback failed\n${rolledBack.stdout}\n${rolledBack.stderr}`);
  }
  const rollbackLog = fs.readFileSync(logPath, "utf8");
  for (const command of [
    "disable --now qintopia-agentos-qiwe-image-send-worker.timer",
    "stop qintopia-agentos-qiwe-image-send-worker.service",
    "reset-failed qintopia-agentos-qiwe-image-send-worker.service",
  ]) {
    if (!rollbackLog.includes(command)) {
      throw new Error(`rollback is missing systemctl command: ${command}`);
    }
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("QiWe image production activation test passed.");
