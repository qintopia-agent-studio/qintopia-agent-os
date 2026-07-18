#!/usr/bin/env node

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const checker = path.join(
  repoRoot,
  "tools/deploy/check-xiaoman-image-send-staging-evidence.mjs"
);
const tmpRoot = fs.mkdtempSync(
  path.join(os.tmpdir(), "xiaoman-image-send-staging-evidence-")
);
const contentHash =
  "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const otherContentHash =
  "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

try {
  const huabaosiEvidence = path.join(tmpRoot, "huabaosi.txt");
  const qiweEvidence = path.join(tmpRoot, "qiwe.txt");
  fs.writeFileSync(huabaosiEvidence, huabaosiOutput(contentHash), "utf8");
  fs.writeFileSync(qiweEvidence, qiweOutput(contentHash), "utf8");

  let result = runChecker(huabaosiEvidence, qiweEvidence);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /Xiaoman image-send staging evidence check passed/);

  const mismatchQiweEvidence = path.join(tmpRoot, "qiwe-mismatch.txt");
  fs.writeFileSync(mismatchQiweEvidence, qiweOutput(otherContentHash), "utf8");
  result = runChecker(huabaosiEvidence, mismatchQiweEvidence);
  assert.notEqual(result.status, 0);
  assert.match(
    result.stderr,
    /Huabaosi content_hash and QiWe artifact_content_hash values differ/
  );

  const rawHuabaosiEvidence = path.join(tmpRoot, "huabaosi-raw.txt");
  fs.writeFileSync(
    rawHuabaosiEvidence,
    `${huabaosiOutput(contentHash)}\n{"artifact_uri":"https://media.example/private.jpg"}\n`,
    "utf8"
  );
  result = runChecker(rawHuabaosiEvidence, qiweEvidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /forbidden sensitive fragment/);

  const rawQiweEvidence = path.join(tmpRoot, "qiwe-raw.txt");
  fs.writeFileSync(
    rawQiweEvidence,
    `${qiweOutput(contentHash)}\n{"requestId":"private-request-id"}\n`,
    "utf8"
  );
  result = runChecker(huabaosiEvidence, rawQiweEvidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /forbidden sensitive fragment/);

  const missingPreflightQiweEvidence = path.join(tmpRoot, "qiwe-no-preflight.txt");
  fs.writeFileSync(
    missingPreflightQiweEvidence,
    qiweOutput(contentHash)
      .split(/\r?\n/)
      .filter((line) => !line.includes('"action_status":"staging_adapter_ready"'))
      .join("\n"),
    "utf8"
  );
  result = runChecker(huabaosiEvidence, missingPreflightQiweEvidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /expected exactly one QiWe preflight evidence record/);

  const localStorageHuabaosiEvidence = path.join(tmpRoot, "huabaosi-local-storage.txt");
  fs.writeFileSync(
    localStorageHuabaosiEvidence,
    huabaosiOutput(contentHash, { storage_backend: "local" }),
    "utf8"
  );
  result = runChecker(localStorageHuabaosiEvidence, qiweEvidence);
  assert.notEqual(result.status, 0);
  assert.match(
    result.stderr,
    /Huabaosi preflight evidence does not prove staging adapter readiness/
  );

  const driftedDatabaseHuabaosiEvidence = path.join(tmpRoot, "huabaosi-db-drift.txt");
  fs.writeFileSync(
    driftedDatabaseHuabaosiEvidence,
    huabaosiOutput(contentHash, {
      generation_database_url_sha256:
        "3333333333333333333333333333333333333333333333333333333333333333",
    }),
    "utf8"
  );
  result = runChecker(driftedDatabaseHuabaosiEvidence, qiweEvidence);
  assert.notEqual(result.status, 0);
  assert.match(
    result.stderr,
    /Huabaosi generation evidence does not prove one pending final JPEG/
  );

  const unexpectedHuabaosiKeyEvidence = path.join(tmpRoot, "huabaosi-extra-key.txt");
  fs.writeFileSync(
    unexpectedHuabaosiKeyEvidence,
    huabaosiOutput(contentHash, { unexpected_generation_key: "unsafe-drift" }),
    "utf8"
  );
  result = runChecker(unexpectedHuabaosiKeyEvidence, qiweEvidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Huabaosi generation evidence includes unexpected key/);
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Xiaoman image-send staging evidence test passed.");

