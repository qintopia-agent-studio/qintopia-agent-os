#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import YAML from "yaml";

const repoRoot = process.cwd();
const errors = [];

const addError = (message) => {
  errors.push(message);
};

const exists = (relativePath) => fs.existsSync(path.join(repoRoot, relativePath));
const readText = (relativePath) =>
  fs.readFileSync(path.join(repoRoot, relativePath), "utf8");
const readJson = (relativePath) => JSON.parse(readText(relativePath));
const readYaml = (relativePath) => YAML.parse(readText(relativePath));

const requiredFiles = [
  "skills/postgres-context/README.md",
  "skills/postgres-context/manifest.yaml",
  "skills/postgres-context/fixtures/member-context-lookup.json",
  "skills/postgres-context/fixtures/answer-context-prepare.json",
  "skills/postgres-context/fixtures/training-note-submit-allowed.json",
  "skills/postgres-context/fixtures/training-note-submit-blocked.json",
  "runtime/sidecar/src/context_tools.rs",
  "runtime/sidecar/src/context_mcp_server.rs",
  "runtime/postgres/migrations/202606240002_agent_os_data_layer.sql",
  "runtime/postgres/migrations/202606290006_erhua_training_memory.sql",
];

for (const file of requiredFiles) {
  if (!exists(file)) {
    addError(`${file}: required Postgres context contract file is missing`);
  }
}

if (exists("skills/postgres-context/manifest.yaml")) {
  const manifest = readYaml("skills/postgres-context/manifest.yaml");
  if (manifest.status !== "adopting") {
    addError("skills/postgres-context/manifest.yaml: status must be adopting");
  }
  if (manifest.risk_level !== "high") {
    addError("skills/postgres-context/manifest.yaml: risk_level must be high");
  }
  if (manifest.production_boundary?.database_writes !== true) {
    addError("skills/postgres-context/manifest.yaml: database_writes must be true");
  }
  for (const command of [
    "pnpm skills:postgres-context:check",
    "pnpm mcp:adapters:check",
    "pnpm test:sidecar",
  ]) {
    if (!manifest.validation?.commands?.includes(command)) {
      addError(
        `skills/postgres-context/manifest.yaml: validation.commands must include ${command}`
      );
    }
  }
}

const readme = exists("skills/postgres-context/README.md")
  ? readText("skills/postgres-context/README.md")
  : "";
for (const fragment of [
  "qintopia_member_context_lookup",
  "qintopia_answer_context_prepare",
  "qintopia_erhua_training_note_submit",
  "read-only",
  "audit",
  "idempotency",
  "QINTOPIA_ERHUA_TRAINER_USER_IDS",
  "qintopia_identity.member_context_audit",
  "qintopia_identity.erhua_training_notes",
  "Do not expose unrestricted SQL",
]) {
  if (readme && !readme.includes(fragment)) {
    addError(`skills/postgres-context/README.md: must mention ${fragment}`);
  }
}

const contextTools = exists("runtime/sidecar/src/context_tools.rs")
  ? readText("runtime/sidecar/src/context_tools.rs")
  : "";
for (const fragment of [
  'MEMBER_CONTEXT_LOOKUP_TOOL: &str = "qintopia_member_context_lookup"',
  'ANSWER_CONTEXT_PREPARE_TOOL: &str = "qintopia_answer_context_prepare"',
  'ERHUA_TRAINING_NOTE_SUBMIT_TOOL: &str = "qintopia_erhua_training_note_submit"',
  "write_member_context_audit(",
  "validate_context_caller(config, &caller)?",
  "is_erhua_trainer(config, &trainer_user_id)",
  "training_text_has_rejected_risk",
  "source_platform_message_id",
  'status: "rejected"',
  'status: "pending"',
  'status: "active"',
  "revoked_at IS NULL",
]) {
  if (contextTools && !contextTools.includes(fragment)) {
    addError(`runtime/sidecar/src/context_tools.rs: must include ${fragment}`);
  }
}

const dataLayerMigration = exists(
  "runtime/postgres/migrations/202606240002_agent_os_data_layer.sql"
)
  ? readText("runtime/postgres/migrations/202606240002_agent_os_data_layer.sql")
  : "";
for (const fragment of [
  "qintopia_identity.member_context_audit",
  "caller_profile text NOT NULL",
  "fields_returned jsonb NOT NULL",
  "redactions jsonb NOT NULL",
]) {
  if (dataLayerMigration && !dataLayerMigration.includes(fragment)) {
    addError(
      `runtime/postgres/migrations/202606240002_agent_os_data_layer.sql: must include ${fragment}`
    );
  }
}

const trainingMigration = exists(
  "runtime/postgres/migrations/202606290006_erhua_training_memory.sql"
)
  ? readText("runtime/postgres/migrations/202606290006_erhua_training_memory.sql")
  : "";
for (const fragment of [
  "qintopia_identity.erhua_training_notes",
  "qintopia_identity.erhua_persona_overlays",
  "source_platform_message_id text NOT NULL",
  "training_type IN ('member_preference', 'member_fact', 'reply_example', 'persona_rule')",
  "status IN ('pending', 'active', 'rejected', 'revoked')",
]) {
  if (trainingMigration && !trainingMigration.includes(fragment)) {
    addError(
      `runtime/postgres/migrations/202606290006_erhua_training_memory.sql: must include ${fragment}`
    );
  }
}

for (const fixturePath of [
  "skills/postgres-context/fixtures/member-context-lookup.json",
  "skills/postgres-context/fixtures/answer-context-prepare.json",
  "skills/postgres-context/fixtures/training-note-submit-allowed.json",
  "skills/postgres-context/fixtures/training-note-submit-blocked.json",
]) {
  if (!exists(fixturePath)) {
    continue;
  }
  const fixture = readJson(fixturePath);
  if (
    !fixture.tool ||
    !fixture.mode ||
    !fixture.request ||
    !fixture.expected_contract
  ) {
    addError(
      `${fixturePath}: fixture must include tool, mode, request, and expected_contract`
    );
  }
  const serialized = JSON.stringify(fixture);
  for (const forbidden of ["password", "secret", "token", "raw chat", "live .env"]) {
    if (serialized.toLowerCase().includes(forbidden)) {
      addError(`${fixturePath}: fixture must not contain ${forbidden}`);
    }
  }
}

if (errors.length > 0) {
  console.error("Postgres context check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log("Postgres context check passed.");
