#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { execFileSync } from "node:child_process";

const repoRoot = process.cwd();
const errors = [];

const addError = (message) => {
  errors.push(message);
};

const exists = (relativePath) => fs.existsSync(path.join(repoRoot, relativePath));

const readText = (relativePath) =>
  fs.readFileSync(path.join(repoRoot, relativePath), "utf8");

const git = (args) =>
  execFileSync("git", args, {
    cwd: repoRoot,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  }).trim();

const walk = (relativeDir = ".") => {
  const root = path.join(repoRoot, relativeDir);
  const files = [];
  const skipDirs = new Set([
    ".git",
    "node_modules",
    "target",
    "dist",
    ".venv",
    "venv",
    "__pycache__",
  ]);

  const visit = (absoluteDir, relativeBase) => {
    for (const entry of fs.readdirSync(absoluteDir, { withFileTypes: true })) {
      if (skipDirs.has(entry.name)) {
        continue;
      }
      const relativePath = path.join(relativeBase, entry.name);
      const absolutePath = path.join(absoluteDir, entry.name);
      if (entry.isDirectory()) {
        visit(absolutePath, relativePath);
      } else if (entry.isFile()) {
        files.push(relativePath);
      }
    }
  };

  visit(root, relativeDir);
  return files.map((file) => file.replace(/^\.\//, "")).sort();
};

const currentBranch = (() => {
  try {
    return git(["rev-parse", "--abbrev-ref", "HEAD"]);
  } catch {
    return "";
  }
})();

const dirtyStatus = (() => {
  try {
    return git(["status", "--porcelain"]);
  } catch {
    return "";
  }
})();

if (!process.env.CI && currentBranch === "master" && dirtyStatus.trim().length > 0) {
  addError("local development must happen on a feature branch, not directly on master");
}

const forbiddenPathPatterns = [
  [/\.java$/i, "Java source is not an approved implementation language"],
  [/\.kt$/i, "Kotlin source is not an approved implementation language"],
  [/\.kts$/i, "Kotlin Gradle scripts are not approved"],
  [/\.go$/i, "Go source is not an approved implementation language"],
  [/\.cs$/i, "C# source is not an approved implementation language"],
  [/\.php$/i, "PHP source is not an approved implementation language"],
  [/\.rb$/i, "Ruby source is not an approved implementation language"],
  [/\.scala$/i, "Scala source is not an approved implementation language"],
  [/\.clj$/i, "Clojure source is not an approved implementation language"],
  [/\.exs?$/i, "Elixir source is not an approved implementation language"],
  [/\.swift$/i, "Swift source is not an approved implementation language"],
  [/(^|\/)pom\.xml$/i, "Maven is not an approved build system"],
  [/(^|\/)build\.gradle(\.kts)?$/i, "Gradle is not an approved build system"],
  [/(^|\/)settings\.gradle(\.kts)?$/i, "Gradle is not an approved build system"],
  [/(^|\/)gradlew(\.bat)?$/i, "Gradle wrapper is not approved"],
  [/(^|\/)mvnw(\.cmd)?$/i, "Maven wrapper is not approved"],
  [/(^|\/)go\.mod$/i, "Go modules are not approved"],
  [/(^|\/)go\.sum$/i, "Go modules are not approved"],
  [/(^|\/)Gemfile$/i, "Ruby Bundler is not approved"],
  [/(^|\/)composer\.json$/i, "PHP Composer is not approved"],
  [/(^|\/)mix\.exs$/i, "Elixir Mix is not approved"],
  [/(^|\/)Package\.swift$/i, "Swift Package Manager is not approved"],
];

for (const file of walk(".")) {
  for (const [pattern, message] of forbiddenPathPatterns) {
    if (pattern.test(file)) {
      addError(`${file}: ${message}`);
    }
  }
}

const forbiddenTopLevelDirs = [
  "python",
  "rust",
  "typescript",
  "javascript",
  "java",
  "golang",
  "go",
];
for (const dir of forbiddenTopLevelDirs) {
  if (fs.existsSync(path.join(repoRoot, dir))) {
    addError(`${dir}/: top-level language buckets are not allowed`);
  }
}

const requiredDocs = [
  "docs/plans/active/current-roadmap.md",
  "docs/plans/completed/monorepo-migration.md",
  "docs/engineering/programming-agent-guardrails.md",
  "docs/engineering/change-routing-index.md",
  "docs/engineering/collaboration-model.md",
  "CONTRIBUTING.md",
  "AGENTS.md",
  "CLAUDE.md",
];

for (const docPath of requiredDocs) {
  if (!exists(docPath)) {
    addError(`${docPath}: required collaboration document is missing`);
  }
}

const rootReadme = exists("README.md") ? readText("README.md") : "";
for (const requiredFragment of [
  "docs/plans/active/current-roadmap.md",
  "docs/plans/completed/monorepo-migration.md",
  "docs/engineering/programming-agent-guardrails.md",
  "docs/engineering/change-routing-index.md",
]) {
  if (rootReadme && !rootReadme.includes(requiredFragment)) {
    addError(`README.md: must link ${requiredFragment}`);
  }
}

const routingIndex = exists("docs/engineering/change-routing-index.md")
  ? readText("docs/engineering/change-routing-index.md")
  : "";
for (const requiredFragment of [
  "Change Erhua reply wording",
  "Change scheduled jobs",
  "Change WenYuanGe document lookup path",
  "Change Postgres table structure",
  "Directory Ownership",
  "Agent Entry Points",
]) {
  if (routingIndex && !routingIndex.includes(requiredFragment)) {
    addError(
      `docs/engineering/change-routing-index.md: must mention ${requiredFragment}`
    );
  }
}

const contributing = exists("CONTRIBUTING.md") ? readText("CONTRIBUTING.md") : "";
for (const requiredFragment of [
  "Create a branch from `master`",
  "Document first",
  "Do not introduce Java",
  "production-boundary",
  "Release Please",
  "Do not edit root `CHANGELOG.md`",
]) {
  if (contributing && !contributing.includes(requiredFragment)) {
    addError(`CONTRIBUTING.md: must mention ${requiredFragment}`);
  }
}

const agentInstructions = exists("AGENTS.md") ? readText("AGENTS.md") : "";
for (const requiredFragment of [
  "Do not develop directly on `master`",
  "Document first",
  "Do not introduce Java",
  "Release Please",
  "Do not manually edit root `CHANGELOG.md`",
  "docs/plans/active/current-roadmap.md",
  "docs/engineering/change-routing-index.md",
]) {
  if (agentInstructions && !agentInstructions.includes(requiredFragment)) {
    addError(`AGENTS.md: must mention ${requiredFragment}`);
  }
}

const guardrails = exists("docs/engineering/programming-agent-guardrails.md")
  ? readText("docs/engineering/programming-agent-guardrails.md")
  : "";
if (guardrails && !guardrails.includes("docs/engineering/change-routing-index.md")) {
  addError(
    "docs/engineering/programming-agent-guardrails.md: must link docs/engineering/change-routing-index.md"
  );
}

const collaborationModel = exists("docs/engineering/collaboration-model.md")
  ? readText("docs/engineering/collaboration-model.md")
  : "";
if (collaborationModel && !collaborationModel.includes("change-routing-index.md")) {
  addError(
    "docs/engineering/collaboration-model.md: must link change-routing-index.md"
  );
}

const packageJson = exists("package.json") ? JSON.parse(readText("package.json")) : {};
const scripts = packageJson.scripts ?? {};
if (!scripts["collaboration:check"]) {
  addError("package.json: missing collaboration:check script");
}
if (!scripts["check:light"]?.includes("pnpm collaboration:check")) {
  addError("package.json: check:light must include pnpm collaboration:check");
}

if (errors.length > 0) {
  console.error("Collaboration policy check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log("Collaboration policy check passed.");
