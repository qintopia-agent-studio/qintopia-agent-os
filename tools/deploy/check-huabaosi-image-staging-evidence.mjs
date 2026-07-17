#!/usr/bin/env node

import fs from "node:fs";
import process from "node:process";

const usage =
  "Usage: node tools/deploy/check-huabaosi-image-staging-evidence.mjs <evidence-output-file>";
const filePath = process.argv[2];

if (!filePath || process.argv.length !== 3) {
  console.error(usage);
  process.exit(2);
}

const text = fs.readFileSync(filePath, "utf8");
const prefix = "huabaosi_image_generation_staging_evidence=";
const allowedFixedLines = new Set([
  "Huabaosi image staging smoke passed: one generated_image remains pending human review; Feishu Base stored the final JPEG; no QiWe or publish adapter was called",
]);
const forbiddenPatterns = [
  /https?:\/\//i,
  /postgres:\/\//i,
  /tenant_access_token/i,
  /message_id/i,
  /raw_chat/i,
  /base_token/i,
  /api[_-]?key/i,
  /token/i,
  /artifact_uri/i,
  /filename/i,
  /file[A-Za-z]*(Key|Id|Md5|Size|Name)/,
];

const records = [];
for (const [index, line] of text.split(/\r?\n/).entries()) {
  if (line.trim() === "" || allowedFixedLines.has(line)) {
    continue;
  }
  if (!line.startsWith(prefix)) {
    throw new Error(`unexpected non-evidence line ${index + 1}`);
  }
  const payloadText = line.slice(prefix.length);
  for (const pattern of forbiddenPatterns) {
    if (pattern.test(payloadText)) {
      throw new Error(`evidence line ${index + 1} contains forbidden sensitive shape`);
    }
  }
  records.push(JSON.parse(payloadText));
}

if (records.length !== 2) {
  throw new Error(
    `expected exactly two Huabaosi staging evidence records, got ${records.length}`
  );
}

const phases = new Map(records.map((record) => [record.phase, record]));
const preflight = phases.get("preflight");
const generation = phases.get("generation");
if (!preflight || !generation) {
  throw new Error("expected one preflight and one generation evidence record");
}

const databaseHash = preflight.database_url_sha256;
if (
  !/^[0-9a-f]{64}$/.test(databaseHash) ||
  generation.database_url_sha256 !== databaseHash
) {
  throw new Error("staging database hash is missing or inconsistent");
}

const assertCommon = (record, expectedActionStatus) => {
  if (
    record.success !== true ||
    record.worker !== "huabaosi-image-generation-worker" ||
    record.action_status !== expectedActionStatus ||
    record.safe_for_chat !== false
  ) {
    throw new Error(`invalid ${record.phase} evidence`);
  }
};

assertCommon(preflight, "adapter_config_ready");
if (
  preflight.adapter_compiled !== true ||
  preflight.config_valid !== true ||
  preflight.generation_enabled !== true ||
  preflight.storage_backend !== "feishu-base"
) {
  throw new Error("preflight evidence does not prove adapter readiness");
}

assertCommon(generation, "generated_image_created");
if (
  generation.apply_requested !== true ||
  generation.dry_run !== false ||
  generation.artifact_count !== 1 ||
  generation.mime_type !== "image/jpeg" ||
  generation.review_status !== "pending" ||
  generation.width !== 1024 ||
  generation.height !== 1024 ||
  !Number.isInteger(generation.byte_size) ||
  generation.byte_size <= 0 ||
  generation.storage_backend !== "feishu-base" ||
  !/^sha256:[0-9a-f]{64}$/.test(generation.content_hash) ||
  !/^[0-9a-f-]{36}$/.test(generation.work_item_id)
) {
  throw new Error("generation evidence does not prove one pending final JPEG");
}

console.log("Huabaosi image staging evidence check passed.");
