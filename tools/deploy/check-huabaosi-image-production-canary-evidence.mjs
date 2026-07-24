#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const args = process.argv.slice(2);
if (args.length !== 1) {
  fail(
    "usage: node tools/deploy/check-huabaosi-image-production-canary-evidence.mjs <production-canary-output.txt>"
  );
}

const evidenceFile = path.resolve(args[0]);
const evidenceText = fs.readFileSync(evidenceFile, "utf8");
const prefix = "huabaosi_image_generation_production_canary_evidence=";
const completionLine =
  "Huabaosi production canary passed: one Feishu-backed JPEG remains pending human review; no generated-image approval, mirror, publish, QiWe, or send was executed";
const expectedPhases = [
  "preflight",
  "brief_review",
  "request_intake",
  "generation",
  "revalidation",
];
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
  /"artifact_uri"\s*:/,
  /"provider_response"\s*:/,
  /"raw_chat"\s*:/,
  /"message_id"\s*:/,
  /"target_group_id"\s*:/,
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
];

for (const pattern of forbiddenPatterns) {
  if (pattern.test(evidenceText)) {
    fail(`evidence contains forbidden sensitive fragment: ${pattern}`);
  }
}

const records = [];
let completionLineCount = 0;
for (const [index, line] of evidenceText.split(/\r?\n/).entries()) {
  if (line.trim() === "") {
    continue;
  }
  if (line === completionLine) {
    completionLineCount += 1;
    continue;
  }
  if (!line.startsWith(prefix)) {
    fail(`unexpected non-evidence line ${index + 1}`);
  }
  try {
    records.push(JSON.parse(line.slice(prefix.length)));
  } catch (error) {
    fail(`evidence line ${index + 1} is not valid JSON: ${error.message}`);
  }
}

if (completionLineCount !== 1) {
  fail("production canary evidence must include exactly one fixed completion line");
}
if (records.length !== expectedPhases.length) {
  fail("production canary evidence must contain exactly five fixed phase records");
}
for (const [index, phase] of expectedPhases.entries()) {
  if (records[index]?.phase !== phase) {
    fail(`unexpected production canary phase order at index ${index + 1}`);
  }
}

const [preflight, briefReview, requestIntake, generation, revalidation] = records;
for (const record of records) {
  if (
    record.success !== true ||
    record.artifact_profile !== "huabaosi-production" ||
    record.release_binary_verified !== true ||
    record.approved_sidecar_sha256_matched !== true ||
    record.approved_database_url_sha256_matched !== true ||
    !isGitSha(record.release_sha) ||
    !isSha256(record.sidecar_binary_sha256) ||
    !isSha256(record.database_url_sha256)
  ) {
    fail(`${record.phase} evidence does not satisfy the shared production boundary`);
  }
}
assertSame(records, "release_sha", "production release SHA");
assertSame(records, "artifact_profile", "production artifact profile");
assertSame(records, "sidecar_binary_sha256", "sidecar binary SHA-256");
assertSame(records, "database_url_sha256", "database URL SHA-256");

const commonKeys = [
  "approved_database_url_sha256_matched",
  "approved_sidecar_sha256_matched",
  "artifact_profile",
  "database_url_sha256",
  "phase",
  "release_binary_verified",
  "release_sha",
  "sidecar_binary_sha256",
  "success",
];

assertExactKeys(
  preflight,
  new Set([...commonKeys, "action_status", "timer_active", "timer_enabled"]),
  "preflight"
);
if (
  preflight.action_status !== "adapter_config_ready" ||
  preflight.timer_active !== false ||
  preflight.timer_enabled !== false
) {
  fail("production canary preflight does not prove adapter readiness");
}

assertExactKeys(
  briefReview,
  new Set([
    ...commonKeys,
    "action_status",
    "brief_artifact_id",
    "brief_work_item_id",
    "review_status",
    "reviewer_id",
  ]),
  "brief review"
);
if (
  briefReview.action_status !== "review_recorded" ||
  !isUuid(briefReview.brief_artifact_id) ||
  !isUuid(briefReview.brief_work_item_id) ||
  briefReview.review_status !== "approved" ||
  briefReview.reviewer_id !== "trainer"
) {
  fail("production canary brief review evidence is incomplete");
}

