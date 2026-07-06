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
  ]) {
    if (!readme.includes(fragment)) {
      errors.push(`${readmePath}: must mention ${fragment}`);
    }
  }
}

const packageJson = JSON.parse(readText(packagePath));
for (const scriptName of ["check:light", "registry:check", "secrets:check"]) {
  if (!packageJson.scripts?.[scriptName]) {
    errors.push(`${packagePath}: missing ${scriptName}`);
  }
}

if (errors.length > 0) {
  console.error("CI contract check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log("CI contract check passed.");
