#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import YAML from "yaml";

const repoRoot = process.cwd();
const errors = [];

const packages = {
  "skills/qintopia-weather": {
    required: [
      "README.md",
      "manifest.yaml",
      "plugin.yaml",
      "__init__.py",
      "tests/test_qintopia_weather.py",
    ],
    fixtures: ["fixtures/weather"],
    requiredReadme: [
      "fixed Qintopia location",
      "arbitrary city",
      "mcp/weather-provider",
    ],
  },
  "skills/knowledge-retrieval": {
    required: [
      "README.md",
      "manifest.yaml",
      "plugin.yaml",
      "__init__.py",
      "tests/test_knowledge_retrieval.py",
    ],
    requiredReadme: [
      "WenYuanGe",
      "Dify",
      "filtered answer basis",
      "qintopia-tools registration shell",
    ],
  },
  "skills/postgres-context": {
    required: [
      "README.md",
      "manifest.yaml",
      "fixtures/member-context-lookup.json",
      "fixtures/answer-context-prepare.json",
      "fixtures/training-note-submit-allowed.json",
      "fixtures/training-note-submit-blocked.json",
    ],
    requiredReadme: [
      "read-only",
      "audit",
      "idempotency",
      "qintopia_erhua_training_note_submit",
    ],
  },
  "skills/operations-intake": {
    required: [
      "README.md",
      "manifest.yaml",
      "plugin.yaml",
      "__init__.py",
      "tests/test_operations_intake.py",
    ],
    requiredReadme: [
      "complaint intake",
      "sales handoff",
      "disclosure filtering",
      "qintopia-tools",
    ],
  },
};

const exists = (relativePath) => fs.existsSync(path.join(repoRoot, relativePath));
const readText = (relativePath) =>
  fs.readFileSync(path.join(repoRoot, relativePath), "utf8");

const addError = (message) => {
  errors.push(message);
};

const walkPackageFiles = (packagePath, options = {}) => {
  const files = [];
  const excludedDirectories = new Set(options.excludedDirectories ?? []);
  const stack = [packagePath];
  while (stack.length > 0) {
    const current = stack.pop();
    const absolute = path.join(repoRoot, current);
    if (!fs.existsSync(absolute)) {
      continue;
    }
    for (const entry of fs.readdirSync(absolute, { withFileTypes: true })) {
      const relative = path.join(current, entry.name);
      if (entry.isDirectory()) {
        if (!excludedDirectories.has(path.relative(packagePath, relative))) {
          stack.push(relative);
        }
      } else if (entry.isFile()) {
        files.push(relative);
      }
    }
  }
  return files.sort();
};

const assertNoForbiddenFiles = (packagePath) => {
  const forbiddenNames = new Set([
    ".env",
    ".env.local",
    "auth.json",
    "state.db",
    "state.db-shm",
    "state.db-wal",
    "session.json",
  ]);
  const stack = [packagePath];
  while (stack.length > 0) {
    const current = stack.pop();
    const absolute = path.join(repoRoot, current);
    if (!fs.existsSync(absolute)) {
      continue;
    }
    for (const entry of fs.readdirSync(absolute, { withFileTypes: true })) {
      const relative = path.join(current, entry.name);
      if (forbiddenNames.has(entry.name)) {
        addError(`${relative}: forbidden live state or secret-like file`);
      }
      if (entry.isDirectory()) {
        stack.push(relative);
      }
    }
  }
};

const assertExternalSendBoundary = (packagePath) => {
  const manifestPath = path.join(packagePath, "manifest.yaml");
  if (!exists(manifestPath)) {
    return;
  }

  const manifest = YAML.parse(readText(manifestPath));
  const currentSourceFiles = walkPackageFiles(packagePath, {
    excludedDirectories: ["docs/server-backups"],
  }).filter((file) =>
    [".py", ".js", ".mjs", ".ts", ".yaml", ".yml"].includes(path.extname(file))
  );
  const externalSendActionFiles = currentSourceFiles.filter((file) =>
    readText(file).includes("qiwe_send_direct_message")
  );
  if (
    externalSendActionFiles.length > 0 &&
    manifest.production_boundary?.external_sends !== true
  ) {
    addError(
      `${manifestPath}: production_boundary.external_sends must be true because current package files reference qiwe_send_direct_message (${externalSendActionFiles.join(", ")})`
    );
  }

  const pluginPath = path.join(packagePath, "plugin.yaml");
  if (exists(pluginPath)) {
    const plugin = YAML.parse(readText(pluginPath));
    if (
      plugin.production_boundary &&
      plugin.production_boundary.external_sends !==
        manifest.production_boundary?.external_sends
    ) {
      addError(
        `${pluginPath}: production_boundary.external_sends must match ${manifestPath}`
      );
    }
  }
};

for (const [packagePath, config] of Object.entries(packages)) {
  for (const required of config.required) {
    if (!exists(path.join(packagePath, required))) {
      addError(`${packagePath}: missing ${required}`);
    }
  }

  if (exists(path.join(packagePath, "manifest.yaml"))) {
    const manifest = YAML.parse(readText(path.join(packagePath, "manifest.yaml")));
    if (manifest.id !== packagePath) {
      addError(`${packagePath}/manifest.yaml: id must be ${packagePath}`);
    }
    if (!manifest.validation?.commands?.length) {
      addError(`${packagePath}/manifest.yaml: validation.commands is required`);
    }
  }

  if (exists(path.join(packagePath, "README.md"))) {
    const readme = readText(path.join(packagePath, "README.md"));
    for (const fragment of config.requiredReadme ?? []) {
      if (!readme.includes(fragment)) {
        addError(`${packagePath}/README.md: must mention ${fragment}`);
      }
    }
  }

  for (const fixtureDir of config.fixtures ?? []) {
    if (!exists(fixtureDir)) {
      addError(`${packagePath}: missing fixture directory ${fixtureDir}`);
    }
  }

  assertNoForbiddenFiles(packagePath);
  assertExternalSendBoundary(packagePath);
}

if (errors.length > 0) {
  console.error("Skill package boundary check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log("Skill package boundary check passed.");