assertExactKeys(
  requestIntake,
  new Set([
    ...commonKeys,
    "action_status",
    "brief_artifact_id",
    "brief_work_item_id",
    "image_generation_work_item_id",
    "request_created",
  ]),
  "request intake"
);
if (
  requestIntake.action_status !== "image_generation_requests_created" ||
  requestIntake.brief_artifact_id !== briefReview.brief_artifact_id ||
  requestIntake.brief_work_item_id !== briefReview.brief_work_item_id ||
  !isUuid(requestIntake.image_generation_work_item_id) ||
  requestIntake.request_created !== true
) {
  fail("production canary request intake does not bind the approved brief");
}

assertExactKeys(
  generation,
  new Set([
    ...commonKeys,
    "action_status",
    "artifact_id",
    "byte_size",
    "content_hash",
    "height",
    "image_generation_work_item_id",
    "mime_type",
    "review_status",
    "storage_backend",
    "width",
  ]),
  "generation"
);
if (
  generation.action_status !== "generated_image_created" ||
  !isUuid(generation.artifact_id) ||
  !Number.isInteger(generation.byte_size) ||
  generation.byte_size <= 0 ||
  generation.byte_size > 10 * 1024 * 1024 ||
  !isCanonicalContentHash(generation.content_hash) ||
  generation.width !== 1024 ||
  generation.height !== 1024 ||
  generation.image_generation_work_item_id !==
    requestIntake.image_generation_work_item_id ||
  generation.mime_type !== "image/jpeg" ||
  generation.review_status !== "pending" ||
  generation.storage_backend !== "feishu-base"
) {
  fail("production canary generation does not prove one pending Feishu-backed JPEG");
}

assertExactKeys(
  revalidation,
  new Set([
    ...commonKeys,
    "action_status",
    "artifact_id",
    "byte_size",
    "content_hash",
    "database_writes_executed",
    "external_calls_executed",
    "height",
    "sensitive_fields_redacted",
    "width",
  ]),
  "revalidation"
);
if (
  revalidation.action_status !== "feishu_primary_storage_revalidated" ||
  revalidation.artifact_id !== generation.artifact_id ||
  revalidation.byte_size !== generation.byte_size ||
  revalidation.content_hash !== generation.content_hash ||
  revalidation.width !== generation.width ||
  revalidation.height !== generation.height ||
  revalidation.database_writes_executed !== false ||
  revalidation.external_calls_executed !== true ||
  revalidation.sensitive_fields_redacted !== true
) {
  fail(
    "production canary revalidation does not prove authenticated same-byte readback"
  );
}

console.log("Huabaosi image production canary evidence check passed.");

function assertExactKeys(entry, allowed, label) {
  for (const key of Object.keys(entry ?? {})) {
    if (!allowed.has(key)) {
      fail(`${label} evidence includes unexpected key: ${key}`);
    }
  }
  for (const key of allowed) {
    if (!(key in (entry ?? {}))) {
      fail(`${label} evidence is missing key: ${key}`);
    }
  }
}

function assertSame(records, key, label) {
  const [first] = records;
  for (const record of records) {
    if (record[key] !== first[key]) {
      fail(`${label} differs across evidence records`);
    }
  }
}

function isUuid(value) {
  return /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(
    String(value ?? "")
  );
}

function isGitSha(value) {
  return /^[0-9a-f]{40}$/.test(String(value ?? ""));
}

function isSha256(value) {
  return /^[0-9a-f]{64}$/.test(String(value ?? ""));
}

function isCanonicalContentHash(value) {
  return /^sha256:[0-9a-f]{64}$/.test(String(value ?? ""));
}

function fail(message) {
  console.error(`Huabaosi image production canary evidence check failed: ${message}`);
  process.exit(1);
}
