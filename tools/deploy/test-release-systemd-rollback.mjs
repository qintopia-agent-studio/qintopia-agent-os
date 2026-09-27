#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "qintopia-systemd-rollback-"));
const candidateSha = "0123456789abcdef0123456789abcdef01234567";
const previousSha = "abcdef0123456789abcdef0123456789abcdef01";
const restorePreviousSha = "fedcba9876543210fedcba9876543210fedcba98";

const writeExecutable = (filePath, content) => {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, content, "utf8");
  fs.chmodSync(filePath, 0o755);
};

const installer = ({ units, marker, installBody = "" }) => `#!/usr/bin/env bash
set -euo pipefail
unit_files=(
${units.map((unit) => `  ${unit}`).join("\n")}
)
runner_unit_files=(
  qintopia-agent-os-deploy-runner.service
  qintopia-agent-os-deploy-runner.timer
)
${installBody}
printf '%s\\n' '${marker}' >>"\${ROLLBACK_TEST_LOG}"
`;

try {
  const releaseRoot = path.join(tmpRoot, "releases");
  const candidateDir = path.join(releaseRoot, candidateSha);
  const previousDir = path.join(releaseRoot, previousSha);
  const restorePreviousDir = path.join(releaseRoot, restorePreviousSha);
  const unitDir = path.join(tmpRoot, "units");
  const logFile = path.join(tmpRoot, "rollback.log");
  const fakeSystemctl = path.join(tmpRoot, "bin", "systemctl");

  fs.mkdirSync(unitDir, { recursive: true });
  fs.mkdirSync(candidateDir, { recursive: true });
  fs.mkdirSync(previousDir, { recursive: true });
  fs.mkdirSync(restorePreviousDir, { recursive: true });
  fs.writeFileSync(
    path.join(candidateDir, "manifest.json"),
    `${JSON.stringify({ release_sha: candidateSha, previous_sha: previousSha })}\n`,
    "utf8"
  );
  fs.writeFileSync(
    path.join(previousDir, "manifest.json"),
    `${JSON.stringify({ release_sha: previousSha })}\n`,
    "utf8"
  );
  fs.writeFileSync(
    path.join(restorePreviousDir, "manifest.json"),
    `${JSON.stringify({ release_sha: restorePreviousSha })}\n`,
    "utf8"
  );
  fs.symlinkSync(candidateDir, path.join(releaseRoot, "current"));
  fs.symlinkSync(previousDir, path.join(releaseRoot, "previous"));
  const resolvedCandidateDir = fs.realpathSync(candidateDir);
  const resolvedPreviousDir = fs.realpathSync(previousDir);
  const resolvedRestorePreviousDir = fs.realpathSync(restorePreviousDir);

  const sharedUnit = "qintopia-agentos-shared.service";
  const candidateOnlyService = "qintopia-agentos-candidate-only.service";
  const candidateOnlyTimer = "qintopia-agentos-candidate-only.timer";
  fs.writeFileSync(path.join(unitDir, sharedUnit), "candidate unit\n", "utf8");
  fs.writeFileSync(
    path.join(unitDir, candidateOnlyService),
    "candidate-only service\n",
    "utf8"
  );
  fs.writeFileSync(
    path.join(unitDir, candidateOnlyTimer),
    "candidate-only timer\n",
    "utf8"
  );

  writeExecutable(
    path.join(candidateDir, "deploy", "runner", "install-release-systemd-units.sh"),
    installer({
      units: [sharedUnit, candidateOnlyService, candidateOnlyTimer],
      marker: "candidate installer must not run",
    })
  );
  writeExecutable(
    path.join(previousDir, "deploy", "runner", "install-release-systemd-units.sh"),
    installer({
      units: [sharedUnit],
      marker: "previous installer ran",
      installBody: `
if [[ "$(readlink -f "${releaseRoot}/current")" != "${resolvedPreviousDir}" ]]; then
  echo "previous installer ran before current pointed to previous" >&2
  exit 71
fi
printf '%s\\n' 'previous unit' >"\${QINTOPIA_SYSTEMD_UNIT_DIR}/${sharedUnit}"
`,
    })
  );

  writeExecutable(
    fakeSystemctl,
    `#!/usr/bin/env bash
set -euo pipefail
printf 'systemctl %s\\n' "$*" >>"${logFile}"
`
  );

  const result = spawnSync(
    "bash",
    [
      path.join(repoRoot, "deploy", "runner", "rollback-release.sh"),
      "--release-root",
      releaseRoot,
      "--expected-current-sha",
      candidateSha,
      "--expected-previous-sha",
      previousSha,
      "--restore-previous-sha",
      restorePreviousSha,
    ],
    {
      cwd: repoRoot,
      env: {
        ...process.env,
        SYSTEMCTL: fakeSystemctl,
        QINTOPIA_SYSTEMD_UNIT_DIR: unitDir,
        ROLLBACK_TEST_LOG: logFile,
      },
      encoding: "utf8",
    }
  );

  if (result.status !== 0) {
    throw new Error(
      `expected release rollback to pass, got ${result.status}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`
    );
  }
  if (fs.realpathSync(path.join(releaseRoot, "current")) !== resolvedPreviousDir) {
    throw new Error("rollback did not restore the previous current target");
  }
  if (
    fs.realpathSync(path.join(releaseRoot, "previous")) !== resolvedRestorePreviousDir
  ) {
    throw new Error("rollback did not restore the original previous target");
  }
  if (
    fs.realpathSync(path.join(releaseRoot, "rollback-from")) !== resolvedCandidateDir
  ) {
    throw new Error("rollback did not retain the candidate rollback-from target");
  }
  if (fs.readFileSync(path.join(unitDir, sharedUnit), "utf8") !== "previous unit\n") {
    throw new Error("rollback did not restore the previous release unit content");
  }
  for (const candidateOnlyUnit of [candidateOnlyService, candidateOnlyTimer]) {
    if (fs.existsSync(path.join(unitDir, candidateOnlyUnit))) {
      throw new Error(`rollback did not remove ${candidateOnlyUnit}`);
    }
  }
  const log = fs.readFileSync(logFile, "utf8");
  for (const required of [
    "previous installer ran",
    `systemctl disable --now ${candidateOnlyService}`,
    `systemctl disable --now ${candidateOnlyTimer}`,
    "systemctl daemon-reload",
  ]) {
    if (!log.includes(required)) {
      throw new Error(`rollback log is missing ${required}`);
    }
  }
  if (log.includes("candidate installer must not run")) {
    throw new Error("rollback ran the candidate installer");
  }

  for (const pointer of ["current", "previous", "rollback-from"]) {
    fs.rmSync(path.join(releaseRoot, pointer), { force: true });
  }
  fs.symlinkSync(candidateDir, path.join(releaseRoot, "current"));
  fs.symlinkSync(previousDir, path.join(releaseRoot, "previous"));
  fs.writeFileSync(path.join(unitDir, sharedUnit), "candidate unit\n", "utf8");
  fs.writeFileSync(
    path.join(unitDir, candidateOnlyService),
    "candidate-only service\n",
    "utf8"
  );
  fs.writeFileSync(
    path.join(unitDir, candidateOnlyTimer),
    "candidate-only timer\n",
    "utf8"
  );
  fs.writeFileSync(logFile, "", "utf8");

  const absentPreviousResult = spawnSync(
    "bash",
    [
      path.join(repoRoot, "deploy", "runner", "rollback-release.sh"),
      "--release-root",
      releaseRoot,
      "--expected-current-sha",
      candidateSha,
      "--expected-previous-sha",
      previousSha,
      "--restore-previous-absent",
    ],
    {
      cwd: repoRoot,
      env: {
        ...process.env,
        SYSTEMCTL: fakeSystemctl,
        QINTOPIA_SYSTEMD_UNIT_DIR: unitDir,
        ROLLBACK_TEST_LOG: logFile,
      },
      encoding: "utf8",
    }
  );

  if (absentPreviousResult.status !== 0) {
    throw new Error(
      `expected absent-previous rollback to pass, got ${absentPreviousResult.status}\nstdout:\n${absentPreviousResult.stdout}\nstderr:\n${absentPreviousResult.stderr}`
    );
  }
  if (fs.realpathSync(path.join(releaseRoot, "current")) !== resolvedPreviousDir) {
    throw new Error("absent-previous rollback did not restore current");
  }
  if (fs.existsSync(path.join(releaseRoot, "previous"))) {
    throw new Error("absent-previous rollback did not remove previous");
  }
  if (
    fs.realpathSync(path.join(releaseRoot, "rollback-from")) !== resolvedCandidateDir
  ) {
    throw new Error("absent-previous rollback did not retain rollback-from");
  }

  const conflictingRestoreResult = spawnSync(
    "bash",
    [
      path.join(repoRoot, "deploy", "runner", "rollback-release.sh"),
      "--release-root",
      releaseRoot,
      "--expected-current-sha",
      candidateSha,
      "--expected-previous-sha",
      previousSha,
      "--restore-previous-sha",
      restorePreviousSha,
      "--restore-previous-absent",
    ],
    { cwd: repoRoot, encoding: "utf8" }
  );
  if (conflictingRestoreResult.status !== 2) {
    throw new Error("conflicting previous restore modes must fail before mutation");
  }

  const realPython = spawnSync("which", ["python3"], {
    encoding: "utf8",
  }).stdout.trim();
  const faultPython = path.join(tmpRoot, "bin", "python3");
  writeExecutable(
    faultPython,
    `#!${realPython}
import os
import subprocess
import sys

args = sys.argv[1:]
if not args or args[0] != "-":
    os.execv(${JSON.stringify(realPython)}, [${JSON.stringify(realPython)}, *args])
code = sys.stdin.read()
phase = os.environ.get("ROLLBACK_FAULT_PHASE", "")
if "temporary = os.path.join(root" in code and len(args) >= 3:
    pointer = args[2]
    if phase == f"before-{pointer}":
        raise SystemExit(78)
    if phase == f"fsync-{pointer}":
        code = code.replace("os.fsync(descriptor)", "raise SystemExit(78)")
result = subprocess.run([${JSON.stringify(realPython)}, "-", *args[1:]], input=code, text=True)
raise SystemExit(result.returncode)
`
  );

  const resetLineage = () => {
    for (const pointer of ["current", "previous", "rollback-from"]) {
      fs.rmSync(path.join(releaseRoot, pointer), { force: true });
    }
    fs.symlinkSync(candidateDir, path.join(releaseRoot, "current"));
    fs.symlinkSync(previousDir, path.join(releaseRoot, "previous"));
  };
  const runFault = (phase) => {
    resetLineage();
    const outcome = spawnSync(
      "bash",
      [
        path.join(repoRoot, "deploy/runner/rollback-release.sh"),
        "--release-root",
        releaseRoot,
        "--expected-current-sha",
        candidateSha,
        "--expected-previous-sha",
        previousSha,
        "--restore-previous-sha",
        restorePreviousSha,
      ],
      {
        cwd: repoRoot,
        encoding: "utf8",
        env: {
          ...process.env,
          PATH: `${path.dirname(faultPython)}:${process.env.PATH}`,
          ROLLBACK_FAULT_PHASE: phase,
          SYSTEMCTL: fakeSystemctl,
          QINTOPIA_SYSTEMD_UNIT_DIR: unitDir,
          ROLLBACK_TEST_LOG: logFile,
        },
      }
    );
    if (outcome.status === 0) {
      throw new Error(`${phase}: real rollback primitive unexpectedly passed`);
    }
    return ["current", "previous", "rollback-from"].map((pointer) => {
      const link = path.join(releaseRoot, pointer);
      return fs.existsSync(link) ? path.basename(fs.realpathSync(link)) : "absent";
    });
  };
  const expectedFaultStates = new Map([
    ["before-rollback-from", [candidateSha, previousSha, "absent"]],
    ["before-current", [candidateSha, previousSha, candidateSha]],
    ["fsync-current", [previousSha, previousSha, candidateSha]],
    ["before-previous", [previousSha, previousSha, candidateSha]],
    ["fsync-previous", [previousSha, restorePreviousSha, candidateSha]],
  ]);
  for (const [phase, expectedState] of expectedFaultStates) {
    const actualState = runFault(phase);
    if (JSON.stringify(actualState) !== JSON.stringify(expectedState)) {
      throw new Error(
        `${phase}: pointer state ${actualState} differs from ${expectedState}`
      );
    }
  }

  writeExecutable(
    path.join(previousDir, "deploy", "runner", "install-release-systemd-units.sh"),
    installer({
      units: [sharedUnit],
      marker: "simulated installer failure",
      installBody: "exit 79",
    })
  );
  const failedInstallerState = runFault("");
  if (
    JSON.stringify(failedInstallerState) !==
    JSON.stringify([previousSha, restorePreviousSha, candidateSha])
  ) {
    throw new Error(
      "installer failure did not leave the complete target pointer state"
    );
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Release systemd rollback test passed.");