function runChecker(huabaosiEvidence, qiweEvidence) {
  return spawnSync("node", [checker, huabaosiEvidence, qiweEvidence], {
    cwd: repoRoot,
    encoding: "utf8",
  });
}

function huabaosiOutput(hash, overrides = {}) {
  const databaseHash =
    overrides.database_url_sha256 ??
    "1111111111111111111111111111111111111111111111111111111111111111";
  const preflight = {
    action_status: "adapter_config_ready",
    adapter_compiled: true,
    config_valid: true,
    database_url_sha256: databaseHash,
    generation_enabled: true,
    phase: "preflight",
    safe_for_chat: false,
    storage_backend: overrides.storage_backend ?? "feishu-base",
    success: true,
    worker: "huabaosi-image-generation-worker",
  };
  const generation = {
    action_status: "generated_image_created",
    apply_requested: true,
    artifact_count: 1,
    byte_size: 123456,
    content_hash: hash,
    database_url_sha256: overrides.generation_database_url_sha256 ?? databaseHash,
    dry_run: false,
    height: 1024,
    mime_type: "image/jpeg",
    phase: "generation",
    review_status: "pending",
    safe_for_chat: false,
    storage_backend: overrides.storage_backend ?? "feishu-base",
    success: true,
    width: 1024,
    work_item_id: "11111111-2222-4333-8444-555555555555",
    worker: "huabaosi-image-generation-worker",
  };
  if (overrides.unexpected_generation_key) {
    generation.unexpected_generation_key = overrides.unexpected_generation_key;
  }
  return [
    `huabaosi_image_generation_staging_evidence=${JSON.stringify(preflight)}`,
    `huabaosi_image_generation_staging_evidence=${JSON.stringify(generation)}`,
    "Huabaosi image staging smoke passed: one generated_image remains pending human review; Feishu Base stored the final JPEG; no QiWe or publish adapter was called",
    "",
  ].join("\n");
}

function qiweOutput(hash) {
  return [
    `qiwe_image_send_staging_evidence=${JSON.stringify({
      action_status: "staging_adapter_ready",
      adapter_compiled: true,
      allowed_group_count: 1,
      allowed_host_count: 1,
      config_valid: true,
      database_boundary_valid: true,
      media_allowed_host_count: 1,
      safe_for_chat: false,
      send_enabled: true,
      sidecar_binary_sha256:
        "2222222222222222222222222222222222222222222222222222222222222222",
      success: true,
      webhook_ready: true,
      worker: "qiwe-image-send-adapter",
    })}`,
    `qiwe_image_send_staging_evidence=${JSON.stringify({
      action_status: "image_upload_accepted",
      apply_requested: true,
      artifact_content_hash: hash,
      callback_received: false,
      dry_run: false,
      external_send_executed: false,
      external_upload_requested: true,
      phase: "upload",
      safe_for_chat: false,
      sidecar_binary_sha256:
        "2222222222222222222222222222222222222222222222222222222222222222",
      success: true,
      work_item_id: "77777777-8888-4999-aaaa-bbbbbbbbbbbb",
      worker: "qiwe-image-send-adapter",
    })}`,
    `qiwe_image_send_staging_evidence=${JSON.stringify({
      action_status: "image_send_completed",
      apply_requested: true,
      artifact_content_hash: hash,
      callback_additional_field_count: 0,
      callback_credential_schema: "fileAesKey+fileId+fileMd5+fileSize+filename",
      callback_received: true,
      dry_run: false,
      external_send_executed: true,
      external_upload_requested: false,
      phase: "callback",
      safe_for_chat: false,
      sidecar_binary_sha256:
        "2222222222222222222222222222222222222222222222222222222222222222",
      success: true,
      work_item_id: "77777777-8888-4999-aaaa-bbbbbbbbbbbb",
      worker: "qiwe-image-send-adapter",
    })}`,
    "QiWe image-send staging callback passed: one reviewed image send completed for the isolated allowlisted group",
    "",
  ].join("\n");
}
