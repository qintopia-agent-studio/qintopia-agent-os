#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpRoot = fs.mkdtempSync(
  path.join(os.tmpdir(), "qintopia-hermes-cron-snapshot-install-")
);
const sourceScript = path.join(
  repoRoot,
  "deploy/sidecar/scripts/install-hermes-cron-snapshot-timer.sh"
);

const writeExecutable = (filePath, content) => {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, content, "utf8");
  fs.chmodSync(filePath, 0o755);
};

try {
  const releaseCurrent = path.join(tmpRoot, "qintopia-agent-os-releases", "current");
  const releaseTarget = path.join(tmpRoot, "qintopia-agent-os-releases", "release");
  const scriptDir = path.join(releaseTarget, "deploy", "sidecar", "scripts");
  const homeDir = path.join(tmpRoot, "ubuntu");
  const unitDir = path.join(homeDir, ".config", "systemd", "user");
  const fakeBin = path.join(tmpRoot, "bin");
  const fakeSystemctl = path.join(fakeBin, "systemctl");
  const fakeRunuser = path.join(fakeBin, "runuser");
  const fakeStat = path.join(fakeBin, "stat");

  fs.mkdirSync(scriptDir, { recursive: true });
  fs.mkdirSync(homeDir, { recursive: true });
  fs.symlinkSync(releaseTarget, releaseCurrent);

  writeExecutable(
    path.join(scriptDir, "sync-hermes-cron-snapshot.sh"),
    `#!/usr/bin/env bash
echo "live_jobs_json=must-not-leak" >&2
exit 73
`
  );
  writeExecutable(
    fakeSystemctl,
    `#!/usr/bin/env bash
exit 0
`
  );
  writeExecutable(
    fakeRunuser,
    `#!/usr/bin/env bash
shift 2
exec "$@"
`
  );
  writeExecutable(
    fakeStat,
    `#!${process.execPath}
const fs = require("node:fs");
const args = process.argv.slice(2);
if (args[0] === "-c" && args[1] === "%u") {
  console.log(fs.statSync(args[2]).uid);
} else if (args[0] === "-c" && args[1] === "%g") {
  console.log(fs.statSync(args[2]).gid);
} else {
  process.exit(64);
}
`
  );

  const testScript = path.join(tmpRoot, "install-hermes-cron-snapshot-timer.sh");
  const source = fs
    .readFileSync(sourceScript, "utf8")
    .replace('SYSTEMCTL="/usr/bin/systemctl"', `SYSTEMCTL="${fakeSystemctl}"`)
    .replace('RUNUSER="/usr/sbin/runuser"', `RUNUSER="${fakeRunuser}"`)
    .replace('STAT="/usr/bin/stat"', `STAT="${fakeStat}"`)
    .replace(
      'RELEASE_DIR="/home/ubuntu/qintopia-agent-os-releases/current"',
      `RELEASE_DIR="${releaseCurrent}"`
    )
    .replace('UNIT_DIR="/home/ubuntu/.config/systemd/user"', `UNIT_DIR="${unitDir}"`)
    .replace('HOME_DIR="/home/ubuntu"', `HOME_DIR="${homeDir}"`);
  writeExecutable(testScript, source);

  const result = spawnSync(testScript, [], {
    encoding: "utf8",
    env: {
      QINTOPIA_HERMES_CRON_SNAPSHOT: "approved-production-hermes-cron-snapshot",
      PATH: `${fakeBin}${path.delimiter}${process.env.PATH ?? ""}`,
    },
  });

  if (result.status !== 1) {
    throw new Error(
      `expected install failure from sync fixture\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`
    );
  }
  if (
    !result.stderr.includes(
      "qintopia_runtime_one_shot_safe_failure=hermes cron snapshot install: baseline snapshot sync failed"
    )
  ) {
    throw new Error(`missing safe failure marker\n${result.stderr}`);
  }
  if (result.stderr.includes("must-not-leak")) {
    throw new Error(`raw sync output leaked\n${result.stderr}`);
  }
  for (const unitName of [
    "hermes-cron-snapshot.service",
    "hermes-cron-snapshot.timer",
  ]) {
    const unitPath = path.join(unitDir, unitName);
    if (!fs.existsSync(unitPath)) {
      throw new Error(`expected unit file ${unitName} to be written`);
    }
  }

  console.log("Hermes cron snapshot install test passed.");
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}
