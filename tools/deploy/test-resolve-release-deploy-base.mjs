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

const fixtureRepo = path.join(tmpRoot, "repo");

const runGit = (args, cwd = fixtureRepo) =>
  execFileSync("git", args, {
    cwd,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  }).trim();

const tagCommit = (tag) => runGit(["rev-list", "-n", "1", `${tag}^{commit}`]);

const createTaggedCommit = (tag, content) => {
  fs.writeFileSync(path.join(fixtureRepo, "file.txt"), `${content}\n`, "utf8");
  runGit(["add", "file.txt"]);
  runGit(["commit", "-m", `commit ${tag}`]);
  runGit(["tag", tag]);
  return tagCommit(tag);
};

const runResolver = (currentTag, releases, runs, results = []) => {
  const releasesFile = writeJson("releases.json", releases);
  const runsFile = writeJson("runs.json", { workflow_runs: runs });
  const resultsFile = writeJson("results.json", results);
  return spawnSync(
    "node",
    [
      path.join(repoRoot, "tools/deploy/resolve-release-deploy-base.mjs"),
      "--current-tag",
      currentTag,
      "--releases-file",
      releasesFile,
      "--workflow-runs-file",
      runsFile,
      "--deploy-results-file",
      resultsFile,
    ],
    {
      cwd: fixtureRepo,
      encoding: "utf8",
    }
  );
};

const assertSuccess = (name, currentTag, releases, runs, results, expectedTag) => {
  const result = runResolver(currentTag, releases, runs, results);
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
  fs.mkdirSync(fixtureRepo, { recursive: true });
  runGit(["init"], fixtureRepo);
  runGit(["config", "user.email", "codex@example.invalid"]);
  runGit(["config", "user.name", "Codex Test"]);

  const v020Sha = createTaggedCommit("v0.2.0", "v0.2.0");
  const v021Sha = createTaggedCommit("v0.2.1", "v0.2.1");
  const v022Sha = createTaggedCommit("v0.2.2", "v0.2.2");

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
        head_branch: "v0.2.2",
        event: "release",
        conclusion: "failure",
        head_sha: v022Sha,
      },
      {
        head_branch: "v0.2.1",
        event: "release",
        conclusion: "failure",
        head_sha: v021Sha,
      },
      {
        head_branch: "v0.2.0",
        event: "release",
        conclusion: "success",
        head_sha: v020Sha,
      },
    ],
    [{ status: "succeeded", release_sha: v020Sha }],
    "v0.2.0"
  );

  assertSuccess(
    "falls-back-to-display-title-for-older-run-payloads",
    "v0.2.2",
    releases,
    [
      {
        display_title: "v0.2.0",
        event: "release",
        conclusion: "success",
        head_sha: v020Sha,
      },
    ],
    [{ status: "succeeded", release_sha: v020Sha }],
    "v0.2.0"
  );

  assertSuccess(
    "skips-non-tag-head-branch-before-display-title",
    "v0.2.2",
    releases,
    [
      {
        head_branch: "master",
        display_title: "v0.2.0",
        event: "release",
        conclusion: "success",
        head_sha: v020Sha,
      },
    ],
    [{ status: "succeeded", release_sha: v020Sha }],
    "v0.2.0"
  );

  assertSuccess(
    "falls-back-to-previous-published-release",
    "v0.2.2",
    releases,
    [],
    [],
    "v0.2.1"
  );

  assertSuccess(
    "does-not-treat-dry-run-result-as-deployed",
    "v0.2.2",
    releases,
    [
      {
        head_branch: "v0.2.1",
        event: "release",
        conclusion: "success",
        head_sha: v021Sha,
      },
      {
        head_branch: "v0.2.0",
        event: "release",
        conclusion: "success",
        head_sha: v020Sha,
      },
    ],
    [
      { status: "dry_run_succeeded", release_sha: v021Sha },
      { status: "succeeded", release_sha: v020Sha },
    ],
    "v0.2.0"
  );

  assertSuccess(
    "ignores-successful-run-without-server-result",
    "v0.2.2",
    releases,
    [
      {
        head_branch: "v0.2.1",
        event: "release",
        conclusion: "success",
        head_sha: v021Sha,
      },
    ],
    [],
    "v0.2.1"
  );
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Release deploy base resolver tests passed.");
