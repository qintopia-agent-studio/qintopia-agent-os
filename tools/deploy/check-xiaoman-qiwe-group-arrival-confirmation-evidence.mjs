#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const args = process.argv.slice(2);
if (args.length !== 2) {
  fail(
    "usage: node tools/deploy/check-xiaoman-qiwe-group-arrival-confirmation-evidence.mjs <production-evidence-output.txt> <group-arrival-confirmation-output.txt>"
  );
}

const productionEvidenceFile = path.resolve(args[0]);
const confirmationEvidenceFile = path.resolve(args[1]);
const productionText = fs.readFileSync(productionEvidenceFile, "utf8");
const confirmationText = fs.readFileSync(confirmationEvidenceFile, "utf8");
const productionEvidenceLines = prefixedLines(
  productionText,
  "xiaoman_real_activity_production_evidence="
);
const confirmationEvidenceLines = prefixedLines(
  confirmationText,
  "xiaoman_qiwe_group_arrival_confirmation_evidence="
);
const allowedConfirmationTemplateLine =
  "Do not record QiWe token, GUID, API secret material, raw target group id, message id, request id, callback event id, file id, MD5 value, AES key, file size, filename, media URL, database URL, database credentials, raw chat content, screenshots containing member profiles, shell logs, or response bodies.";

const forbiddenPatterns = [
  /https?:\/\//i,
  /postgres(?:ql)?:\/\//i,
  /tenant_access_token/i,
  /base_token/i,
  /api[_-]?key/i,
  /\btoken\b/i,
  /QIWE_TOKEN/,
  /QIWE_GUID/,
  /DATABASE_URL/,
  /"target_group_id"\s*:/,
  /"artifact_uri"\s*:/,
  /"message_id"\s*:/,
  /"callback_event_id"\s*:/,
  /"request_id"\s*:/,
  /"requestId"\s*:/,
  /"fileAesKey"\s*:/,
  /"fileAeskey"\s*:/,
  /"file_aes_key"\s*:/,
  /"fileId"\s*:/,
  /"file_id"\s*:/,
  /"fileMd5"\s*:/,
  /"file_md5"\s*:/,
  /"fileSize"\s*:/,
  /"file_size"\s*:/,
  /"filename"\s*:/,
  /"fileName"\s*:/,
  /"raw_chat"\s*:/,
];

for (const [label, text] of [
  ["production evidence", productionText],
  ["group arrival confirmation evidence", stripAllowedTemplateLines(confirmationText)],
]) {
  for (const pattern of forbiddenPatterns) {
    if (pattern.test(text)) {
      fail(`${label} contains forbidden sensitive fragment: ${pattern}`);
    }
  }
}

const productionCheck = spawnSync(
  "node",
  [
    "tools/deploy/check-xiaoman-real-activity-production-evidence.mjs",
    productionEvidenceFile,
  ],
  { cwd: repoRoot, encoding: "utf8" }
);
if (productionCheck.status !== 0) {
  fail(
    `production real activity evidence failed: ${safeDiagnostic(productionCheck.stderr)}`
  );
}

const productionRecords = prefixedRecords(
  productionEvidenceLines,
  "xiaoman_real_activity_production_evidence="
);
const sendReady = singlePhase(productionRecords, "send_ready");
const qiweCallback = singlePhase(productionRecords, "qiwe_callback_send");
const retention = singlePhase(productionRecords, "sanitized_evidence_retention");
const confirmation = singleRecord(
  prefixedRecords(
    confirmationEvidenceLines,
    "xiaoman_qiwe_group_arrival_confirmation_evidence="
  ),
  "expected one QiWe group arrival confirmation evidence record"
);

assertExactKeys(
  confirmation,
  new Set([
    "schema",
    "success",
    "confirmation_status",
    "confirmation_method",
    "confirmed_by",
    "confirmed_at",
    "target_channel",
    "target_group_alias",
    "workflow_root_id",
    "send_ready_work_item_id",
    "generated_image_artifact_id",
    "artifact_content_hash",
    "external_send_executed",
    "raw_secret_fields_retained",
  ]),
  "QiWe group arrival confirmation"
);

