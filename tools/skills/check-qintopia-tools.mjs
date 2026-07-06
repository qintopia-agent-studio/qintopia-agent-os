#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { execFileSync } from "node:child_process";

const repoRoot = process.cwd();
const packageRoot = path.join(repoRoot, "skills/qintopia-tools");
const variants = ["erhua", "xiaoman", "wenyuange"];
const requiredRegisteredTools = {
  erhua: [
    "qintopia_wenyuange_lookup",
    "qintopia_weather_lookup",
    "qintopia_daily_digest_publish",
  ],
  xiaoman: [
    "qintopia_wenyuange_lookup",
    "qintopia_weather_lookup",
    "qintopia_daily_digest_publish",
  ],
  wenyuange: ["qintopia_wenyuange_lookup"],
};
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

  try {
    execFileSync(
      "python3",
      [
        "-c",
        `
import importlib.util
import pathlib

plugin_path = pathlib.Path("skills/qintopia-tools/variants/${variant}/__init__.py").resolve()
module_name = "qintopia_tools_${variant}_check"
spec = importlib.util.spec_from_file_location(module_name, plugin_path)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

class Ctx:
    def __init__(self):
        self.names = []

    def register_tool(self, **kwargs):
        assert kwargs.get("name")
        assert kwargs.get("schema") is not None
        assert callable(kwargs.get("handler"))
        assert kwargs.get("description")
        self.names.append(kwargs["name"])

ctx = Ctx()
module.register(ctx)
required = set(${JSON.stringify(requiredRegisteredTools[variant])})
missing = sorted(required - set(ctx.names))
assert not missing, missing
`,
      ],
      {
        cwd: repoRoot,
        env: {
          ...process.env,
          PYTHONDONTWRITEBYTECODE: "1",
          QINTOPIA_PROFILE_ID: variant,
          QINTOPIA_AGENT_OS_MONOREPO_DIR: repoRoot,
        },
        stdio: ["ignore", "pipe", "pipe"],
      }
    );
  } catch (error) {
    errors.push(
      `import/register smoke failed for ${variant}: ${
        error.stderr?.toString() ?? error
      }`
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
