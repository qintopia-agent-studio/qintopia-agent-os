#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "qintopia-systemd-install-"));
const releaseSha = "0123456789abcdef0123456789abcdef01234567";

const writeExecutable = (filePath, content) => {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, content, "utf8");
  fs.chmodSync(filePath, 0o755);
};

try {
  const releaseRoot = path.join(tmpRoot, "releases");
  const releaseDir = path.join(releaseRoot, releaseSha);
  const scriptsDir = path.join(releaseDir, "deploy", "sidecar", "scripts");
  const unitDir = path.join(tmpRoot, "units");
  const systemctlLog = path.join(tmpRoot, "systemctl.log");
  const systemctl = path.join(tmpRoot, "bin", "systemctl");

  fs.mkdirSync(path.join(releaseDir, "sidecar"), { recursive: true });
  fs.mkdirSync(path.join(releaseDir, "runtime", "postgres", "migrations"), {
    recursive: true,
  });
  fs.mkdirSync(scriptsDir, { recursive: true });
  fs.mkdirSync(path.join(releaseDir, "deploy", "runner"), { recursive: true });
  fs.mkdirSync(path.join(releaseDir, "sidecar-profiles", "qiwe-production"), {
    recursive: true,
  });
  fs.copyFileSync(
    path.join(repoRoot, "deploy", "sidecar", "scripts", "render-systemd-units.sh"),
    path.join(scriptsDir, "render-systemd-units.sh")
  );
  fs.chmodSync(path.join(scriptsDir, "render-systemd-units.sh"), 0o755);
  writeExecutable(
    path.join(releaseDir, "sidecar", "qintopia-message-sidecar"),
    "#!/usr/bin/env bash\nexit 0\n"
  );
  writeExecutable(
    path.join(
      releaseDir,
      "sidecar-profiles",
      "qiwe-production",
      "qintopia-message-sidecar"
    ),
    "#!/usr/bin/env bash\nexit 0\n"
  );
  for (const unitName of [
    "qintopia-agent-os-deploy-runner.service",
    "qintopia-agent-os-deploy-runner.timer",
  ]) {
    fs.copyFileSync(
      path.join(repoRoot, "deploy", "runner", unitName),
      path.join(releaseDir, "deploy", "runner", unitName)
    );
  }
  fs.mkdirSync(releaseRoot, { recursive: true });
  fs.symlinkSync(releaseDir, path.join(releaseRoot, "current"));
  const resolvedReleaseDir = fs.realpathSync(releaseDir);

  writeExecutable(
    systemctl,
    `#!/usr/bin/env bash
set -euo pipefail
printf '%s\\n' "$*" >>"${systemctlLog}"
case "$1" in
  daemon-reload|enable|is-active) exit 0 ;;
  *) echo "unexpected systemctl command: $*" >&2; exit 64 ;;
esac
`
  );

  const result = spawnSync(
    "bash",
    [
      path.join(repoRoot, "deploy", "runner", "install-release-systemd-units.sh"),
      "--release-root",
      releaseRoot,
      "--release-sha",
      releaseSha,
    ],
    {
      cwd: repoRoot,
      env: {
        ...process.env,
        SYSTEMCTL: systemctl,
        QINTOPIA_SYSTEMD_UNIT_DIR: unitDir,
      },
      encoding: "utf8",
    }
  );

  if (result.status !== 0) {
    throw new Error(
      `expected systemd install to pass, got ${result.status}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`
    );
  }
  const sidecarUnit = fs.readFileSync(
    path.join(unitDir, "qintopia-message-sidecar.service"),
    "utf8"
  );
  const releaseExecPrefix = `/usr/bin/env QINTOPIA_DEPLOYED_COMMIT_SHA=${releaseSha}`;
  for (const required of [
    `WorkingDirectory=${resolvedReleaseDir}`,
    `ExecStart=${releaseExecPrefix} ${resolvedReleaseDir}/sidecar/qintopia-message-sidecar run`,
  ]) {
    if (!sidecarUnit.includes(required)) {
      throw new Error(`sidecar unit is missing ${required}`);
    }
  }
  if (sidecarUnit.includes("Environment=QINTOPIA_DEPLOYED_COMMIT_SHA=")) {
    throw new Error(
      "sidecar unit must bind deployed commit SHA after EnvironmentFile at the exec boundary"
    );
  }
  for (const unitName of [
    "qintopia-agentos-huabaosi-image-generation-preflight.service",
    "qintopia-agentos-huabaosi-image-generation-worker.service",
  ]) {
    const unit = fs.readFileSync(path.join(unitDir, unitName), "utf8");
    for (const required of [
      `${releaseExecPrefix} QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_RELEASE_SHA=${releaseSha} QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA=${releaseSha}`,
    ]) {
      if (!unit.includes(required)) {
        throw new Error(`${unitName} is missing ${required}`);
      }
    }
    for (const forbidden of [
      "Environment=QINTOPIA_DEPLOYED_COMMIT_SHA=",
      "Environment=QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_RELEASE_SHA=",
      "Environment=QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA=",
    ]) {
      if (unit.includes(forbidden)) {
        throw new Error(`${unitName} must not use vulnerable ${forbidden}`);
      }
    }
  }
  for (const unitName of [
    "qintopia-agentos-huabaosi-feishu-artifact-mirror-preflight.service",
    "qintopia-agentos-huabaosi-feishu-artifact-mirror-worker.service",
  ]) {
    const unit = fs.readFileSync(path.join(unitDir, unitName), "utf8");
    for (const required of [
      `${releaseExecPrefix} QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA=${releaseSha}`,
    ]) {
      if (!unit.includes(required)) {
        throw new Error(`${unitName} is missing ${required}`);
      }
    }
    for (const forbidden of [
      "Environment=QINTOPIA_DEPLOYED_COMMIT_SHA=",
      "Environment=QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA=",
    ]) {
      if (unit.includes(forbidden)) {
        throw new Error(`${unitName} must not use vulnerable ${forbidden}`);
      }
    }
    if (unit.includes("QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_RELEASE_SHA")) {
      throw new Error(`${unitName} must not inherit Huabaosi image release binding`);
    }
  }
  for (const unitName of [
    "qintopia-agentos-qiwe-image-send-preflight.service",
    "qintopia-agentos-qiwe-image-send-worker.service",
  ]) {
    const unit = fs.readFileSync(path.join(unitDir, unitName), "utf8");
    const qiweBin = `${resolvedReleaseDir}/sidecar-profiles/qiwe-production/qintopia-message-sidecar`;
    const qiweExecPrefix = `${releaseExecPrefix} QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA=${releaseSha}`;
    const expectedCommand = unitName.includes("preflight")
      ? "qiwe-image-send-production-preflight"
      : "run-qiwe-image-send-worker --once --apply";
    if (!unit.includes(`ExecStart=${qiweExecPrefix} ${qiweBin} ${expectedCommand}`)) {
      throw new Error(`${unitName} must execute the QiWe companion binary`);
    }
    if (
      unitName.includes("worker") &&
      !unit.includes(
        `ExecStartPre=${qiweExecPrefix} ${qiweBin} qiwe-image-send-production-preflight`
      )
    ) {
      throw new Error(`${unitName} must preflight the release-bound QiWe companion`);
    }
    if (
      unit.includes(
        `ExecStart=${qiweExecPrefix} ${resolvedReleaseDir}/sidecar/qintopia-message-sidecar`
      )
    ) {
      throw new Error(`${unitName} must not execute the Huabaosi binary`);
    }
    if (unit.includes("QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_RELEASE_SHA")) {
      throw new Error(`${unitName} must not inherit Huabaosi image release binding`);
    }
    for (const forbidden of [
      "Environment=QINTOPIA_DEPLOYED_COMMIT_SHA=",
      "Environment=QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA=",
    ]) {
      if (unit.includes(forbidden)) {
        throw new Error(`${unitName} must not use vulnerable ${forbidden}`);
      }
    }
  }
  const runnerUnit = fs.readFileSync(
    path.join(unitDir, "qintopia-agent-os-deploy-runner.service"),
    "utf8"
  );
  for (const required of [
    "StateDirectory=qintopia-agent-os-deploy",
    "StateDirectoryMode=0700",
    "WorkingDirectory=/var/lib/qintopia-agent-os-deploy",
  ]) {
    if (!runnerUnit.includes(required)) {
      throw new Error(`deploy runner unit is missing ${required}`);
    }
  }
  if (!fs.existsSync(path.join(unitDir, "qintopia-agent-os-deploy-runner.timer"))) {
    throw new Error("deploy runner timer was not installed from the release");
  }
  const xiaomanPosterUnits = [
    "qintopia-agentos-operations-intake.service",
    "qintopia-agentos-xiaoman-poster-notification-starter.service",
    "qintopia-agentos-xiaoman-poster-notification-starter.timer",
    "qintopia-agentos-xiaoman-feishu-poster-preflight.service",
    "qintopia-agentos-xiaoman-feishu-poster-delivery.service",
    "qintopia-agentos-xiaoman-feishu-poster-delivery.timer",
    "qintopia-agentos-xiaoman-feishu-internal-group-poster-preflight.service",
    "qintopia-agentos-xiaoman-feishu-internal-group-poster-delivery.service",
    "qintopia-agentos-xiaoman-feishu-internal-group-poster-delivery.timer",
    "qintopia-agentos-xiaoman-poster-review-callback.service",
  ];
  for (const unitName of xiaomanPosterUnits) {
    if (!fs.existsSync(path.join(unitDir, unitName))) {
      throw new Error(`expected installed Xiaoman poster unit ${unitName}`);
    }
  }
  for (const [unitName, scope] of [
    ["qintopia-agentos-xiaoman-feishu-poster-preflight.service", "direct"],
    ["qintopia-agentos-xiaoman-feishu-poster-delivery.service", "direct"],
    [
      "qintopia-agentos-xiaoman-feishu-internal-group-poster-preflight.service",
      "group",
    ],
    ["qintopia-agentos-xiaoman-feishu-internal-group-poster-delivery.service", "group"],
  ]) {
    const unit = fs.readFileSync(path.join(unitDir, unitName), "utf8");
    const expectedCommand = unitName.includes("preflight")
      ? `xiaoman-feishu-poster-preflight --conversation-scope ${scope}`
      : `run-xiaoman-feishu-poster-delivery --once --apply --conversation-scope ${scope}`;
    if (!unit.includes(expectedCommand)) {
      throw new Error(`${unitName} must pin the ${scope} delivery scope`);
    }
  }
  for (const timer of [
    "qintopia-agentos-xiaoman-activity-signal-worker.timer",
    "qintopia-agentos-xiaoman-activity-promotion-starter-worker.timer",
    "qintopia-agentos-xiaoman-activity-image-generation-starter-worker.timer",
    "qintopia-agentos-huabaosi-image-generation-worker.timer",
    "qintopia-agentos-xiaoman-activity-send-request-starter-worker.timer",
    "qintopia-agentos-operations-group-send-ready.timer",
    "qintopia-agentos-huabaosi-feishu-artifact-mirror-worker.timer",
    "qintopia-agentos-qiwe-image-send-worker.timer",
  ]) {
    if (!fs.existsSync(path.join(unitDir, timer))) {
      throw new Error(`expected rendered timer ${timer}`);
    }
  }
  for (const [timer, firstTrigger] of [
    ["qintopia-agentos-huabaosi-image-generation-worker.timer", "11min"],
    ["qintopia-agentos-huabaosi-feishu-artifact-mirror-worker.timer", "12min"],
    ["qintopia-agentos-qiwe-image-send-worker.timer", "13min"],
  ]) {
    const unit = fs.readFileSync(path.join(unitDir, timer), "utf8");
    if (!unit.includes(`OnActiveSec=${firstTrigger}`) || unit.includes("OnBootSec=")) {
      throw new Error(`${timer} must schedule its first run from manual activation`);
    }
  }
  const log = fs.readFileSync(systemctlLog, "utf8");
  for (const required of [
    "daemon-reload",
    "enable --now qintopia-agentos-xiaoman-activity-signal-worker.timer",
    "enable --now qintopia-agentos-xiaoman-activity-promotion-starter-worker.timer",
    "enable --now qintopia-agentos-xiaoman-activity-image-generation-starter-worker.timer",
    "enable --now qintopia-agentos-xiaoman-activity-send-request-starter-worker.timer",
    "enable --now qintopia-agentos-operations-group-send-ready.timer",
  ]) {
    if (!log.includes(required)) {
      throw new Error(`systemctl log is missing ${required}`);
    }
  }
  if (
    log.includes("enable --now qintopia-agentos-huabaosi-image-generation-worker.timer")
  ) {
    throw new Error(
      "release installer must not automatically enable Huabaosi generation"
    );
  }
  if (
    log.includes(
      "enable --now qintopia-agentos-huabaosi-feishu-artifact-mirror-worker.timer"
    )
  ) {
    throw new Error(
      "release installer must not automatically enable Huabaosi Feishu mirroring"
    );
  }
  if (log.includes("enable --now qintopia-agentos-qiwe-image-send-worker.timer")) {
    throw new Error("release installer must not automatically enable QiWe image send");
  }
  for (const unitName of xiaomanPosterUnits) {
    const enabled = log
      .split("\n")
      .some((line) => line.startsWith("enable ") && line.endsWith(unitName));
    if (enabled) {
      throw new Error(
        `release installer must not automatically enable Xiaoman poster unit ${unitName}`
      );
    }
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Release systemd install test passed.");