if (
  confirmation.schema !== "xiaoman-qiwe-group-arrival-confirmation-evidence-v1" ||
  confirmation.success !== true ||
  confirmation.confirmation_status !== "confirmed" ||
  confirmation.confirmation_method !== "human_visible_group_check" ||
  !isSafeLabel(confirmation.confirmed_by) ||
  !isUtcSecondTimestamp(confirmation.confirmed_at) ||
  confirmation.target_channel !== "qiwe" ||
  confirmation.target_group_alias !== "community_activity_group" ||
  confirmation.workflow_root_id !== retention.workflow_root_id ||
  confirmation.send_ready_work_item_id !== sendReady.send_ready_work_item_id ||
  confirmation.send_ready_work_item_id !== qiweCallback.send_ready_work_item_id ||
  confirmation.send_ready_work_item_id !== retention.send_ready_work_item_id ||
  confirmation.generated_image_artifact_id !== sendReady.generated_image_artifact_id ||
  confirmation.generated_image_artifact_id !==
    qiweCallback.generated_image_artifact_id ||
  confirmation.generated_image_artifact_id !== retention.generated_image_artifact_id ||
  confirmation.artifact_content_hash !== sendReady.artifact_content_hash ||
  confirmation.artifact_content_hash !== qiweCallback.artifact_content_hash ||
  confirmation.artifact_content_hash !== retention.artifact_content_hash ||
  confirmation.external_send_executed !== true ||
  confirmation.raw_secret_fields_retained !== false
) {
  fail("QiWe group arrival confirmation does not bind to the real activity send");
}

console.log("Xiaoman QiWe group arrival confirmation evidence check passed.");

function prefixedLines(text, prefix) {
  return text.split(/\r?\n/).filter((line) => line.startsWith(prefix));
}

function stripAllowedTemplateLines(text) {
  return text
    .split(/\r?\n/)
    .filter((line) => line !== allowedConfirmationTemplateLine)
    .join("\n");
}

function prefixedRecords(lines, prefix) {
  return lines.map((line, index) => {
    let record;
    try {
      record = JSON.parse(line.slice(prefix.length));
    } catch (error) {
      fail(`evidence line ${index + 1} is not valid JSON: ${error.message}`);
    }
    if (!record || typeof record !== "object" || Array.isArray(record)) {
      fail(`evidence line ${index + 1} must be a JSON object`);
    }
    return record;
  });
}

function singleRecord(records, message) {
  if (records.length !== 1) {
    fail(message);
  }
  return records[0];
}

function singlePhase(records, phase) {
  return singleRecord(
    records.filter((record) => record.phase === phase),
    `expected one ${phase} evidence record`
  );
}

function assertExactKeys(entry, allowed, label) {
  if (!entry || typeof entry !== "object" || Array.isArray(entry)) {
    fail(`${label} must be a JSON object`);
  }
  for (const key of Object.keys(entry)) {
    if (!allowed.has(key)) {
      fail(`${label} includes unexpected key: ${key}`);
    }
  }
  for (const key of allowed) {
    if (!(key in entry)) {
      fail(`${label} is missing key: ${key}`);
    }
  }
}

function isSafeLabel(value) {
  return /^[a-z0-9][a-z0-9_-]{1,63}$/i.test(String(value ?? ""));
}

function isUtcSecondTimestamp(value) {
  if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/.test(String(value ?? ""))) {
    return false;
  }
  const parsed = new Date(value);
  return (
    !Number.isNaN(parsed.valueOf()) &&
    parsed.toISOString().replace(".000Z", "Z") === value
  );
}

function safeDiagnostic(text) {
  const firstLine =
    String(text ?? "")
      .split(/\r?\n/)
      .find(Boolean) ?? "no diagnostic";
  return firstLine.replace(/[^\w .:()/=-]/g, "").slice(0, 240);
}

function fail(message) {
  console.error(
    `Xiaoman QiWe group arrival confirmation evidence check failed: ${message}`
  );
  process.exit(1);
}
