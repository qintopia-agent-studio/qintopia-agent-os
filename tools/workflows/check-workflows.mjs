#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import YAML from "yaml";

const repoRoot = process.cwd();
const errors = [];

const workflows = [
  "workflows/activity-promotion",
  "workflows/erhua-consultation",
  "workflows/xiaoman-activity-signal",
  "workflows/visual-asset-request",
  "workflows/silaoshi-daily-ops",
];

const fixtureDirs = ["fixtures/operations", "fixtures/qiwe", "fixtures/xiaoman"];

const exists = (relativePath) => fs.existsSync(path.join(repoRoot, relativePath));
const readText = (relativePath) =>
  fs.readFileSync(path.join(repoRoot, relativePath), "utf8");

const addError = (message) => {
  errors.push(message);
};

for (const workflowPath of workflows) {
  const readmePath = `${workflowPath}/README.md`;
  const manifestPath = `${workflowPath}/workflow.yaml`;
  if (!exists(readmePath)) {
    addError(`${workflowPath}: missing README.md`);
    continue;
  }
  if (!exists(manifestPath)) {
    addError(`${workflowPath}: missing workflow.yaml`);
    continue;
  }

  const readme = readText(readmePath);
  for (const fragment of [
    "Production Boundary",
    "Acceptance Scenarios",
    "Validation",
  ]) {
    if (!readme.includes(fragment)) {
      addError(`${readmePath}: must include ${fragment}`);
    }
  }

  const manifest = YAML.parse(readText(manifestPath));
  if (manifest.id !== workflowPath) {
    addError(`${manifestPath}: id must be ${workflowPath}`);
  }
  if (manifest.type !== "workflow") {
    addError(`${manifestPath}: type must be workflow`);
  }
  if (!manifest.validation?.commands?.length) {
    addError(`${manifestPath}: validation.commands is required`);
  }
}

for (const fixtureDir of fixtureDirs) {
  if (!exists(`${fixtureDir}/README.md`)) {
    addError(`${fixtureDir}: missing README.md`);
  }
  const absolute = path.join(repoRoot, fixtureDir);
  if (fs.existsSync(absolute)) {
    for (const file of fs.readdirSync(absolute)) {
      if (!file.endsWith(".json")) {
        continue;
      }
      try {
        const parsed = JSON.parse(readText(path.join(fixtureDir, file)));
        if (!parsed.case || !parsed.input || !parsed.expected) {
          addError(`${fixtureDir}/${file}: must include case, input, and expected`);
        }
      } catch (error) {
        addError(`${fixtureDir}/${file}: invalid JSON: ${error.message}`);
      }
    }
  }
}

if (errors.length > 0) {
  console.error("Workflow check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log("Workflow check passed.");
