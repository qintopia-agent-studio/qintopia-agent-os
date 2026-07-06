#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import YAML from "yaml";

const repoRoot = process.cwd();
const errors = [];
const packages = ["mcp/feishu", "mcp/postgres"];

const exists = (relativePath) => fs.existsSync(path.join(repoRoot, relativePath));
const readText = (relativePath) =>
  fs.readFileSync(path.join(repoRoot, relativePath), "utf8");

const addError = (message) => errors.push(message);

for (const packagePath of packages) {
  const readmePath = `${packagePath}/README.md`;
  const manifestPath = `${packagePath}/manifest.yaml`;
  if (!exists(readmePath)) {
    addError(`${packagePath}: missing README.md`);
    continue;
  }
  if (!exists(manifestPath)) {
    addError(`${packagePath}: missing manifest.yaml`);
    continue;
  }

  const readme = readText(readmePath);
  for (const fragment of ["Production Boundary", "secrets", "Validation"]) {
    if (!readme.includes(fragment)) {
      addError(`${readmePath}: must mention ${fragment}`);
    }
  }

  const manifest = YAML.parse(readText(manifestPath));
  if (manifest.id !== packagePath) {
    addError(`${manifestPath}: id must be ${packagePath}`);
  }
  if (manifest.type !== "mcp") {
    addError(`${manifestPath}: type must be mcp`);
  }
  if (!manifest.production_boundary?.secrets) {
    addError(`${manifestPath}: secrets boundary must be true`);
  }
}

if (errors.length > 0) {
  console.error("MCP adapter check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log("MCP adapter check passed.");
