#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const fixtureRepo = fs.mkdtempSync(
  path.join(os.tmpdir(), "qintopia-release-restart-targets-")
);

const run = (command, args, options = {}) => {
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? fixtureRepo,
    encoding: "utf8",
    ...options,
  });
  if (result.status !== 0) {
    throw new Error(
      `${command} ${args.join(" ")} failed\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`
    );
  }
  return result.stdout.trim();
};

const write = (relativePath, content) => {
  const target = path.join(fixtureRepo, relativePath);
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.writeFileSync(target, content);
};

const commit = (message) => {
  run("git", ["add", "."]);
  run("git", ["commit", "-m", message]);
  return run("git", ["rev-parse", "HEAD"]);
};

const tag = (name) => run("git", ["tag", name]);

const writeJson = (name, value) => {
  const file = path.join(fixtureRepo, name);
  fs.writeFileSync(file, `${JSON.stringify(value, null, 2)}\n`);
  return file;
};

const runResolver = ({ currentTag, releases, results }) => {
  const releasesFile = writeJson("releases.json", releases);
  const resultsFile = writeJson("results.json", results);
  return spawnSync(
    "node",
    [
      path.join(repoRoot, "tools/deploy/resolve-release-restart-targets.mjs"),
      "--current-tag",
      currentTag,
      "--releases-file",
      releasesFile,
      "--deploy-results-file",
      resultsFile,
      "--rules",
      path.join(repoRoot, "deploy/restart-target-rules.yaml"),
    ],
    {
      cwd: fixtureRepo,
      encoding: "utf8",
    }
  );
};

try {
  run("git", ["init", "-b", "master"]);
  run("git", ["config", "user.email", "codex@example.invalid"]);
  run("git", ["config", "user.name", "Codex Test"]);

  write("README.md", "v0.2.0\n");
  const v020Sha = commit("v0.2.0");
  tag("v0.2.0");

  write("runtime/sidecar/src/context_tools.rs", "sidecar identity fix\n");
  const v021Sha = commit("v0.2.1");
  tag("v0.2.1");

  write("docs/operations/production-deploy-runner.md", "deploy docs\n");
  const v022Sha = commit("v0.2.2");
  tag("v0.2.2");

  const releases = [
    { tag_name: "v0.2.2", draft: false, prerelease: false },
    { tag_name: "v0.2.1", draft: false, prerelease: false },
    { tag_name: "v0.2.0", draft: false, prerelease: false },
  ];

  const result = runResolver({
    currentTag: "v0.2.2",
    releases,
    results: [
      {
        status: "dry_run_succeeded",
        release_sha: v021Sha,
        workflow_run: { id: "101", run_started_at: "2026-07-08T05:00:00Z" },
        restart_targets: ["hermes-erhua", "qintopia-system-services"],
      },
      {
        status: "succeeded",
        release_sha: v022Sha,
        previous_sha: v020Sha,
        workflow_run: { id: "102", run_started_at: "2026-07-08T06:00:00Z" },
        restart_targets: ["qintopia-system-services"],
      },
    ],
  });
  if (result.status !== 0) {
    throw new Error(
      `expected success, got ${result.status}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`
    );
  }
  const actual = result.stdout.trim();
  const expected = "hermes-erhua";
  if (actual !== expected) {
    throw new Error(`expected ${expected}, got ${actual}`);
  }

  const noRestart = runResolver({
    currentTag: "v0.2.2",
    releases,
    results: [
      {
        status: "succeeded",
        release_sha: v021Sha,
        previous_sha: v020Sha,
        workflow_run: { id: "101", run_started_at: "2026-07-08T05:00:00Z" },
        restart_targets: ["hermes-erhua", "qintopia-system-services"],
      },
      {
        status: "succeeded",
        release_sha: v022Sha,
        previous_sha: v021Sha,
        workflow_run: { id: "102", run_started_at: "2026-07-08T06:00:00Z" },
        restart_targets: ["qintopia-system-services"],
      },
    ],
  });
  if (noRestart.status !== 0) {
    throw new Error(
      `expected success, got ${noRestart.status}\nstdout:\n${noRestart.stdout}\nstderr:\n${noRestart.stderr}`
    );
  }
  if (noRestart.stdout.trim() !== "") {
    throw new Error(`expected no targets, got ${noRestart.stdout.trim()}`);
  }

  const newestFirst = runResolver({
    currentTag: "v0.2.2",
    releases,
    results: [
      {
        status: "succeeded",
        release_sha: v022Sha,
        previous_sha: v021Sha,
        workflow_run: { id: "102", run_started_at: "2026-07-08T06:00:00Z" },
        restart_targets: ["qintopia-system-services"],
      },
      {
        status: "succeeded",
        release_sha: v021Sha,
        previous_sha: v020Sha,
        workflow_run: { id: "101", run_started_at: "2026-07-08T05:00:00Z" },
        restart_targets: ["hermes-erhua", "qintopia-system-services"],
      },
    ],
  });
  if (newestFirst.status !== 0) {
    throw new Error(
      `expected success, got ${newestFirst.status}\nstdout:\n${newestFirst.stdout}\nstderr:\n${newestFirst.stderr}`
    );
  }
  if (newestFirst.stdout.trim() !== "") {
    throw new Error(
      `expected timestamp sorting to avoid restart, got ${newestFirst.stdout.trim()}`
    );
  }

  if (!v020Sha || !v022Sha) {
    throw new Error("fixture commits were not created");
  }
} finally {
  fs.rmSync(fixtureRepo, { recursive: true, force: true });
}

console.log("Release restart target resolver tests passed.");
