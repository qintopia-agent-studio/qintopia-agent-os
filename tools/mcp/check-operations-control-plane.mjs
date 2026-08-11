#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { execFileSync } from "node:child_process";
import YAML from "yaml";

const repoRoot = process.cwd();
const packageRoot = path.join(repoRoot, "mcp/operations-control-plane");
const errors = [];

for (const file of [
  "README.md",
  "manifest.yaml",
  "bin/qintopia-operations-control-plane-mcp",
  "tests/test_operations_control_plane_mcp.py",
]) {
  if (!fs.existsSync(path.join(packageRoot, file))) {
    errors.push(`missing ${file}`);
  }
}

const readme = fs.readFileSync(path.join(packageRoot, "README.md"), "utf8");
for (const fragment of ["Production Boundary", "secrets", "Validation"]) {
  if (!readme.includes(fragment)) {
    errors.push(`README.md must mention ${fragment}`);
  }
}

const manifest = YAML.parse(
  fs.readFileSync(path.join(packageRoot, "manifest.yaml"), "utf8")
);
if (manifest.id !== "mcp/operations-control-plane") {
  errors.push("manifest id must be mcp/operations-control-plane");
}
if (manifest.type !== "mcp") {
  errors.push("manifest type must be mcp");
}
if (!manifest.production_boundary?.secrets) {
  errors.push("manifest secrets boundary must be true");
}

try {
  execFileSync(
    "python3",
    [
      "-m",
      "unittest",
      "discover",
      "-s",
      "mcp/operations-control-plane/tests",
      "-p",
      "test_*.py",
      "-v",
    ],
    {
      cwd: repoRoot,
      env: { ...process.env, PYTHONDONTWRITEBYTECODE: "1" },
      stdio: "inherit",
    }
  );
} catch {
  errors.push("operations-control-plane unittest failed");
}

if (errors.length > 0) {
  console.error("Operations control plane MCP check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log("Operations control plane MCP check passed.");
