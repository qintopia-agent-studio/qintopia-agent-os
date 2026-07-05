#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { execFileSync } from "node:child_process";

const repoRoot = process.cwd();
const packageRoot = path.join(repoRoot, "skills/feishu-base");
const errors = [];

const requiredFiles = [
  "README.md",
  "manifest.yaml",
  "plugin.yaml",
  "__init__.py",
  "docs/source-snapshot.md",
  "tests/test_feishu_base.py",
];

const forbiddenContentPatterns = [
  { pattern: /\bcli_[A-Za-z0-9_-]{20,}/, label: "hardcoded Feishu app id" },
  { pattern: /\b[A-Za-z0-9_-]{32,}#[A-Za-z0-9_-]{8,}/, label: "secret-like fallback" },
  { pattern: /\bbascn[A-Za-z0-9]+/, label: "hardcoded Base app token" },
  { pattern: /\btbl[A-Za-z0-9]+/, label: "hardcoded Base table id" },
  { pattern: /base_token"\s*:/, label: "response echoes Base token" },
  { pattern: /table_id"\s*:/, label: "response echoes table id" },
];

const addError = (message) => {
  errors.push(message);
};

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

for (const relativePath of requiredFiles) {
  if (!exists(relativePath)) {
    addError(`missing ${relativePath}`);
  }
}

if (exists("__init__.py")) {
  try {
    execFileSync(
      "python3",
      [
        "-c",
        "import pathlib, sys; p=pathlib.Path(sys.argv[1]); compile(p.read_text(), str(p), 'exec')",
        "skills/feishu-base/__init__.py",
      ],
      {
        cwd: repoRoot,
        stdio: ["ignore", "pipe", "pipe"],
      }
    );
  } catch (error) {
    addError(`py_compile failed: ${error.stderr?.toString() ?? error}`);
  }
}

try {
  execFileSync(
    "python3",
    ["-m", "unittest", "discover", "-s", "skills/feishu-base/tests", "-v"],
    {
      cwd: repoRoot,
      stdio: ["ignore", "pipe", "pipe"],
      env: { ...process.env, PYTHONDONTWRITEBYTECODE: "1" },
    }
  );
} catch (error) {
  addError(`unit tests failed: ${error.stderr?.toString() ?? error}`);
}

for (const file of fs.existsSync(packageRoot) ? walk(packageRoot) : []) {
  if (file.includes("__pycache__/") || file.endsWith(".pyc")) {
    addError(`runtime cache file must not be committed: ${file}`);
  }
  if (file.endsWith(".env") || file.includes("/.env")) {
    addError(`env file must not be committed: ${file}`);
  }

  const absolutePath = path.join(packageRoot, file);
  if ([".py", ".yaml", ".yml", ".md"].includes(path.extname(file))) {
    const content = fs.readFileSync(absolutePath, "utf8");
    for (const { pattern, label } of forbiddenContentPatterns) {
      if (pattern.test(content)) {
        addError(`${file}: ${label}`);
      }
    }
  }
}

if (errors.length > 0) {
  console.error("Feishu Base skill check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log("Feishu Base skill check passed.");
