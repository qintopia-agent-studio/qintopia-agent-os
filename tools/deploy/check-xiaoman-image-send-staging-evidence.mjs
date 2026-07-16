#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const args = process.argv.slice(2);
if (args.length !== 2) {
  fail(
    "usage: node tools/deploy/check-xiaoman-image-send-staging-evidence.mjs <huabaosi-evidence-output.txt> <qiwe-evidence-output.txt>"
  );
}

const [huabaosiFile, qiweFile] = args.map((arg) => path.resolve(arg));
const huabaosiText = fs.readFileSync(huabaosiFile, "utf8");
const qiweText = fs.readFileSync(qiweFile, "utf8");

const forbiddenPatterns = [
  /https?:\/\//i,
  /postgres(?:ql)?:\/\//i,
  /tenant_access_token/i,
  /base_token/i,
  /api[_-]?key/i,
  /token/i,
  /QIWE_TOKEN/,
  /QIWE_GUID/,
  /"requestId"\s*:/,
  /"request_id"\s*:/,
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
  /"target_group_id"\s*:/,
  /"artifact_uri"\s*:/,
];

for (const [label, text] of [
  ["Huabaosi", huabaosiText],
  ["QiWe", qiweText],
]) {
  for (const pattern of forbiddenPatterns) {
    if (pattern.test(text)) {
      fail(`${label} evidence contains forbidden sensitive fragment: ${pattern}`);
    }
  }
}

const huabaosiRecords = parsePrefixedLines(
  huabaosiText,
  "huabaosi_image_generation_staging_evidence=",
  new Set([
    "Huabaosi image staging smoke passed: one generated_image remains pending human review; no Feishu, QiWe, or publish adapter was called",
  ]),
  "Huabaosi"
);
const qiweRecords = parsePrefixedLines(
  qiweText,
  "qiwe_image_send_staging_evidence=",
  new Set([
    "QiWe image-send staging preflight passed: configuration is ready; no work item was claimed and no external upload or send was executed",
    "QiWe image-send staging upload passed: awaiting one bounded owner-approved callback; no image send was executed",
    "QiWe image-send staging callback passed: one reviewed image send completed for the isolated allowlisted group",
  ]),
  "QiWe"
);

const huabaosiGeneration = single(
  huabaosiRecords.filter((record) => record.phase === "generation"),
  "expected exactly one Huabaosi generation evidence record"
);
const huabaosiPreflight = single(
  huabaosiRecords.filter((record) => record.phase === "preflight"),
  "expected exactly one Huabaosi preflight evidence record"
);
if (
  huabaosiPreflight.success !== true ||
  huabaosiPreflight.worker !== "huabaosi-image-generation-worker" ||
  huabaosiPreflight.action_status !== "adapter_config_ready" ||
  huabaosiPreflight.adapter_compiled !== true ||
  huabaosiPreflight.config_valid !== true ||
  huabaosiPreflight.generation_enabled !== true
) {
  fail("Huabaosi preflight evidence does not prove staging adapter readiness");
}
if (
  huabaosiGeneration.success !== true ||
  huabaosiGeneration.worker !== "huabaosi-image-generation-worker" ||
  huabaosiGeneration.action_status !== "generated_image_created" ||
  huabaosiGeneration.review_status !== "pending" ||
  huabaosiGeneration.mime_type !== "image/jpeg" ||
  !isCanonicalContentHash(huabaosiGeneration.content_hash)
) {
  fail("Huabaosi generation evidence does not prove one pending final JPEG");
}

const qiwePreflight = single(
  qiweRecords.filter((record) => !("phase" in record)),
  "expected exactly one QiWe preflight evidence record"
);
const qiweUpload = single(
  qiweRecords.filter((record) => record.phase === "upload"),
  "expected exactly one QiWe upload evidence record"
);
const qiweCallback = single(
  qiweRecords.filter((record) => record.phase === "callback"),
  "expected exactly one QiWe callback evidence record"
);
if (
  qiwePreflight.success !== true ||
  qiwePreflight.worker !== "qiwe-image-send-adapter" ||
  qiwePreflight.action_status !== "staging_adapter_ready" ||
  qiwePreflight.adapter_compiled !== true ||
  qiwePreflight.send_enabled !== true ||
  qiwePreflight.config_valid !== true ||
  qiwePreflight.database_boundary_valid !== true ||
  qiwePreflight.webhook_ready !== true ||
  qiwePreflight.allowed_group_count !== 1
) {
  fail("QiWe preflight evidence does not prove staging send readiness");
}
if (
  qiweUpload.success !== true ||
  qiweUpload.worker !== "qiwe-image-send-adapter" ||
  qiweUpload.action_status !== "image_upload_accepted" ||
  qiweUpload.external_upload_requested !== true ||
  qiweUpload.external_send_executed !== false ||
  !isCanonicalContentHash(qiweUpload.artifact_content_hash)
) {
  fail("QiWe upload evidence does not prove one accepted final JPEG upload");
}
if (
  qiweCallback.success !== true ||
  qiweCallback.worker !== "qiwe-image-send-adapter" ||
  qiweCallback.action_status !== "image_send_completed" ||
  qiweCallback.external_upload_requested !== false ||
  qiweCallback.external_send_executed !== true ||
  !isCanonicalContentHash(qiweCallback.artifact_content_hash)
) {
  fail("QiWe callback evidence does not prove one completed final JPEG send");
}
if (qiweUpload.work_item_id !== qiweCallback.work_item_id) {
  fail("QiWe upload and callback work_item_id values differ");
}
if (qiweUpload.artifact_content_hash !== qiweCallback.artifact_content_hash) {
  fail("QiWe upload and callback artifact_content_hash values differ");
}
if (huabaosiGeneration.content_hash !== qiweCallback.artifact_content_hash) {
  fail("Huabaosi content_hash and QiWe artifact_content_hash values differ");
}

console.log("Xiaoman image-send staging evidence check passed.");

function parsePrefixedLines(text, prefix, allowedLines, label) {
  const records = [];
  for (const [index, line] of text.split(/\r?\n/).entries()) {
    if (line.trim() === "" || allowedLines.has(line)) {
      continue;
    }
    if (!line.startsWith(prefix)) {
      fail(`${label} evidence has unexpected non-evidence line ${index + 1}`);
    }
    try {
      records.push(JSON.parse(line.slice(prefix.length)));
    } catch (error) {
      fail(`${label} evidence line ${index + 1} is not valid JSON: ${error.message}`);
    }
  }
  if (records.length === 0) {
    fail(`${label} evidence has no records`);
  }
  return records;
}

function single(records, message) {
  if (records.length !== 1) {
    fail(message);
  }
  return records[0];
}

function isCanonicalContentHash(value) {
  return /^sha256:[0-9a-f]{64}$/.test(String(value ?? ""));
}

function fail(message) {
  console.error(`Xiaoman image-send staging evidence check failed: ${message}`);
  process.exit(1);
}
