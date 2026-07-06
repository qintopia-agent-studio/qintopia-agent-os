#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import process from "node:process";

const run = (command, args, options = {}) =>
  execFileSync(command, args, {
    encoding: "utf8",
    stdio: options.stdio ?? ["ignore", "pipe", "pipe"],
  }).trim();

const eventName = process.env.GITHUB_EVENT_NAME ?? "";
const baseSha = process.env.GITHUB_BASE_SHA ?? "";
const headSha = process.env.GITHUB_SHA ?? "";
const eventPath = process.env.GITHUB_EVENT_PATH ?? "";

const pullRequestEvent = () => {
  if (!eventPath || !fs.existsSync(eventPath)) {
    return {};
  }
  try {
    return JSON.parse(fs.readFileSync(eventPath, "utf8"));
  } catch {
    return {};
  }
};

const commitExists = (sha) => {
  try {
    run("git", ["cat-file", "-e", `${sha}^{commit}`]);
    return true;
  } catch {
    return false;
  }
};

const fetchCommit = (sha, refspecs = []) => {
  if (!sha || commitExists(sha)) {
    return;
  }

  for (const refspec of [sha, ...refspecs]) {
    try {
      run("git", ["fetch", "--no-tags", "--depth=50", "origin", refspec]);
    } catch {
      // Continue through fallback refspecs; the final range read reports failure.
    }
    if (commitExists(sha)) {
      return;
    }
  }
};

const currentBranch = () => {
  try {
    return run("git", ["rev-parse", "--abbrev-ref", "HEAD"]);
  } catch {
    return "";
  }
};

const localDefaultRange = () => {
  const branch = currentBranch();
  if (branch === "master") {
    return "HEAD~1..HEAD";
  }

  try {
    const upstream = run("git", [
      "rev-parse",
      "--abbrev-ref",
      "--symbolic-full-name",
      "@{u}",
    ]);
    const mergeBase = run("git", ["merge-base", upstream, "HEAD"]);
    return `${mergeBase}..HEAD`;
  } catch {
    const mergeBase = run("git", ["merge-base", "origin/master", "HEAD"]);
    return `${mergeBase}..HEAD`;
  }
};

const commitRange = (() => {
  if (process.argv[2]) {
    return process.argv[2];
  }
  if (eventName === "pull_request") {
    const event = pullRequestEvent();
    const prNumber = event.pull_request?.number;
    const prBaseRef = event.pull_request?.base?.ref;
    const prBaseSha = event.pull_request?.base?.sha;
    const prHeadSha = event.pull_request?.head?.sha;
    if (prBaseSha && prHeadSha) {
      fetchCommit(prBaseSha, prBaseRef ? [`refs/heads/${prBaseRef}`] : []);
      fetchCommit(prHeadSha, prNumber ? [`refs/pull/${prNumber}/head`] : []);
      return `${prBaseSha}..${prHeadSha}`;
    }
    return "origin/master..HEAD";
  }
  if (baseSha && headSha && !/^0+$/.test(baseSha)) {
    return `${baseSha}..${headSha}`;
  }
  return localDefaultRange();
})();

let commits = [];
try {
  const output = run("git", ["log", "--format=%H%x00%P%x00%s", commitRange]);
  commits = output
    ? output
        .split("\n")
        .filter(Boolean)
        .map((line) => {
          const [sha, parents, subject] = line.split("\0");
          return {
            sha,
            parents: parents ? parents.split(" ").filter(Boolean) : [],
            subject,
          };
        })
    : [];
} catch (error) {
  console.error(`Failed to read commit messages for range ${commitRange}`);
  console.error(error.stderr || error.message);
  process.exit(1);
}

if (commits.length === 0) {
  console.log(`No commit messages to validate for range ${commitRange}.`);
  process.exit(0);
}

let checkedCount = 0;
for (const commit of commits) {
  if (commit.parents.length > 1 || /^Merge /.test(commit.subject)) {
    continue;
  }

  try {
    execFileSync("pnpm", ["exec", "commitlint"], {
      input: `${commit.subject}\n`,
      encoding: "utf8",
      stdio: ["pipe", "pipe", "pipe"],
    });
    checkedCount += 1;
  } catch (error) {
    console.error(`Invalid commit subject: ${commit.subject}`);
    console.error(error.stdout || error.stderr || error.message);
    process.exit(1);
  }
}

console.log(`Commit message check passed for ${checkedCount} commit(s).`);
