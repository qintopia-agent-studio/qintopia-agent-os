#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import YAML from "yaml";

const repoRoot = process.cwd();
const errors = [];

const exists = (relativePath) => fs.existsSync(path.join(repoRoot, relativePath));
const readText = (relativePath) =>
  fs.readFileSync(path.join(repoRoot, relativePath), "utf8");
const readYaml = (relativePath) => YAML.parse(readText(relativePath));

const addError = (message) => errors.push(message);

const requireFragment = (relativePath, text, fragment) => {
  if (!text.includes(fragment)) {
    addError(`${relativePath}: must include ${fragment}`);
  }
};

const normalizeWhitespace = (value) => value.replace(/\s+/g, " ").trim();

const forbidFragment = (relativePath, text, fragment) => {
  if (text.includes(fragment)) {
    addError(`${relativePath}: must not include ${fragment}`);
  }
};

const requireFile = (relativePath) => {
  if (!exists(relativePath)) {
    addError(`${relativePath}: missing required Xiaoman preflight artifact`);
    return "";
  }
  return readText(relativePath);
};

const workflowPath = "workflows/xiaoman-activity-signal/workflow.yaml";
if (exists(workflowPath)) {
  const workflow = readYaml(workflowPath);
  if (workflow.status !== "active") {
    addError(`${workflowPath}: status must be active for AgentOS-only preflight`);
  }
  if (workflow.production_boundary?.external_sends !== false) {
    addError(`${workflowPath}: production_boundary.external_sends must be false`);
  }
  if (!workflow.next_actions?.some((item) => item.includes("aggregate Xiaoman"))) {
    addError(`${workflowPath}: next_actions must keep aggregate preflight gate`);
  }
} else {
  addError(`${workflowPath}: missing Xiaoman workflow manifest`);
}

const registryPath = "registry/workflows.yaml";
if (exists(registryPath)) {
  const registry = readYaml(registryPath);
  const entry = registry.entries?.find(
    (candidate) => candidate.id === "workflows/xiaoman-activity-signal"
  );
  if (!entry) {
    addError(`${registryPath}: missing workflows/xiaoman-activity-signal entry`);
  } else {
    if (entry.status !== "active") {
      addError(`${registryPath}: Xiaoman workflow registry status must be active`);
    }
    if (
      !String(entry.notes ?? "").includes("production observation record still pending")
    ) {
      addError(
        `${registryPath}: Xiaoman notes must keep production observation pending`
      );
    }
  }
} else {
  addError(`${registryPath}: missing workflow registry`);
}

const readmePath = "workflows/xiaoman-activity-signal/README.md";
const readme = requireFile(readmePath);
const normalizedReadme = normalizeWhitespace(readme);
for (const fragment of [
  "event_signals -> activity request -> evidence/visual children -> internal artifacts -> awaiting-publish group message request",
  "deploy/smoke/docs/xiaoman-production-preflight-record.md",
  "does not approve Feishu writeback, QiWe sends, poster publishing",
  "xiaoman-activity-production-preflight-smoke.sh",
  "safe_for_chat=false",
]) {
  requireFragment(readmePath, normalizedReadme, fragment);
}

const renderPath = "deploy/sidecar/scripts/render-systemd-units.sh";
const render = requireFile(renderPath);
for (const fragment of [
  "run-xiaoman-activity-signal-worker --once --apply",
  "run-xiaoman-activity-promotion-starter-worker --once --apply",
  "run-evidence-worker --once --apply",
  "run-collaboration-worker --work-item-type visual_asset_request --once --apply",
  "run-xiaoman-activity-send-request-starter-worker --once --apply",
]) {
  requireFragment(renderPath, render, fragment);
}

const preflightPath =
  "deploy/sidecar/scripts/xiaoman-activity-production-preflight-smoke.sh";
const preflight = requireFile(preflightPath);
for (const fragment of [
  "QINTOPIA_XIAOMAN_ACTIVITY_PRODUCTION_PREFLIGHT_ENABLE",
  "xiaoman-activity-signal-timer-observation-smoke.sh",
  "xiaoman-activity-promotion-starter-timer-observation-smoke.sh",
  "operations-downstream-timers-observation-smoke.sh",
  "xiaoman-activity-downstream-observation-smoke.sh",
  "xiaoman-activity-send-request-starter-observation-smoke.sh",
]) {
  requireFragment(preflightPath, preflight, fragment);
}
for (const fragment of [
  "QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE=1",
  "server-deploy.sh",
  "gh release",
  "run-group-message-send-worker",
  "--use-feishu-base",
  "QIWE_TOKEN",
  "tenant_access_token",
]) {
  forbidFragment(preflightPath, preflight, fragment);
}

const recordPath = "deploy/smoke/docs/xiaoman-production-preflight-record.md";
const record = requireFile(recordPath);
for (const fragment of [
  "Commit SHA",
  "Run time",
  "Xiaoman signal timer",
  "Xiaoman promotion starter timer",
  "Operations evidence timer",
  "Operations visual timer",
  "Xiaoman send request starter timer",
  "Secret and external-send scan",
  "Queue Snapshot",
  "Pass: production observation can continue without enabling external adapters",
  "Hold: one or more timers, commands, previews, or boundary checks failed",
  "Passing this preflight does not approve publishing",
]) {
  requireFragment(recordPath, record, fragment);
}

const applySmokePath = "deploy/sidecar/scripts/operations-control-plane-apply-smoke.sh";
const applySmoke = requireFile(applySmokePath);
for (const fragment of [
  "run-xiaoman-activity-signal-worker --check-only",
  "run-xiaoman-activity-signal-worker --once --apply",
  "run-xiaoman-activity-promotion-starter-worker --check-only",
  "run-xiaoman-activity-promotion-starter-worker --once --apply",
  "run-evidence-worker --once",
  "run-collaboration-worker --work-item-type visual_asset_request --once",
  "operations-artifact-review-decision --apply",
  "run-xiaoman-activity-send-request-starter-worker --check-only",
  "run-xiaoman-activity-send-request-starter-worker --once --apply",
  "operations-group-message-confirm --apply",
  "run-group-message-send-worker --once",
  "xiaoman_send_did_not_send_or_queue",
  "xiaoman_group_send_ready_event_not_duplicated",
  "payload->>'send_executed' = 'false'",
]) {
  requireFragment(applySmokePath, applySmoke, fragment);
}

if (errors.length > 0) {
  console.error("Xiaoman preflight readiness check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log("Xiaoman preflight readiness check passed.");
