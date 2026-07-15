#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const prefix = "qiwe_image_send_staging_evidence=";
const args = process.argv.slice(2);
const mode = args.includes("--preflight-only") ? "preflight-only" : "complete";
const filePath = args.find((arg) => !arg.startsWith("--"));

const fail = (message) => {
  console.error(`QiWe staging evidence check failed: ${message}`);
  process.exit(1);
};

if (!filePath) {
  fail(
    "usage: node tools/deploy/check-qiwe-image-staging-evidence.mjs [--preflight-only] <evidence-output.txt>"
  );
}

const text = fs.readFileSync(path.resolve(filePath), "utf8");

const forbiddenPatterns = [
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
  /QIWE_TOKEN/,
  /QIWE_GUID/,
  /postgres:\/\//,
  /postgresql:\/\//,
];

for (const pattern of forbiddenPatterns) {
  if (pattern.test(text)) {
    fail(`forbidden sensitive fragment appeared in evidence: ${pattern}`);
  }
}

const parseEvidence = (line, index) => {
  try {
    return JSON.parse(line.slice(prefix.length));
  } catch (error) {
    fail(`evidence line ${index + 1} is not valid JSON: ${error.message}`);
  }
};

const entries = text
  .split(/\r?\n/)
  .filter((line) => line.startsWith(prefix))
  .map(parseEvidence);

if (entries.length === 0) {
  fail("no staging evidence lines found");
}

const allowedPreflightKeys = new Set([
  "action_status",
  "adapter_compiled",
  "allowed_group_count",
  "allowed_host_count",
  "config_valid",
  "database_boundary_valid",
  "media_allowed_host_count",
  "safe_for_chat",
  "send_enabled",
  "success",
  "webhook_ready",
  "worker",
]);

const allowedPhaseKeys = new Set([
  "action_status",
  "apply_requested",
  "callback_received",
  "dry_run",
  "external_send_executed",
  "external_upload_requested",
  "phase",
  "safe_for_chat",
  "success",
  "worker",
  "work_item_id",
]);

const allowedCallbackKeys = new Set([
  ...allowedPhaseKeys,
  "callback_additional_field_count",
  "callback_credential_schema",
]);

const allowedCredentialSchemas = new Set([
  "fileAesKey+fileId+fileMd5+fileSize+filename",
  "fileAeskey+fileId+fileMd5+fileSize+filename",
  "fileAesKey+fileId+fileMd5+fileSize+fileName",
  "fileAeskey+fileId+fileMd5+fileSize+fileName",
]);

const assertExactKeys = (entry, allowed, label) => {
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
};

const assertBase = (entry, label) => {
  if (
    entry.success !== true ||
    entry.worker !== "qiwe-image-send-adapter" ||
    entry.safe_for_chat !== false
  ) {
    fail(`${label} evidence has invalid base fields`);
  }
};

const preflights = [];
let upload = null;
let callback = null;

for (const entry of entries) {
  assertBase(entry, entry.phase ?? "preflight");
  if (!("phase" in entry)) {
    assertExactKeys(entry, allowedPreflightKeys, "preflight");
    if (
      entry.action_status !== "staging_adapter_ready" ||
      entry.adapter_compiled !== true ||
      entry.send_enabled !== true ||
      entry.config_valid !== true ||
      entry.database_boundary_valid !== true ||
      entry.webhook_ready !== true ||
      !Number.isInteger(entry.allowed_host_count) ||
      entry.allowed_host_count < 1 ||
      !Number.isInteger(entry.media_allowed_host_count) ||
      entry.media_allowed_host_count < 1 ||
      entry.allowed_group_count !== 1
    ) {
      fail("preflight evidence is not a ready staging boundary");
    }
    preflights.push(entry);
    continue;
  }

  if (entry.phase === "upload") {
    if (upload) {
      fail("multiple upload evidence records found");
    }
    assertExactKeys(entry, allowedPhaseKeys, "upload");
    if (
      entry.action_status !== "image_upload_accepted" ||
      entry.apply_requested !== true ||
      entry.dry_run !== false ||
      entry.external_upload_requested !== true ||
      entry.callback_received !== false ||
      entry.external_send_executed !== false ||
      !isUuid(entry.work_item_id)
    ) {
      fail("upload evidence is invalid");
    }
    upload = entry;
    continue;
  }

  if (entry.phase === "callback") {
    if (callback) {
      fail("multiple callback evidence records found");
    }
    assertExactKeys(entry, allowedCallbackKeys, "callback");
    if (
      entry.action_status !== "image_send_completed" ||
      entry.apply_requested !== true ||
      entry.dry_run !== false ||
      entry.external_upload_requested !== false ||
      entry.callback_received !== true ||
      entry.external_send_executed !== true ||
      !isUuid(entry.work_item_id) ||
      !allowedCredentialSchemas.has(entry.callback_credential_schema) ||
      !Number.isInteger(entry.callback_additional_field_count) ||
      entry.callback_additional_field_count < 0
    ) {
      fail("callback evidence is invalid");
    }
    callback = entry;
    continue;
  }

  fail(`unknown evidence phase: ${entry.phase}`);
}

if (mode === "preflight-only") {
  if (preflights.length !== 1 || upload || callback) {
    fail("preflight-only evidence must contain exactly one preflight record");
  }
} else {
  if (preflights.length < 1 || !upload || !callback) {
    fail("complete evidence requires preflight, upload, and callback records");
  }
  if (upload.work_item_id !== callback.work_item_id) {
    fail("upload and callback work_item_id values differ");
  }
}

console.log(`QiWe staging evidence check passed (${mode}).`);

function isUuid(value) {
  return /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(
    String(value ?? "")
  );
}
