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
]) {
  if (!packageJson.scripts?.[scriptName]) {
    errors.push(`${packagePath}: missing ${scriptName}`);
  }
}

for (const requiredPath of [
  "commitlint.config.mjs",
  ".husky/commit-msg",
  "tools/ci/check-commit-messages.mjs",
]) {
  if (!fs.existsSync(path.join(repoRoot, requiredPath))) {
    errors.push(`${requiredPath}: required commit message gate file is missing`);
  }
}

if (!packageJson.scripts?.["check:light"]?.includes("pnpm commitlint:check")) {
  errors.push("package.json: check:light must include pnpm commitlint:check");
}

const ciWorkflow = fs.existsSync(path.join(repoRoot, ".github/workflows/ci.yml"))
  ? readText(".github/workflows/ci.yml")
  : "";
if (ciWorkflow && !ciWorkflow.includes("GITHUB_BASE_SHA")) {
  errors.push(
    ".github/workflows/ci.yml: must pass GITHUB_BASE_SHA to commit message checks"
  );
}

if (errors.length > 0) {
  console.error("CI contract check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log("CI contract check passed.");
