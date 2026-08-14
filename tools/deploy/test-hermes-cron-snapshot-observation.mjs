#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpRoot = fs.mkdtempSync(
  path.join(os.tmpdir(), "qintopia-hermes-cron-snapshot-observation-")
);
const sourceScript = path.join(
  repoRoot,
  "deploy/sidecar/scripts/hermes-cron-snapshot-observation-smoke.sh"
);

const writeFile = (filePath, content, mode = 0o600) => {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, content, "utf8");
  fs.chmodSync(filePath, mode);
};

try {
  const homeDir = path.join(tmpRoot, "ubuntu");
  const snapshotRoot = path.join(
    tmpRoot,
    "ubuntu/.local/state/qintopia-agentos/hermes-cron-snapshot"
  );
  const unitDir = path.join(homeDir, ".config/systemd/user");
  const serviceUnit = path.join(unitDir, "hermes-cron-snapshot.service");
  const timerUnit = path.join(unitDir, "hermes-cron-snapshot.timer");
  const syncScript = path.join(
    tmpRoot,
    "qintopia-agent-os-releases/current/deploy/sidecar/scripts/sync-hermes-cron-snapshot.sh"
  );
  const fakeBin = path.join(tmpRoot, "bin");
  const fakeGit = path.join(fakeBin, "git");
  const fakeId = path.join(fakeBin, "id");
  const fakeRunuser = path.join(fakeBin, "runuser");
  const fakeStat = path.join(fakeBin, "stat");
  const gitCallLog = path.join(tmpRoot, "git-calls.log");

  fs.mkdirSync(path.join(snapshotRoot, ".git"), { recursive: true, mode: 0o700 });
  fs.chmodSync(snapshotRoot, 0o700);
  writeFile(serviceUnit, `[Service]\nExecStart=${syncScript}\n`);
  writeFile(timerUnit, `[Timer]\nOnUnitActiveSec=5min\n`);

  writeFile(
    fakeId,
    `#!/usr/bin/env bash
if [[ "$1" == "-u" ]]; then
  echo 0
  exit 0
fi
exit 64
`,
    0o755
  );
  writeFile(
    fakeRunuser,
    `#!${process.execPath}
const { spawnSync } = require("node:child_process");
const args = process.argv.slice(2);
if (args[0] !== "-u" || args[1] !== "ubuntu" || args[2] !== "--") process.exit(64);
const command = args[3];
const commandArgs = args.slice(4);
if (command.endsWith("/env") && commandArgs[0] === "-i") {
  commandArgs.splice(1, 0, "QINTOPIA_FAKE_RUNUSER_UBUNTU=1");
}
const result = spawnSync(command, commandArgs, {
  stdio: "inherit",
  env: { ...process.env, QINTOPIA_FAKE_RUNUSER_UBUNTU: "1" },
});
process.exit(result.status ?? 1);
`,
    0o755
  );
  writeFile(
    fakeGit,
    `#!${process.execPath}
const fs = require("node:fs");
const args = process.argv.slice(2);
fs.appendFileSync(${JSON.stringify(gitCallLog)}, JSON.stringify({
  viaRunuser: process.env.QINTOPIA_FAKE_RUNUSER_UBUNTU === "1",
  args,
}) + "\\n");
if (process.env.QINTOPIA_FAKE_RUNUSER_UBUNTU !== "1") {
  console.error("direct root git access must not be used");
  process.exit(128);
}
if (args[0] !== "-C") process.exit(64);
const command = args[2];
if (command === "remote") process.exit(0);
if (command === "log" && args.includes("--format=%ct")) {
  console.log("1786320600");
  process.exit(0);
}
process.exit(65);
`,
    0o755
  );
  writeFile(
    fakeStat,
    `#!${process.execPath}
const fs = require("node:fs");
const args = process.argv.slice(2);
if (args[0] !== "-c" || args[1] !== "%a") process.exit(64);
console.log((fs.statSync(args[2]).mode & 0o777).toString(8));
`,
    0o755
  );

  const testScript = path.join(tmpRoot, "hermes-cron-snapshot-observation-smoke.sh");
  const source = fs
    .readFileSync(sourceScript, "utf8")
    .replace('GIT_BIN="/usr/bin/git"', `GIT_BIN="${fakeGit}"`)
    .replace('ID_BIN="/usr/bin/id"', `ID_BIN="${fakeId}"`)
    .replace('RUNUSER_BIN="/usr/sbin/runuser"', `RUNUSER_BIN="${fakeRunuser}"`)
    .replace('STAT_BIN="/usr/bin/stat"', `STAT_BIN="${fakeStat}"`)
    .replace(
      'SNAPSHOT_ROOT="/home/ubuntu/.local/state/qintopia-agentos/hermes-cron-snapshot"',
      `SNAPSHOT_ROOT="${snapshotRoot}"`
    )
    .replace('HOME_DIR="/home/ubuntu"', `HOME_DIR="${homeDir}"`)
    .replace('UNIT_DIR="/home/ubuntu/.config/systemd/user"', `UNIT_DIR="${unitDir}"`)
    .replace(
      'SYNC_SCRIPT="/home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/sync-hermes-cron-snapshot.sh"',
      `SYNC_SCRIPT="${syncScript}"`
    );
  writeFile(testScript, source, 0o755);

  const result = spawnSync(testScript, [], {
    encoding: "utf8",
    env: {
      ...process.env,
      QINTOPIA_HERMES_CRON_SNAPSHOT_OBSERVATION_ENABLE: "1",
    },
  });

  if (result.status !== 0) {
    throw new Error(
      `expected observation success\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`
    );
  }
  if (!result.stdout.includes("hermes_cron_snapshot_observation_result=success")) {
    throw new Error(`missing success marker\n${result.stdout}`);
  }
  if (result.stdout.includes("repo_commit_missing")) {
    throw new Error(`commit was not observed through runuser\n${result.stdout}`);
  }

  const calls = fs
    .readFileSync(gitCallLog, "utf8")
    .trim()
    .split("\n")
    .filter(Boolean)
    .map((line) => JSON.parse(line));
  if (calls.length !== 2 || calls.some((call) => call.viaRunuser !== true)) {
    throw new Error(
      `git was not constrained through runuser\n${JSON.stringify(calls)}`
    );
  }

  console.log("Hermes cron snapshot observation test passed.");
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}
