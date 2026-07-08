#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { execFileSync, spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "qintopia-release-base-test-"));

const writeJson = (name, value) => {
  const filePath = path.join(tmpRoot, name);
  fs.writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`, "utf8");
  return filePath;
};

const runGit = (args) =>
  execFileSync("git", args, {
    cwd: repoRoot,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  }).trim();

const tagCommit = (tag) => runGit(["rev-list", "-n", "1", `${tag}^{commit}`]);

const runResolver = (currentTag, releases, runs) => {
  const releasesFile = writeJson("releases.json", releases);
  const runsFile = writeJson("runs.json", { workflow_runs: runs });
  return spawnSync(
    "node",
    [
      "tools/deploy/resolve-release-deploy-base.mjs",
      "--current-tag",
      currentTag,
      "--releases-file",
      releasesFile,
      "--workflow-runs-file",
      runsFile,
    ],
    {
      cwd: repoRoot,
      encoding: "utf8",
    }
  );
};

const assertSuccess = (name, currentTag, releases, runs, expectedTag) => {
  const result = runResolver(currentTag, releases, runs);
  if (result.status !== 0) {
    throw new Error(
      `${name}: expected success, got ${result.status}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`
    );
  }
  const actual = result.stdout.trim();
  if (actual !== expectedTag) {
    throw new Error(`${name}: expected ${expectedTag}, got ${actual}`);
  }
};

try {
  const releases = [
    { tag_name: "v0.2.2", draft: false, prerelease: false },
    { tag_name: "v0.2.1", draft: false, prerelease: false },
    { tag_name: "v0.2.0", draft: false, prerelease: false },
  ];

  assertSuccess(
    "uses-latest-successful-deployed-release",
    "v0.2.2",
    releases,
    [
      {
        display_title: "v0.2.2",
        event: "release",
        conclusion: "failure",
        head_sha: tagCommit("v0.2.2"),
      },
      {
        display_title: "v0.2.1",
        event: "release",
        conclusion: "failure",
        head_sha: tagCommit("v0.2.1"),
      },
      {
        display_title: "v0.2.0",
        event: "release",
        conclusion: "success",
        head_sha: tagCommit("v0.2.0"),
      },
    ],
    "v0.2.0"
  );

  assertSuccess(
    "falls-back-to-previous-published-release",
    "v0.2.2",
    releases,
    [],
    "v0.2.1"
  );
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Release deploy base resolver tests passed.");
