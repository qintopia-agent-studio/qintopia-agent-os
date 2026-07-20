#!/usr/bin/env node

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const checker = path.join(
  repoRoot,
  "tools/deploy/check-xiaoman-real-activity-production-evidence.mjs"
);
const tmpRoot = fs.mkdtempSync(
  path.join(os.tmpdir(), "xiaoman-real-activity-production-evidence-")
);
const releaseSha = "0123456789abcdef0123456789abcdef01234567";
const otherReleaseSha = "1111111111111111111111111111111111111111";
const databaseHash = "1".repeat(64);
const sidecarHash = "2".repeat(64);
const contentHash = `sha256:${"a".repeat(64)}`;
const otherContentHash = `sha256:${"b".repeat(64)}`;

try {
  const validEvidence = path.join(tmpRoot, "valid.txt");
  fs.writeFileSync(validEvidence, productionOutput(), "utf8");

  let result = runChecker(validEvidence);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /Xiaoman real activity production evidence check passed/);

  const hashMismatchEvidence = path.join(tmpRoot, "hash-mismatch.txt");
  fs.writeFileSync(
    hashMismatchEvidence,
    productionOutput({ qiweCallbackArtifactHash: otherContentHash }),
    "utf8"
  );
  result = runChecker(hashMismatchEvidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /QiWe callback evidence does not prove/);

  const rawSecretEvidence = path.join(tmpRoot, "raw-secret.txt");
  fs.writeFileSync(
    rawSecretEvidence,
    `${productionOutput()}\n{"artifact_uri":"https://media.example/private.jpg"}\n`,
    "utf8"
  );
  result = runChecker(rawSecretEvidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /forbidden sensitive fragment/);

  const missingPhaseEvidence = path.join(tmpRoot, "missing-phase.txt");
  fs.writeFileSync(
    missingPhaseEvidence,
    productionOutput()
      .split(/\r?\n/)
      .filter((line) => !line.includes('"phase":"human_approval"'))
      .join("\n"),
    "utf8"
  );
  result = runChecker(missingPhaseEvidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /expected exactly one human_approval evidence record/);

  const releaseDriftEvidence = path.join(tmpRoot, "release-drift.txt");
  fs.writeFileSync(
    releaseDriftEvidence,
    productionOutput({ retentionReleaseSha: otherReleaseSha }),
    "utf8"
  );
  result = runChecker(releaseDriftEvidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /production release SHA differs/);

  const unexpectedKeyEvidence = path.join(tmpRoot, "unexpected-key.txt");
  fs.writeFileSync(
    unexpectedKeyEvidence,
    productionOutput({ unexpectedRetentionKey: "raw-drift" }),
    "utf8"
  );
  result = runChecker(unexpectedKeyEvidence);
  assert.notEqual(result.status, 0);
  assert.match(
    result.stderr,
    /sanitized evidence retention evidence includes unexpected key/
  );

  const mutableBinaryEvidence = path.join(tmpRoot, "mutable-binary.txt");
  fs.writeFileSync(
    mutableBinaryEvidence,
    productionOutput({ releaseBinaryVerified: false }),
    "utf8"
  );
  result = runChecker(mutableBinaryEvidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /shared production boundary/);

  const extraPhaseEvidence = path.join(tmpRoot, "extra-phase.txt");
  fs.writeFileSync(
    extraPhaseEvidence,
    `${productionOutput()}xiaoman_real_activity_production_evidence=${JSON.stringify({
      phase: "unreviewed_extra_phase",
      success: true,
      worker: "unreviewed-worker",
      action_status: "unreviewed",
      production_release_sha: releaseSha,
      sidecar_binary_sha256: sidecarHash,
      database_url_sha256: databaseHash,
      release_binary_verified: true,
      approved_sidecar_sha256_matched: true,
      safe_for_chat: false,
    })}\n`,
    "utf8"
  );
  result = runChecker(extraPhaseEvidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /exactly seven fixed phase records/);
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Xiaoman real activity production evidence test passed.");

function runChecker(evidencePath) {
  return spawnSync("node", [checker, evidencePath], {
    cwd: repoRoot,
    encoding: "utf8",
  });
}

