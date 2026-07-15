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
  fs.copyFileSync(
    path.join(repoRoot, "deploy", "sidecar", "scripts", "render-systemd-units.sh"),
    path.join(scriptsDir, "render-systemd-units.sh")
  );
  fs.chmodSync(path.join(scriptsDir, "render-systemd-units.sh"), 0o755);
  writeExecutable(
    path.join(releaseDir, "sidecar", "qintopia-message-sidecar"),
    "#!/usr/bin/env bash\nexit 0\n"
  );
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
  for (const required of [
    `WorkingDirectory=${resolvedReleaseDir}`,
    `Environment=QINTOPIA_DEPLOYED_COMMIT_SHA=${releaseSha}`,
    `ExecStart=${resolvedReleaseDir}/sidecar/qintopia-message-sidecar run`,
  ]) {
    if (!sidecarUnit.includes(required)) {
      throw new Error(`sidecar unit is missing ${required}`);
    }
  }
  for (const timer of [
    "qintopia-agentos-xiaoman-activity-signal-worker.timer",
    "qintopia-agentos-xiaoman-activity-promotion-starter-worker.timer",
    "qintopia-agentos-xiaoman-activity-image-generation-starter-worker.timer",
    "qintopia-agentos-huabaosi-image-generation-worker.timer",
    "qintopia-agentos-huabaosi-feishu-artifact-mirror-worker.timer",
    "qintopia-agentos-xiaoman-activity-send-request-starter-worker.timer",
    "qintopia-agentos-operations-group-send-ready.timer",
  ]) {
    if (!fs.existsSync(path.join(unitDir, timer))) {
      throw new Error(`expected rendered timer ${timer}`);
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
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Release systemd install test passed.");
