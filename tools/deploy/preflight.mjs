#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { execFileSync } from "node:child_process";
import YAML from "yaml";

const repoRoot = process.cwd();
const args = new Set(process.argv.slice(2));
const ciMode = args.has("--ci") || process.env.CI === "true";
const errors = [];

const requiredScripts = [
  "format:check",
  "lint:md",
  "registry:check",
  "agents:check",
  "policy:check",
  "secrets:check",
  "deploy:preflight:ci",
  "artifact:sidecar",
  "artifact:prune:sidecar",
  "test:qiwe",
  "test:sidecar",
  "smoke:sidecar",
  "check",
];

const requiredDocs = [
  "docs/engineering/server-change-policy.md",
  "docs/engineering/ci-cd-gates.md",
  "deploy/sidecar/docs/monorepo-cutover-plan.md",
  "docs/operations/sidecar-ci-artifacts.md",
];

const requiredCheckFragments = [
  "pnpm format:check",
  "pnpm lint:md",
  "pnpm registry:check",
  "pnpm agents:check",
  "pnpm policy:check",
  "pnpm secrets:check",
  "pnpm deploy:preflight:ci",
  "pnpm test:qiwe",
  "pnpm test:sidecar",
  "pnpm smoke:sidecar",
];

const addError = (message) => {
  errors.push(message);
};

const exists = (relativePath) => fs.existsSync(path.join(repoRoot, relativePath));

const readText = (relativePath) =>
  fs.readFileSync(path.join(repoRoot, relativePath), "utf8");

const readYaml = (relativePath) => YAML.parse(readText(relativePath));

const git = (args) =>
  execFileSync("git", args, {
    cwd: repoRoot,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  }).trim();

const packageJson = JSON.parse(readText("package.json"));
const scripts = packageJson.scripts ?? {};

for (const scriptName of requiredScripts) {
  if (!scripts[scriptName]) {
    addError(`package.json: missing script ${scriptName}`);
  }
}

for (const fragment of requiredCheckFragments) {
  if (!scripts.check?.includes(fragment)) {
    addError(`package.json: check script must include '${fragment}'`);
  }
}

for (const docPath of requiredDocs) {
  if (!exists(docPath)) {
    addError(`${docPath}: required deploy gate document is missing`);
  }
}

const serverPolicy = exists("docs/engineering/server-change-policy.md")
  ? readText("docs/engineering/server-change-policy.md").toLowerCase()
  : "";
for (const phrase of [
  "approved commit sha",
  "smoke check",
  "rollback",
  "server is a deployment target",
  "scp",
]) {
  if (!serverPolicy.includes(phrase)) {
    addError(`docs/engineering/server-change-policy.md: must mention ${phrase}`);
  }
}

if (exists("deploy/sidecar/manifest.yaml")) {
  const deployManifest = readYaml("deploy/sidecar/manifest.yaml");
  if (!deployManifest.tags?.includes("legacy-snapshot")) {
    addError("deploy/sidecar/manifest.yaml: legacy deploy snapshot tag is required");
  }
  if (
    !deployManifest.validation?.commands?.some((command) => command.includes("pnpm"))
  ) {
    addError(
      "deploy/sidecar/manifest.yaml: validation commands must include pnpm gates"
    );
  }
}

const ciWorkflow = exists(".github/workflows/ci.yml")
  ? readText(".github/workflows/ci.yml")
  : "";
for (const phrase of [
  "sidecar-artifact",
  'NODE_VERSION: "24"',
  "pnpm/action-setup@v6",
  "actions/checkout@v7",
  "actions/setup-node@v6",
  "actions/setup-python@v6",
  "actions/upload-artifact@v7",
  "actions: write",
  "concurrency:",
  "cancel-in-progress: true",
  "github.event_name == 'push' && github.ref == 'refs/heads/master'",
  "node tools/deploy/prune-github-artifacts.mjs",
  "retention-days: 14",
  "qintopia-message-sidecar-linux-x86_64-gnu",
  "dtolnay/rust-toolchain@1.75.0",
  "components: rustfmt",
]) {
  if (!ciWorkflow.includes(phrase)) {
    addError(`.github/workflows/ci.yml: must include ${phrase}`);
  }
}

if (!ciMode) {
  let branch = "";
  try {
    branch = git(["branch", "--show-current"]);
  } catch {
    addError("git branch check failed");
  }
  if (branch !== "master") {
    addError(
      `deploy preflight must run from master; current branch is ${branch || "unknown"}`
    );
  }

  let status = "";
  try {
    status = git(["status", "--short"]);
  } catch {
    addError("git status check failed");
  }
  if (status) {
    addError("deploy preflight requires a clean worktree");
  }
}

if (errors.length > 0) {
  console.error(
    ciMode ? "Deploy preflight CI gate failed:" : "Deploy preflight failed:"
  );
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log(ciMode ? "Deploy preflight CI gate passed." : "Deploy preflight passed.");
