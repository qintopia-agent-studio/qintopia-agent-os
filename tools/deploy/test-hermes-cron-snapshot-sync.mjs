#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpRoot = fs.mkdtempSync(
  path.join(os.tmpdir(), "qintopia-hermes-cron-snapshot-sync-")
);
const sourceScript = path.join(
  repoRoot,
  "deploy/sidecar/scripts/sync-hermes-cron-snapshot.sh"
);

const writeExecutable = (filePath, content) => {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, content, "utf8");
  fs.chmodSync(filePath, 0o755);
};

const commandPath = (name) => {
  const result = spawnSync("which", [name], { encoding: "utf8" });
  const resolved = result.stdout.trim();
  if (!resolved) {
    throw new Error(`${name} is required for the Hermes cron snapshot fixture`);
  }
  return resolved;
};

const modeOf = (filePath) => fs.statSync(filePath).mode & 0o777;
const run = (cmd, args, options = {}) =>
  spawnSync(cmd, args, { encoding: "utf8", ...options });

try {
  const homeDir = path.join(tmpRoot, "ubuntu-home");
  const hermesHome = path.join(homeDir, ".hermes");
  const snapshotRoot = path.join(
    homeDir,
    ".local",
    "state",
    "qintopia-agentos",
    "hermes-cron-snapshot"
  );
  const snapshotParent = path.dirname(snapshotRoot);
  const scriptPath = path.join(tmpRoot, "sync-hermes-cron-snapshot.sh");
  const fakeBin = path.join(tmpRoot, "bin");
  const fakeStat = path.join(fakeBin, "stat");
  const fakeChown = path.join(fakeBin, "chown");
  const fakeRunuser = path.join(fakeBin, "runuser");

  fs.mkdirSync(path.join(hermesHome, "profiles", "erhua", "cron"), {
    recursive: true,
  });
  fs.mkdirSync(path.join(hermesHome, "profiles", "erhua", "scripts"), {
    recursive: true,
  });
  fs.mkdirSync(path.join(hermesHome, "scripts"), { recursive: true });
  fs.writeFileSync(
    path.join(hermesHome, "profiles", "erhua", "cron", "jobs.json"),
    JSON.stringify({ schema_version: 1, jobs: [] }, null, 2) + "\n",
    "utf8"
  );
  fs.writeFileSync(
    path.join(
      hermesHome,
      "profiles",
      "erhua",
      "scripts",
      "qintopia_erhua_morning_brief.sh"
    ),
    "#!/usr/bin/env bash\nexit 0\n",
    "utf8"
  );
  fs.writeFileSync(
    path.join(hermesHome, "profiles", "erhua", "scripts", "unreviewed.sh"),
    "#!/usr/bin/env bash\nexit 0\n",
    "utf8"
  );
  fs.writeFileSync(
    path.join(hermesHome, "scripts", "qintopia_fixture.sh"),
    "#!/usr/bin/env bash\nexit 0\n",
    "utf8"
  );
  const secretSource = path.join(tmpRoot, "message-sidecar.env");
  fs.writeFileSync(secretSource, "QIWE_TOKEN=must-not-enter-snapshot\n", "utf8");
  fs.symlinkSync(
    secretSource,
    path.join(
      hermesHome,
      "profiles",
      "erhua",
      "scripts",
      "qintopia_erhua_morning_brief_symlink.sh"
    )
  );
  fs.symlinkSync(
    secretSource,
    path.join(hermesHome, "scripts", "qintopia_fixture_symlink.sh")
  );

  fs.mkdirSync(snapshotRoot, { recursive: true });
  fs.chmodSync(snapshotRoot, 0o700);
  let result;
  result = run(commandPath("git"), ["-C", snapshotParent, "init", "--quiet"]);
  if (result.status !== 0) {
    throw new Error(`parent repo init failed\n${result.stderr}`);
  }
  result = run(commandPath("git"), [
    "-C",
    snapshotParent,
    "remote",
    "add",
    "origin",
    "https://example.invalid/leaky-snapshot-parent.git",
  ]);
  if (result.status !== 0) {
    throw new Error(`parent repo remote setup failed\n${result.stderr}`);
  }

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
  console.error("unsupported stat fixture args");
  process.exit(64);
}
`
  );
  writeExecutable(
    fakeChown,
    `#!/usr/bin/env bash
echo "fixture chown should not run for non-root tests" >&2
exit 65
`
  );
  writeExecutable(
    fakeRunuser,
    `#!/usr/bin/env bash
