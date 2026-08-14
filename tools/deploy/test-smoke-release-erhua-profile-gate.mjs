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

const runSmoke = (extraArgs = []) =>
  spawnSync(
    "bash",
    [
      path.join(repoRoot, "deploy/runner/smoke-release.sh"),
      "--release-root",
      path.join(tmpRoot, "releases"),
      "--restart-targets",
      "hermes-erhua",
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
exit 0
`
  );

  const ordinaryRestart = runSmoke();
  if (ordinaryRestart.status !== 0) {
    throw new Error(
      `ordinary Erhua restart must not require profile overlay verification\nstdout:\n${ordinaryRestart.stdout}\nstderr:\n${ordinaryRestart.stderr}`
    );
  }

  const metadataPath = path.join(tmpRoot, "profile-backups", "metadata.json");
  fs.mkdirSync(path.dirname(metadataPath), { recursive: true });
  fs.writeFileSync(metadataPath, "{}\n");
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
