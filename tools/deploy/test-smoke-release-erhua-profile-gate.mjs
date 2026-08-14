#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "qintopia-smoke-release-"));

const writeExecutable = (relativePath, content) => {
  const target = path.join(tmpRoot, relativePath);
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.writeFileSync(target, content, { mode: 0o755 });
};

const runSmoke = (extraArgs = [], restartTargets = "hermes-erhua") =>
  spawnSync(
    "bash",
    [
      path.join(repoRoot, "deploy/runner/smoke-release.sh"),
      "--release-root",
      path.join(tmpRoot, "releases"),
      "--restart-targets",
      restartTargets,
      ...extraArgs,
    ],
    {
      cwd: repoRoot,
      env: {
        ...process.env,
        PATH: `${path.join(tmpRoot, "bin")}${path.delimiter}${process.env.PATH ?? ""}`,
        QINTOPIA_HERMES_SYSTEMD_USER: "ubuntu",
        QINTOPIA_ERHUA_PROFILE_DIR: path.join(tmpRoot, "profiles", "erhua"),
        QINTOPIA_HERMES_BIN: "/home/ubuntu/.local/bin/hermes",
        QINTOPIA_HERMES_PYTHON: "/home/ubuntu/.hermes/hermes-agent/venv/bin/python",
      },
      encoding: "utf8",
    }
  );

try {
  fs.mkdirSync(path.join(tmpRoot, "profiles", "erhua"), { recursive: true });
  fs.mkdirSync(path.join(tmpRoot, "releases", "current"), { recursive: true });
  writeExecutable(
    "bin/runuser",
    `#!/usr/bin/env bash
joined="$*"
if [[ "$joined" == *"verify_runtime_provider.py"* || "$joined" == *"--profile erhua doctor"* ]]; then
  echo "ordinary restart must not run Erhua provider checks" >&2
  exit 41
fi
exit 0
`
  );

  const ordinaryRestart = runSmoke();
  if (ordinaryRestart.status !== 0) {
    throw new Error(
      `ordinary Erhua restart must not require profile overlay verification\nstdout:\n${ordinaryRestart.stdout}\nstderr:\n${ordinaryRestart.stderr}`
    );
  }

  writeExecutable(
    "bin/systemctl",
    `#!/usr/bin/env bash
if [[ "\${1:-}" == "restart" && "\${2:-}" == "qintopia-agentos-daily-digest-publisher.service" ]]; then
  exit 77
fi
exit 0
`
  );
  const systemServiceRestart = runSmoke([], "qintopia-system-services");
  if (systemServiceRestart.status === 0) {
    throw new Error(
      `expected fixed system service restart failure\nstdout:\n${systemServiceRestart.stdout}\nstderr:\n${systemServiceRestart.stderr}`
    );
  }
  if (
    !systemServiceRestart.stderr.includes(
      "qintopia_smoke_release_safe_failure=target=qintopia-system-services;phase=restart;subject=qintopia-agentos-daily-digest-publisher.service"
    )
  ) {
    throw new Error(
      `system service restart failure did not emit safe marker\nstderr:\n${systemServiceRestart.stderr}`
    );
  }

  const metadataPath = path.join(tmpRoot, "profile-backups", "metadata.json");
  fs.mkdirSync(path.dirname(metadataPath), { recursive: true });
  fs.writeFileSync(metadataPath, "{}\n");
  writeExecutable(
    "bin/python3",
    `#!/usr/bin/env bash
if [[ "$*" == *"verify-activated"* ]]; then
  echo "activated-file verification reached" >&2
  exit 42
fi
exec /usr/bin/python3 "$@"
`
  );
  const profileActivation = runSmoke(["--profile-metadata", metadataPath]);
  if (profileActivation.status === 0) {
    throw new Error(
      "profile activation smoke must still require activated-file verification"
    );
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Smoke release Erhua profile gate tests passed.");
