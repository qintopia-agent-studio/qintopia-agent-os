#!/usr/bin/env node

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const checker = path.join(
  repoRoot,
  "tools/deploy/check-xiaoman-qiwe-group-arrival-confirmation-evidence.mjs"
);
const template = fs.readFileSync(
  path.join(
    repoRoot,
    "docs/reports/templates/xiaoman-qiwe-group-arrival-confirmation-evidence.md"
  ),
  "utf8"
);
const tmpRoot = fs.mkdtempSync(
  path.join(os.tmpdir(), "xiaoman-qiwe-arrival-confirmation-")
);
const releaseSha = "0123456789abcdef0123456789abcdef01234567";
const databaseHash = "1".repeat(64);
const sidecarHash = "2".repeat(64);
const contentHash = `sha256:${"a".repeat(64)}`;
const otherContentHash = `sha256:${"b".repeat(64)}`;

try {
  const valid = writeCase("valid");
  let result = runChecker(valid);
  assert.equal(result.status, 0, result.stderr);
  assert.match(
    result.stdout,
    /Xiaoman QiWe group arrival confirmation evidence check passed/
  );

  const templateText = writeCase("template-text", {
    confirmationReport: true,
  });
  result = runChecker(templateText);
  assert.equal(result.status, 0, result.stderr);

  const hashDrift = writeCase("hash-drift", {
    confirmation: { artifact_content_hash: otherContentHash },
  });
  result = runChecker(hashDrift);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /does not bind to the real activity send/);

  const notConfirmed = writeCase("not-confirmed", {
    confirmation: { confirmation_status: "pending" },
  });
  result = runChecker(notConfirmed);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /does not bind to the real activity send/);

  const leakedGroup = writeCase("leaked-group", {
    confirmation: { target_group_id: "raw-secret-group-id" },
  });
  result = runChecker(leakedGroup);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /forbidden sensitive fragment/);

  const leakedUrl = writeCase("leaked-url", {
    confirmation: { artifact_uri: "https://media.example/private.jpg" },
  });
  result = runChecker(leakedUrl);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /forbidden sensitive fragment/);

  const nonPrefixedLeak = writeCase("non-prefixed-leak");
  fs.appendFileSync(
    nonPrefixedLeak.confirmation,
    "operator note: https://media.example/private.jpg\n",
    "utf8"
  );
  result = runChecker(nonPrefixedLeak);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /forbidden sensitive fragment/);

  const nonPrefixedTokenLeak = writeCase("non-prefixed-token-leak");
  fs.appendFileSync(
    nonPrefixedTokenLeak.confirmation,
    "operator note: QiWe token was present before redaction\n",
    "utf8"
  );
  result = runChecker(nonPrefixedTokenLeak);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /forbidden sensitive fragment/);

  const missingRecord = writeCase("missing-record");
  fs.writeFileSync(missingRecord.confirmation, "\n", "utf8");
  result = runChecker(missingRecord);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /expected one QiWe group arrival confirmation/);

  const invalidProduction = writeCase("invalid-production", {
    production: { qiweCallbackArtifactHash: otherContentHash },
  });
  result = runChecker(invalidProduction);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /production real activity evidence failed/);
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Xiaoman QiWe group arrival confirmation evidence test passed.");

function runChecker(files) {
  return spawnSync("node", [checker, files.production, files.confirmation], {
    cwd: repoRoot,
    encoding: "utf8",
  });
}

function writeCase(name, overrides = {}) {
  const dir = fs.mkdtempSync(path.join(tmpRoot, `${name}-`));
  const files = {
    production: path.join(dir, "production.txt"),
    confirmation: path.join(dir, "confirmation.txt"),
  };
  fs.writeFileSync(
    files.production,
    productionOutput(overrides.production ?? {}),
    "utf8"
  );
  fs.writeFileSync(
    files.confirmation,
    overrides.confirmationReport
      ? confirmationTemplateOutput(overrides.confirmation ?? {})
      : confirmationOutput(overrides.confirmation ?? {}),
    "utf8"
  );
  return files;
}

function confirmationTemplateOutput(overrides = {}) {
  const filledRecord = confirmationOutput(overrides).trim();
  return template.replace(
    /^xiaoman_qiwe_group_arrival_confirmation_evidence=.*$/m,
    filledRecord
  );
}

function confirmationOutput(overrides = {}) {
  return [
    `xiaoman_qiwe_group_arrival_confirmation_evidence=${JSON.stringify({
      schema: "xiaoman-qiwe-group-arrival-confirmation-evidence-v1",
      success: true,
      confirmation_status: "confirmed",
      confirmation_method: "human_visible_group_check",
      confirmed_by: "owner",
      confirmed_at: "2026-07-20T06:30:00Z",
      target_channel: "qiwe",
      target_group_alias: "community_activity_group",
      workflow_root_id: "22222222-3333-4444-8555-666666666666",
      send_ready_work_item_id: "55555555-6666-4777-8888-999999999999",
      generated_image_artifact_id: "44444444-5555-4666-8777-888888888888",
      artifact_content_hash: contentHash,
      external_send_executed: true,
      raw_secret_fields_retained: false,
      ...overrides,
    })}`,
    "",
  ].join("\n");
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
    runtime_artifact_profile: overrides.runtimeArtifactProfile ?? "qiwe-production",
    sidecar_binary_sha256: sidecarHash,
    database_url_sha256: databaseHash,
    release_binary_verified: true,
    approved_sidecar_sha256_matched: true,
    approved_database_url_sha256_matched: true,
    safe_for_chat: false,
  };
  return [
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
      source_event_signal_id: sourceEventSignalId,
      workflow_root_id: workflowRootId,
      send_ready_work_item_id: sendReadyWorkItemId,
      generated_image_artifact_id: generatedImageArtifactId,
      artifact_content_hash: contentHash,
      retained_report_schema: "xiaoman-real-activity-production-evidence-v1",
      raw_secret_fields_retained: false,
      external_send_executed: true,
    },
  ]
    .map(
      (record) => `xiaoman_real_activity_production_evidence=${JSON.stringify(record)}`
    )
    .concat(
      "Xiaoman real activity production evidence retained: signal intake, image generation, human approval, send-ready, QiWe group delivery, and sanitized evidence retention completed",
      ""
    )
    .join("\n");
}
