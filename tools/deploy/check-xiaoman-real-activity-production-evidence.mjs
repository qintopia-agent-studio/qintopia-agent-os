#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const args = process.argv.slice(2);
if (args.length !== 1) {
  fail(
    "usage: node tools/deploy/check-xiaoman-real-activity-production-evidence.mjs <production-evidence-output.txt>"
  );
}

const evidenceFile = path.resolve(args[0]);
const evidenceText = fs.readFileSync(evidenceFile, "utf8");

const allowedCallbackSchemas = new Set([
  "fileAesKey+fileId+fileMd5+fileSize+filename",
  "fileAeskey+fileId+fileMd5+fileSize+fileName",
  "file_aes_key+file_id+file_md5+file_size+filename",
  "fileAesKey+fileId+fileMd5+fileSize+fileName",
]);

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
  /"requestId"\s*:/,
  /"request_id"\s*:/,
  /"callback_event_id"\s*:/,
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
  /"message_id"\s*:/,
  /"raw_chat"\s*:/,
  /"provider_response"\s*:/,
];

for (const pattern of forbiddenPatterns) {
  if (pattern.test(evidenceText)) {
    fail(`evidence contains forbidden sensitive fragment: ${pattern}`);
  }
}

const records = parsePrefixedLines(
  evidenceText,
  "xiaoman_real_activity_production_evidence=",
  new Set([
    "Xiaoman real activity production evidence retained: signal intake, image generation, human approval, send-ready, QiWe group delivery, and sanitized evidence retention completed",
  ])
);

const signal = singlePhase(records, "signal_intake");
const generation = singlePhase(records, "image_generation");
const approval = singlePhase(records, "human_approval");
const sendReady = singlePhase(records, "send_ready");
const qiweUpload = singlePhase(records, "qiwe_upload");
const qiweCallback = singlePhase(records, "qiwe_callback_send");
const retention = singlePhase(records, "sanitized_evidence_retention");

if (records.length !== 7) {
  fail("production evidence must contain exactly seven fixed phase records");
}

const commonKeys = [
  "phase",
  "success",
  "worker",
  "action_status",
  "production_release_sha",
  "sidecar_binary_sha256",
  "database_url_sha256",
  "release_binary_verified",
  "approved_sidecar_sha256_matched",
  "approved_database_url_sha256_matched",
  "safe_for_chat",
];

assertExactKeys(
  signal,
  new Set([
    ...commonKeys,
    "apply_requested",
    "dry_run",
    "source_event_signal_id",
    "workflow_root_id",
    "activity_phase",
    "activity_route",
    "external_send_executed",
  ]),
  "signal intake"
);
assertExactKeys(
  generation,
  new Set([
    ...commonKeys,
    "apply_requested",
    "dry_run",
    "workflow_root_id",
    "image_generation_work_item_id",
    "generated_image_artifact_id",
    "artifact_content_hash",
    "artifact_type",
    "review_status",
    "storage_backend",
    "mime_type",
    "width",
    "height",
    "byte_size",
    "external_send_executed",
  ]),
  "image generation"
);
assertExactKeys(
  approval,
  new Set([
    ...commonKeys,
    "workflow_root_id",
    "generated_image_artifact_id",
    "artifact_content_hash",
    "artifact_type",
    "previous_review_status",
    "review_status",
    "human_review_applied",
    "feishu_revalidation_executed",
    "external_send_executed",
  ]),
  "human approval"
);
assertExactKeys(
  sendReady,
  new Set([
    ...commonKeys,
    "workflow_root_id",
    "send_ready_work_item_id",
    "generated_image_artifact_id",
    "artifact_content_hash",
    "target_channel",
    "target_group_alias",
    "review_policy",
    "final_confirmation_recorded",
    "external_send_executed",
  ]),
  "send-ready"
);
assertExactKeys(
  qiweUpload,
  new Set([
    ...commonKeys,
    "send_ready_work_item_id",
    "generated_image_artifact_id",
    "artifact_content_hash",
    "apply_requested",
    "dry_run",
    "external_upload_requested",
    "callback_received",
    "external_send_executed",
  ]),
  "QiWe upload"
);
assertExactKeys(
  qiweCallback,
  new Set([
    ...commonKeys,
    "send_ready_work_item_id",
    "generated_image_artifact_id",
    "artifact_content_hash",
    "apply_requested",
    "dry_run",
    "external_upload_requested",
    "callback_received",
    "callback_credential_schema",
    "callback_additional_field_count",
    "external_send_executed",
  ]),
  "QiWe callback/send"
);
assertExactKeys(
  retention,
  new Set([
    ...commonKeys,
    "source_event_signal_id",
    "workflow_root_id",
    "send_ready_work_item_id",
    "generated_image_artifact_id",
    "artifact_content_hash",
    "retained_report_schema",
    "raw_secret_fields_retained",
    "external_send_executed",
  ]),
  "sanitized evidence retention"
);

