#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { execFileSync } from "node:child_process";

const repoRoot = process.cwd();
const packageRoot = path.join(repoRoot, "skills/qintopia-tools");
const variants = ["erhua", "xiaoman", "wenyuange"];
const errors = [];

const exists = (relativePath) => fs.existsSync(path.join(packageRoot, relativePath));

const walk = (dir) => {
  const files = [];
  const visit = (absoluteDir) => {
    for (const entry of fs.readdirSync(absoluteDir, { withFileTypes: true })) {
      const absolutePath = path.join(absoluteDir, entry.name);
      if (entry.isDirectory()) {
        visit(absolutePath);
      } else if (entry.isFile()) {
        files.push(path.relative(packageRoot, absolutePath));
      }
    }
  };
  visit(dir);
  return files.sort();
};

for (const variant of variants) {
  for (const relativePath of [
    `variants/${variant}/README.md`,
    `variants/${variant}/plugin.yaml`,
    `variants/${variant}/__init__.py`,
    `variants/${variant}/tests/test_qintopia_tools.py`,
  ]) {
    if (!exists(relativePath)) {
      errors.push(`missing ${relativePath}`);
    }
  }

  try {
    execFileSync(
      "python3",
      [
        "-c",
        "import pathlib, sys; p=pathlib.Path(sys.argv[1]); compile(p.read_text(), str(p), 'exec')",
        `skills/qintopia-tools/variants/${variant}/__init__.py`,
      ],
      {
        cwd: repoRoot,
        stdio: ["ignore", "pipe", "pipe"],
      }
    );
  } catch (error) {
    errors.push(
      `py_compile failed for ${variant}: ${error.stderr?.toString() ?? error}`
    );
  }
}

for (const file of walk(packageRoot)) {
  if (file.includes("__pycache__/") || file.endsWith(".pyc")) {
    errors.push(`runtime cache file must not be committed: ${file}`);
  }

  if (file.endsWith(".env") || file.includes("/.env")) {
    errors.push(`env file must not be committed: ${file}`);
  }
}

if (errors.length > 0) {
  console.error("Qintopia tools check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log("Qintopia tools check passed.");