echo "fixture runuser should not run for non-root tests" >&2
exit 66
`
  );

  const source = fs
    .readFileSync(sourceScript, "utf8")
    .replace(
      "    /home/ubuntu | /home/ubuntu/* | /usr/bin/* | /usr/sbin/*) ;;",
      "    *) ;;"
    )
    .replaceAll(
      "/home/ubuntu/.local/state/qintopia-agentos/hermes-cron-snapshot",
      snapshotRoot
    )
    .replaceAll("/home/ubuntu/.hermes", hermesHome)
    .replaceAll("/home/ubuntu", homeDir)
    .replaceAll("/usr/bin/python3", commandPath("python3"))
    .replaceAll("/usr/bin/git", commandPath("git"))
    .replaceAll("/usr/bin/stat", fakeStat)
    .replaceAll("/usr/bin/chmod", commandPath("chmod"))
    .replaceAll("/usr/bin/chown", fakeChown)
    .replaceAll("/usr/bin/find", commandPath("find"))
    .replaceAll("/usr/sbin/runuser", fakeRunuser);
  writeExecutable(scriptPath, source);

  result = run("bash", [scriptPath], { cwd: repoRoot });
  if (result.status === 0) {
    throw new Error("snapshot sync accepted invalid git repo without approval");
  }
  if (
    !result.stderr.includes(
      "first snapshot init requires QINTOPIA_HERMES_CRON_SNAPSHOT"
    )
  ) {
    throw new Error(`unexpected missing-approval failure\n${result.stderr}`);
  }

  result = run("bash", [scriptPath], {
    cwd: repoRoot,
    env: {
      ...process.env,
      QINTOPIA_HERMES_CRON_SNAPSHOT: "approved-production-hermes-cron-snapshot",
    },
  });
  if (result.status !== 0) {
    throw new Error(`snapshot sync failed\n${result.stdout}\n${result.stderr}`);
  }
  if (
    !result.stdout.includes("snapshot_files_copied=3 snapshot_files_removed=0") ||
    !result.stdout.includes("snapshot_commit=created")
  ) {
    throw new Error(`snapshot sync evidence incomplete\n${result.stdout}`);
  }

  result = run(commandPath("git"), ["-C", snapshotRoot, "rev-parse", "--git-dir"]);
  if (result.status !== 0) {
    throw new Error(`snapshot repo was not initialized\n${result.stderr}`);
  }
  result = run(commandPath("git"), ["-C", snapshotRoot, "remote"]);
  if (result.status !== 0 || result.stdout.trim() !== "") {
    throw new Error("snapshot repo must not have a remote");
  }
  result = run(commandPath("git"), ["-C", snapshotRoot, "rev-list", "--count", "HEAD"]);
  if (result.status !== 0 || result.stdout.trim() !== "1") {
    throw new Error("snapshot sync did not create exactly one commit");
  }
  if (
    modeOf(snapshotRoot) !== 0o700 ||
    modeOf(path.join(snapshotRoot, ".git")) !== 0o700 ||
    modeOf(path.join(snapshotRoot, "profiles")) !== 0o700 ||
    modeOf(path.join(snapshotRoot, "profiles", "erhua", "cron", "jobs.json")) !==
      0o600 ||
    modeOf(
      path.join(
        snapshotRoot,
        "profiles",
        "erhua",
        "scripts",
        "qintopia_erhua_morning_brief.sh"
      )
    ) !== 0o600
  ) {
    throw new Error("snapshot sync did not normalize repo permissions");
  }
  if (
    fs.existsSync(
      path.join(snapshotRoot, "profiles", "erhua", "scripts", "unreviewed.sh")
    ) ||
    fs.existsSync(
      path.join(
        snapshotRoot,
        "profiles",
        "erhua",
        "scripts",
        "qintopia_erhua_morning_brief_symlink.sh"
      )
    ) ||
    fs.existsSync(path.join(snapshotRoot, "scripts", "qintopia_fixture_symlink.sh"))
  ) {
    throw new Error("snapshot sync copied unreviewed files or symlinks");
  }

  result = run("bash", [scriptPath], {
    cwd: repoRoot,
    env: {
      ...process.env,
    },
  });
  if (result.status !== 0) {
    throw new Error(
      `idempotent snapshot sync failed\n${result.stdout}\n${result.stderr}`
    );
  }
  if (!result.stdout.includes("snapshot_commit=skipped-no-changes")) {
    throw new Error("idempotent snapshot sync did not skip unchanged state");
  }
  const snapshotWrapperPath = path.join(
    snapshotRoot,
    "profiles",
    "erhua",
    "scripts",
    "qintopia_erhua_morning_brief.sh"
  );
  fs.unlinkSync(snapshotWrapperPath);
  fs.symlinkSync(secretSource, snapshotWrapperPath);
  result = run("bash", [scriptPath], {
    cwd: repoRoot,
    env: {
      ...process.env,
    },
  });
  if (result.status !== 0) {
    throw new Error(
      `snapshot sync failed to replace a destination symlink\n${result.stdout}\n${result.stderr}`
    );
  }
  if (
    fs.lstatSync(snapshotWrapperPath).isSymbolicLink() ||
    fs.readFileSync(snapshotWrapperPath, "utf8").includes("QIWE_TOKEN") ||
    fs.readFileSync(secretSource, "utf8") !== "QIWE_TOKEN=must-not-enter-snapshot\n"
  ) {
    throw new Error("snapshot sync followed or preserved a destination symlink");
  }
  const staleSnapshotSymlink = path.join(
    snapshotRoot,
    "profiles",
    "erhua",
    "scripts",
    "stale-symlink.sh"
  );
  fs.symlinkSync(path.join(tmpRoot, "missing-secret.env"), staleSnapshotSymlink);
  result = run("bash", [scriptPath], {
    cwd: repoRoot,
    env: {
      ...process.env,
    },
  });
  if (result.status !== 0) {
    throw new Error(
      `snapshot sync failed to remove a stale symlink\n${result.stdout}\n${result.stderr}`
    );
  }
  if (
    fs.existsSync(staleSnapshotSymlink) ||
    fs.lstatSync(path.dirname(staleSnapshotSymlink)).isSymbolicLink()
  ) {
    throw new Error("snapshot sync did not remove a stale symlink");
  }
  result = run(commandPath("git"), [
    "-C",
    snapshotRoot,
    "rev-parse",
    "--show-toplevel",
  ]);
  if (
    result.status !== 0 ||
    fs.realpathSync(result.stdout.trim()) !== fs.realpathSync(snapshotRoot)
  ) {
    throw new Error("snapshot sync did not bind git to the fixed snapshot root");
  }
  if (
    fs.existsSync(path.join(snapshotParent, "profiles")) ||
    fs.existsSync(path.join(snapshotParent, "scripts"))
  ) {
    throw new Error("snapshot sync wrote sensitive files into the parent git repo");
  }

  console.log("Hermes cron snapshot sync fixture passed.");
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}