for (const record of records) {
  if (
    record.success !== true ||
    record.safe_for_chat !== false ||
    !isGitSha(record.production_release_sha) ||
    !isSha256(record.sidecar_binary_sha256) ||
    !isSha256(record.database_url_sha256) ||
    record.release_binary_verified !== true ||
    record.approved_sidecar_sha256_matched !== true ||
    record.approved_database_url_sha256_matched !== true
  ) {
    fail(`${record.phase} evidence does not satisfy the shared production boundary`);
  }
}

assertSame(records, "production_release_sha", "production release SHA");
assertSame(records, "sidecar_binary_sha256", "sidecar binary SHA-256");
assertSame(records, "database_url_sha256", "database URL SHA-256");

if (
  signal.worker !== "xiaoman-activity-signal-worker" ||
  signal.action_status !== "signal_ingest_submitted" ||
  signal.apply_requested !== true ||
  signal.dry_run !== false ||
  !isUuid(signal.source_event_signal_id) ||
  !isUuid(signal.workflow_root_id) ||
  !["pre_event", "post_event"].includes(signal.activity_phase) ||
  !["activity_promotion", "activity_recap"].includes(signal.activity_route) ||
  signal.external_send_executed !== false
) {
  fail("signal intake evidence does not prove one real Xiaoman activity root");
}

if (
  generation.worker !== "huabaosi-image-generation-worker" ||
  generation.action_status !== "generated_image_created" ||
  generation.apply_requested !== true ||
  generation.dry_run !== false ||
  generation.workflow_root_id !== signal.workflow_root_id ||
  !isUuid(generation.image_generation_work_item_id) ||
  !isUuid(generation.generated_image_artifact_id) ||
  !isCanonicalContentHash(generation.artifact_content_hash) ||
  generation.artifact_type !== "generated_image" ||
  generation.review_status !== "pending" ||
  generation.storage_backend !== "feishu-base" ||
  generation.mime_type !== "image/jpeg" ||
  generation.width !== 1024 ||
  generation.height !== 1024 ||
  !Number.isInteger(generation.byte_size) ||
  generation.byte_size <= 0 ||
  generation.external_send_executed !== false
) {
  fail("image generation evidence does not prove one pending Feishu-backed JPEG");
}

if (
  approval.worker !== "huabaosi-generated-image-review" ||
  approval.action_status !== "generated_image_approved" ||
  approval.workflow_root_id !== signal.workflow_root_id ||
  approval.generated_image_artifact_id !== generation.generated_image_artifact_id ||
  approval.artifact_content_hash !== generation.artifact_content_hash ||
  approval.artifact_type !== "generated_image" ||
  approval.previous_review_status !== "pending" ||
  approval.review_status !== "approved" ||
  approval.human_review_applied !== true ||
  approval.feishu_revalidation_executed !== true ||
  approval.external_send_executed !== false
) {
  fail("human approval evidence does not prove reviewed generated-image approval");
}

