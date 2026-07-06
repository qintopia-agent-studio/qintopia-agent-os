#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import process from "node:process";

const run = (command, args, options = {}) =>
  execFileSync(command, args, {
    encoding: "utf8",
    stdio: options.stdio ?? ["ignore", "pipe", "pipe"],
  }).trim();

const eventName = process.env.GITHUB_EVENT_NAME ?? "";
const baseSha = process.env.GITHUB_BASE_SHA ?? "";
const headSha = process.env.GITHUB_SHA ?? "";

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
  if (baseSha && headSha && !/^0+$/.test(baseSha)) {
    return `${baseSha}..${headSha}`;
  }
  if (eventName === "pull_request") {
    return "origin/master..HEAD";
  }
  return localDefaultRange();
})();

let subjects = [];
try {
  const output = run("git", ["log", "--format=%s", commitRange]);
  subjects = output ? output.split("\n").filter(Boolean) : [];
} catch (error) {
  console.error(`Failed to read commit messages for range ${commitRange}`);
  console.error(error.stderr || error.message);
  process.exit(1);
}

if (subjects.length === 0) {
  console.log(`No commit messages to validate for range ${commitRange}.`);
  process.exit(0);
}

for (const subject of subjects) {
  if (/^Merge pull request #\d+ /.test(subject)) {
    continue;
  }

  try {
    execFileSync("pnpm", ["exec", "commitlint"], {
      input: `${subject}\n`,
      encoding: "utf8",
      stdio: ["pipe", "pipe", "pipe"],
    });
  } catch (error) {
    console.error(`Invalid commit subject: ${subject}`);
    console.error(error.stdout || error.stderr || error.message);
    process.exit(1);
  }
}

console.log(`Commit message check passed for ${subjects.length} commit(s).`);
