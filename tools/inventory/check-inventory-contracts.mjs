#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const repoRoot = process.cwd();
const readmePath = "tools/inventory/README.md";
const absolute = path.join(repoRoot, readmePath);

if (!fs.existsSync(absolute)) {
  console.error(`${readmePath}: missing inventory tool contract`);
  process.exit(1);
}

const readme = fs.readFileSync(absolute, "utf8");
const required = ["read-only", "secrets", "deletion", "server mutation"];
const missing = required.filter((fragment) => !readme.includes(fragment));

if (missing.length > 0) {
  console.error(`${readmePath}: missing ${missing.join(", ")}`);
  process.exit(1);
}

console.log("Inventory contract check passed.");
