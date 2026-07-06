#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const repoRoot = process.cwd();
const readmePath = "tools/ci/README.md";
const packagePath = "package.json";
const errors = [];

const readText = (relativePath) =>
  fs.readFileSync(path.join(repoRoot, relativePath), "utf8");

if (!fs.existsSync(path.join(repoRoot, readmePath))) {
  errors.push(`${readmePath}: missing CI tool contract`);
} else {
  const readme = readText(readmePath);
  for (const fragment of [
    "docs-only",
    "required checks",
    "production-adjacent",
    "secrets",
    "commit message",
  ]) {
    if (!readme.includes(fragment)) {
      errors.push(`${readmePath}: must mention ${fragment}`);
    }
  }
}

const packageJson = JSON.parse(readText(packagePath));
for (const scriptName of [
  "check:light",
  "registry:check",
  "secrets:check",
  "commitlint:check",
  "pr:check-body",
  "pr:doctor",
  "pr:bootstrap",
  "pr:create",
]) {
  if (!packageJson.scripts?.[scriptName]) {
    errors.push(`${packagePath}: missing ${scriptName}`);
  }
}

for (const requiredPath of [
  "commitlint.config.mjs",
  ".husky/commit-msg",
  "tools/ci/check-commit-messages.mjs",
  "tools/ci/check-pr-body.mjs",
  "tools/agents/pr-body.mjs",
  "tools/agents/pr-doctor.mjs",
  "tools/agents/pr-bootstrap.mjs",
  "tools/agents/create-pr.mjs",
]) {
  if (!fs.existsSync(path.join(repoRoot, requiredPath))) {
    errors.push(`${requiredPath}: required CI or PR gate file is missing`);
  }
}

if (!packageJson.scripts?.["check:light"]?.includes("pnpm commitlint:check")) {
  errors.push("package.json: check:light must include pnpm commitlint:check");
}

const commitMessageCheck = fs.existsSync(
  path.join(repoRoot, "tools/ci/check-commit-messages.mjs")
)
  ? readText("tools/ci/check-commit-messages.mjs")
  : "";
for (const requiredFragment of [
  "GITHUB_EVENT_PATH",
  "pull_request?.base?.sha",
  "pull_request?.head?.sha",
  'eventName === "push"',
  "event.before",
  "event.after",
  "refs/pull/${prNumber}/head",
  'git", ["cat-file", "-e"',
  "--format=%H%x00%P%x00%s",
]) {
  if (commitMessageCheck && !commitMessageCheck.includes(requiredFragment)) {
    errors.push(`tools/ci/check-commit-messages.mjs: must include ${requiredFragment}`);
  }
}

const ciWorkflow = fs.existsSync(path.join(repoRoot, ".github/workflows/ci.yml"))
  ? readText(".github/workflows/ci.yml")
  : "";
if (ciWorkflow && !ciWorkflow.includes("fetch-depth: 0")) {
  errors.push(
    ".github/workflows/ci.yml: checkouts must keep enough history for commitlint"
  );
}

if (ciWorkflow && !ciWorkflow.includes("pnpm pr:check-body")) {
  errors.push(".github/workflows/ci.yml: pull_request checks must validate PR body");
}

if (errors.length > 0) {
  console.error("CI contract check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log("CI contract check passed.");