if (
  sendReady.worker !== "operations-group-send-ready" ||
  sendReady.action_status !== "send_ready_recorded" ||
  sendReady.workflow_root_id !== signal.workflow_root_id ||
  !isUuid(sendReady.send_ready_work_item_id) ||
  sendReady.generated_image_artifact_id !== generation.generated_image_artifact_id ||
  sendReady.artifact_content_hash !== generation.artifact_content_hash ||
  sendReady.target_channel !== "qiwe" ||
  sendReady.target_group_alias !== "community_activity_group" ||
  sendReady.review_policy !== "human_final_confirmation" ||
  sendReady.final_confirmation_recorded !== true ||
  sendReady.external_send_executed !== false
) {
  fail("send-ready evidence does not prove human-final-confirmed QiWe request");
}

if (
  qiweUpload.worker !== "qiwe-image-send-adapter" ||
  qiweUpload.action_status !== "image_upload_accepted" ||
  qiweUpload.send_ready_work_item_id !== sendReady.send_ready_work_item_id ||
  qiweUpload.generated_image_artifact_id !== generation.generated_image_artifact_id ||
  qiweUpload.artifact_content_hash !== generation.artifact_content_hash ||
  qiweUpload.apply_requested !== true ||
  qiweUpload.dry_run !== false ||
  qiweUpload.external_upload_requested !== true ||
  qiweUpload.callback_received !== false ||
  qiweUpload.external_send_executed !== false
) {
  fail("QiWe upload evidence does not prove one accepted async upload");
}

if (
  qiweCallback.worker !== "qiwe-image-send-adapter" ||
  qiweCallback.action_status !== "image_send_completed" ||
  qiweCallback.send_ready_work_item_id !== sendReady.send_ready_work_item_id ||
  qiweCallback.generated_image_artifact_id !== generation.generated_image_artifact_id ||
  qiweCallback.artifact_content_hash !== generation.artifact_content_hash ||
  qiweCallback.apply_requested !== true ||
  qiweCallback.dry_run !== false ||
  qiweCallback.external_upload_requested !== false ||
  qiweCallback.callback_received !== true ||
  !allowedCallbackSchemas.has(qiweCallback.callback_credential_schema) ||
  !Number.isInteger(qiweCallback.callback_additional_field_count) ||
  qiweCallback.callback_additional_field_count < 0 ||
  qiweCallback.external_send_executed !== true
) {
  fail("QiWe callback evidence does not prove one completed group image send");
}

if (
  retention.worker !== "xiaoman-real-activity-production-evidence" ||
  retention.action_status !== "sanitized_evidence_retained" ||
  retention.source_event_signal_id !== signal.source_event_signal_id ||
  retention.workflow_root_id !== signal.workflow_root_id ||
  retention.send_ready_work_item_id !== sendReady.send_ready_work_item_id ||
  retention.generated_image_artifact_id !== generation.generated_image_artifact_id ||
  retention.artifact_content_hash !== generation.artifact_content_hash ||
  retention.retained_report_schema !== "xiaoman-real-activity-production-evidence-v1" ||
  retention.raw_secret_fields_retained !== false ||
  retention.external_send_executed !== true
) {
  fail("sanitized evidence retention does not bind the completed activity chain");
}

console.log("Xiaoman real activity production evidence check passed.");

function parsePrefixedLines(text, prefix, allowedLines) {
  const records = [];
  for (const [index, line] of text.split(/\r?\n/).entries()) {
    if (line.trim() === "" || allowedLines.has(line)) {
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
  if (records.length === 0) {
    fail("evidence has no records");
  }
  return records;
}

function singlePhase(records, phase) {
  return single(
    records.filter((record) => record.phase === phase),
    `expected exactly one ${phase} evidence record`
  );
}

function single(records, message) {
  if (records.length !== 1) {
    fail(message);
  }
  return records[0];
}

function assertExactKeys(entry, allowed, label) {
  for (const key of Object.keys(entry)) {
    if (!allowed.has(key)) {
      fail(`${label} evidence includes unexpected key: ${key}`);
    }
  }
  for (const key of allowed) {
    if (!(key in entry)) {
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
  console.error(`Xiaoman real activity production evidence check failed: ${message}`);
  process.exit(1);
}
