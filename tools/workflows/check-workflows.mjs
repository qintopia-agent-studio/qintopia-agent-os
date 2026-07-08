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
const xiaomanSignalFixtures = [
  "activity-signal.json",
  "duplicate-signal.json",
  "missing-fields-signal.json",
];
const xiaomanSignalExpectedFields = [
  "validation_status",
  "action_status",
  "capability_key",
  "work_item_type",
  "requester_agent",
  "target_agent",
  "idempotency_key",
  "review_needed",
  "missing_required_fields",
];

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

for (const file of xiaomanSignalFixtures) {
  const fixturePath = `fixtures/xiaoman/${file}`;
  if (!exists(fixturePath)) {
    addError(`${fixturePath}: missing Xiaoman signal replay fixture`);
    continue;
  }
  const parsed = JSON.parse(readText(fixturePath));
  if (parsed.input?.workflow !== "workflows/xiaoman-activity-signal") {
    addError(
      `${fixturePath}: input.workflow must be workflows/xiaoman-activity-signal`
    );
  }
  if (parsed.input?.operation !== "signal-ingest") {
    addError(`${fixturePath}: input.operation must be signal-ingest`);
  }
  if (parsed.input?.actor_agent !== "xiaoman") {
    addError(`${fixturePath}: input.actor_agent must be xiaoman`);
  }
  if (!parsed.input?.event_signal_id) {
    addError(`${fixturePath}: input.event_signal_id is required`);
  }
  for (const field of xiaomanSignalExpectedFields) {
    if (!(field in (parsed.expected ?? {}))) {
      addError(`${fixturePath}: expected.${field} is required`);
    }
  }
  const expectedIdempotencyKey = `xiaoman_activity_signal:${parsed.input?.event_signal_id}`;
  if (parsed.expected?.idempotency_key !== expectedIdempotencyKey) {
    addError(
      `${fixturePath}: expected.idempotency_key must match input.event_signal_id`
    );
  }
  if (parsed.expected?.capability_key !== "xiaoman.create_activity_request") {
    addError(
      `${fixturePath}: expected.capability_key must be xiaoman.create_activity_request`
    );
  }
  if (parsed.expected?.work_item_type !== "activity_promotion_request") {
    addError(
      `${fixturePath}: expected.work_item_type must be activity_promotion_request`
    );
  }
  if (
    parsed.expected?.requester_agent !== "default" ||
    parsed.expected?.target_agent !== "xiaoman"
  ) {
    addError(
      `${fixturePath}: expected routing must be requester_agent=default and target_agent=xiaoman`
    );
  }
  if (parsed.expected?.external_sends !== false) {
    addError(`${fixturePath}: expected.external_sends must be false`);
  }
  if (!Array.isArray(parsed.expected?.missing_required_fields)) {
    addError(`${fixturePath}: expected.missing_required_fields must be an array`);
  }
}

const duplicateSignal = JSON.parse(readText("fixtures/xiaoman/duplicate-signal.json"));
if (duplicateSignal.expected?.creates_duplicate_activity !== false) {
  addError(
    "fixtures/xiaoman/duplicate-signal.json: expected.creates_duplicate_activity must be false"
  );
}
const missingFieldsSignal = JSON.parse(
  readText("fixtures/xiaoman/missing-fields-signal.json")
);
if (missingFieldsSignal.expected?.action_status !== "review_needed") {
  addError(
    "fixtures/xiaoman/missing-fields-signal.json: expected.action_status must be review_needed"
  );
}
if (!missingFieldsSignal.expected?.missing_required_fields?.includes("signal_date")) {
  addError(
    "fixtures/xiaoman/missing-fields-signal.json: expected missing_required_fields must include signal_date"
  );
}

if (errors.length > 0) {
  console.error("Workflow check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log("Workflow check passed.");
