#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { execFileSync } from "node:child_process";

const repoRoot = process.cwd();
const errors = [];

const workerUnits = [
  ["qintopia-agentos-member-profile-worker.service", "run-member-profile-worker"],
  ["qintopia-agentos-graph-projection-worker.service", "run-graph-projection-worker"],
  ["qintopia-agentos-raw-archive-worker.service", "run-raw-archive-worker"],
  ["qintopia-agentos-event-signal-worker.service", "run-event-signal-worker"],
  ["qintopia-agentos-daily-digest-worker.service", "agentos-daily-digest-worker"],
  [
    "qintopia-agentos-daily-digest-publisher.service",
    "run-daily-digest-publisher-worker",
  ],
];

const requiredDocs = [
  "deploy/sidecar/docs/m9f-legacy-reference-removal.md",
  "deploy/sidecar/docs/systemd-cutover-plan.md",
  "docs/operations/m9-server-cutover-runbook.md",
  "docs/operations/server-directory-plan.md",
];

const readText = (relativePath) =>
  fs.readFileSync(path.join(repoRoot, relativePath), "utf8");

const exists = (relativePath) => fs.existsSync(path.join(repoRoot, relativePath));

const addError = (message) => {
  errors.push(message);
};

for (const docPath of requiredDocs) {
  if (!exists(docPath)) {
    addError(`${docPath}: required release/current model document is missing`);
  }
}

const packageJson = JSON.parse(readText("package.json"));
if (!packageJson.scripts?.["deploy:release-model:check"]) {
  addError("package.json: missing deploy:release-model:check script");
}
if (
  !packageJson.scripts?.["check:light"]?.includes("pnpm deploy:release-model:check")
) {
  addError(
    "package.json: check:light script must include pnpm deploy:release-model:check"
  );
}
if (!packageJson.scripts?.check?.includes("pnpm check:light")) {
  addError("package.json: check script must include pnpm check:light");
}

const wrapperPath = "deploy/sidecar/scripts/hermes/qintopia-context-mcp";
if (!exists(wrapperPath)) {
  addError(`${wrapperPath}: missing Hermes MCP wrapper`);
} else {
  const wrapper = readText(wrapperPath);
  for (const requiredFragment of [
    "QINTOPIA_DEPLOYED_COMMIT_SHA",
    "QINTOPIA_SIDECAR_BIN",
    "/home/ubuntu/qintopia-agent-os-artifacts",
    "/home/ubuntu/qintopia-agent-os-releases/current",
    'exec "$BIN" mcp-context',
  ]) {
    if (!wrapper.includes(requiredFragment)) {
      addError(`${wrapperPath}: missing ${requiredFragment}`);
    }
  }
  if (wrapper.includes("/home/ubuntu/qintopia-msg-sidecar")) {
    addError(`${wrapperPath}: must not default to the legacy standalone checkout`);
  }
}

const renderedDir = fs.mkdtempSync(path.join(os.tmpdir(), "qintopia-m9f-"));
try {
  execFileSync(
    "deploy/sidecar/scripts/render-systemd-units.sh",
    [
      "--target-sha",
      "m9f-check",
      "--artifact-dir",
      "/home/ubuntu/qintopia-agent-os-releases/current/sidecar",
      "--monorepo-dir",
      "/home/ubuntu/qintopia-agent-os-releases/current",
      "--migrations-dir",
      "/home/ubuntu/qintopia-agent-os-releases/current/runtime/postgres/migrations",
      "--output-dir",
      renderedDir,
    ],
    {
      cwd: repoRoot,
      stdio: ["ignore", "pipe", "pipe"],
    }
  );

  for (const [unitName, command] of workerUnits) {
    const unitPath = path.join(renderedDir, unitName);
    if (!fs.existsSync(unitPath)) {
      addError(`rendered systemd output: missing ${unitName}`);
      continue;
    }
    const unit = fs.readFileSync(unitPath, "utf8");
    for (const requiredFragment of [
      "WorkingDirectory=/home/ubuntu/qintopia-agent-os-releases/current",
      "ExecStart=/home/ubuntu/qintopia-agent-os-releases/current/sidecar/qintopia-message-sidecar",
      command,
      "Environment=QINTOPIA_SIDECAR_MIGRATIONS_DIR=/home/ubuntu/qintopia-agent-os-releases/current/runtime/postgres/migrations",
      "Environment=QINTOPIA_DEPLOYED_COMMIT_SHA=m9f-check",
    ]) {
      if (!unit.includes(requiredFragment)) {
        addError(`${unitName}: missing ${requiredFragment}`);
      }
    }
    if (unit.includes("/home/ubuntu/qintopia-msg-sidecar")) {
      addError(`${unitName}: still references legacy standalone checkout`);
    }
  }
} catch (error) {
  addError(`render-systemd-units.sh failed: ${error.message}`);
} finally {
  fs.rmSync(renderedDir, { recursive: true, force: true });
}

for (const docPath of requiredDocs.filter(exists)) {
  const doc = readText(docPath);
  for (const [unitName] of workerUnits) {
    if (!doc.includes(unitName)) {
      addError(`${docPath}: must mention ${unitName}`);
    }
  }
}

const m9fDoc = exists("deploy/sidecar/docs/m9f-legacy-reference-removal.md")
  ? readText("deploy/sidecar/docs/m9f-legacy-reference-removal.md")
  : "";
for (const requiredFragment of [
  "/home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/hermes/qintopia-context-mcp",
  "deploy-bundle",
  "QINTOPIA_DEPLOYED_COMMIT_SHA",
  "Do not enable operations timers",
  "Do not enable real external send",
  "archive",
  "rollback",
]) {
  if (m9fDoc && !m9fDoc.includes(requiredFragment)) {
    addError(
      `deploy/sidecar/docs/m9f-legacy-reference-removal.md: missing ${requiredFragment}`
    );
  }
}

if (errors.length > 0) {
  console.error("Release/current model check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log("Release/current model check passed.");