function productionOutput(overrides = {}) {
  const sourceEventSignalId = "11111111-2222-4333-8444-555555555555";
  const workflowRootId = "22222222-3333-4444-8555-666666666666";
  const imageWorkItemId = "33333333-4444-4555-8666-777777777777";
  const generatedImageArtifactId = "44444444-5555-4666-8777-888888888888";
  const sendReadyWorkItemId = "55555555-6666-4777-8888-999999999999";
  const common = {
    success: true,
    production_release_sha: releaseSha,
    sidecar_binary_sha256: sidecarHash,
    database_url_sha256: databaseHash,
    release_binary_verified: overrides.releaseBinaryVerified ?? true,
    approved_sidecar_sha256_matched: overrides.approvedSidecarSha256Matched ?? true,
    safe_for_chat: false,
  };
  const records = [
    {
      ...common,
      phase: "signal_intake",
      worker: "xiaoman-activity-signal-worker",
      action_status: "signal_ingest_submitted",
      apply_requested: true,
      dry_run: false,
      source_event_signal_id: sourceEventSignalId,
      workflow_root_id: workflowRootId,
      activity_phase: "pre_event",
      activity_route: "activity_promotion",
      external_send_executed: false,
    },
    {
      ...common,
      phase: "image_generation",
      worker: "huabaosi-image-generation-worker",
      action_status: "generated_image_created",
      apply_requested: true,
      dry_run: false,
      workflow_root_id: workflowRootId,
      image_generation_work_item_id: imageWorkItemId,
      generated_image_artifact_id: generatedImageArtifactId,
      artifact_content_hash: contentHash,
      artifact_type: "generated_image",
      review_status: "pending",
      storage_backend: "feishu-base",
      mime_type: "image/jpeg",
      width: 1024,
      height: 1024,
      byte_size: 123456,
      external_send_executed: false,
    },
    {
      ...common,
      phase: "human_approval",
      worker: "huabaosi-generated-image-review",
      action_status: "generated_image_approved",
      workflow_root_id: workflowRootId,
      generated_image_artifact_id: generatedImageArtifactId,
      artifact_content_hash: contentHash,
      artifact_type: "generated_image",
      previous_review_status: "pending",
      review_status: "approved",
      human_review_applied: true,
      feishu_revalidation_executed: true,
      external_send_executed: false,
    },
    {
      ...common,
      phase: "send_ready",
      worker: "operations-group-send-ready",
      action_status: "send_ready_recorded",
      workflow_root_id: workflowRootId,
      send_ready_work_item_id: sendReadyWorkItemId,
      generated_image_artifact_id: generatedImageArtifactId,
      artifact_content_hash: contentHash,
      target_channel: "qiwe",
      target_group_alias: "community_activity_group",
      review_policy: "human_final_confirmation",
      final_confirmation_recorded: true,
      external_send_executed: false,
    },
    {
      ...common,
      phase: "qiwe_upload",
      worker: "qiwe-image-send-adapter",
      action_status: "image_upload_accepted",
      send_ready_work_item_id: sendReadyWorkItemId,
      generated_image_artifact_id: generatedImageArtifactId,
      artifact_content_hash: contentHash,
      apply_requested: true,
      dry_run: false,
      external_upload_requested: true,
      callback_received: false,
      external_send_executed: false,
    },
    {
      ...common,
      phase: "qiwe_callback_send",
      worker: "qiwe-image-send-adapter",
      action_status: "image_send_completed",
      send_ready_work_item_id: sendReadyWorkItemId,
      generated_image_artifact_id: generatedImageArtifactId,
      artifact_content_hash: overrides.qiweCallbackArtifactHash ?? contentHash,
      apply_requested: true,
      dry_run: false,
      external_upload_requested: false,
      callback_received: true,
      callback_credential_schema: "fileAesKey+fileId+fileMd5+fileSize+filename",
      callback_additional_field_count: 0,
      external_send_executed: true,
    },
    {
      ...common,
      phase: "sanitized_evidence_retention",
      worker: "xiaoman-real-activity-production-evidence",
      action_status: "sanitized_evidence_retained",
      production_release_sha: overrides.retentionReleaseSha ?? releaseSha,
      source_event_signal_id: sourceEventSignalId,
      workflow_root_id: workflowRootId,
      send_ready_work_item_id: sendReadyWorkItemId,
      generated_image_artifact_id: generatedImageArtifactId,
      artifact_content_hash: contentHash,
      retained_report_schema: "xiaoman-real-activity-production-evidence-v1",
      raw_secret_fields_retained: false,
      external_send_executed: true,
    },
  ];
  if (overrides.unexpectedRetentionKey) {
    records[6].unexpectedRetentionKey = overrides.unexpectedRetentionKey;
  }
  return [
    ...records.map(
      (record) => `xiaoman_real_activity_production_evidence=${JSON.stringify(record)}`
    ),
    "Xiaoman real activity production evidence retained: signal intake, image generation, human approval, send-ready, QiWe group delivery, and sanitized evidence retention completed",
    "",
  ].join("\n");
}
