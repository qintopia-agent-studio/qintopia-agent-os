#!/usr/bin/env node

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const checker = path.join(
  repoRoot,
  "tools/deploy/check-xiaoman-production-completion-evidence.mjs"
);
const tmpRoot = fs.mkdtempSync(
  path.join(os.tmpdir(), "xiaoman-production-completion-evidence-")
);
const releaseSha = "0123456789abcdef0123456789abcdef01234567";
const productionReleaseSha = "89abcdef012345670123456789abcdef01234567";
const sidecarHash = "1".repeat(64);
const productionSidecarHash = "2".repeat(64);
const stagingDatabaseHash = "3".repeat(64);
const productionDatabaseHash = "4".repeat(64);
const contentHash = `sha256:${"a".repeat(64)}`;

try {
  const files = writeEvidenceFiles();
  let result = runChecker(files);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /Xiaoman production completion evidence check passed/);

  const missingEnablement = writeEvidenceFiles({
    manifest: {
      qiwe_production_enablement: {
        status: "observation-only",
      },
    },
  });
  result = runChecker(missingEnablement);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /QiWe production enablement evidence is incomplete/);

  const testModeReadiness = writeEvidenceFiles({
    stagingRuntime: { test_mode: true },
  });
  result = runChecker(testModeReadiness);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /staging runtime readiness/);

  const productionDrift = writeEvidenceFiles({
    manifest: {
      huabaosi_production_activation: {
        sidecar_binary_sha256: "f".repeat(64),
      },
    },
  });
  result = runChecker(productionDrift);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /do not bind to real activity evidence/);

  const rawSecret = writeEvidenceFiles({
    manifest: {
      real_activity_confirmation: {
        confirmed_by: "owner-token",
      },
    },
  });
  result = runChecker(rawSecret);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /forbidden sensitive fragment/);
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Xiaoman production completion evidence test passed.");

function runChecker(files) {
  return spawnSync(
    "node",
    [
      checker,
      "--manifest",
      files.manifest,
      "--staging-runtime-readiness",
      files.stagingRuntime,
      "--huabaosi-staging",
      files.huabaosi,
      "--qiwe-staging",
      files.qiwe,
      "--production-real-activity",
      files.production,
    ],
    {
      cwd: repoRoot,
      encoding: "utf8",
    }
  );
}

function writeEvidenceFiles(overrides = {}) {
  const dir = fs.mkdtempSync(path.join(tmpRoot, "case-"));
  const files = {
    manifest: path.join(dir, "completion-manifest.json"),
    stagingRuntime: path.join(dir, "staging-runtime.txt"),
    huabaosi: path.join(dir, "huabaosi.txt"),
    qiwe: path.join(dir, "qiwe.txt"),
    production: path.join(dir, "production.txt"),
  };
  fs.writeFileSync(
    files.manifest,
    `${JSON.stringify(deepMerge(completionManifest(), overrides.manifest ?? {}))}\n`,
    "utf8"
  );
  fs.writeFileSync(
    files.stagingRuntime,
    `staging_runtime_readiness_evidence=${JSON.stringify(
      deepMerge(stagingRuntimeReadiness(), overrides.stagingRuntime ?? {})
    )}\n`,
    "utf8"
  );
  fs.writeFileSync(files.huabaosi, huabaosiStagingOutput(), "utf8");
  fs.writeFileSync(files.qiwe, qiweStagingOutput(), "utf8");
  fs.writeFileSync(files.production, productionOutput(), "utf8");
  return files;
}

function completionManifest() {
  return {
    schema: "xiaoman-production-completion-evidence-v1",
    release_please_validation: {
      status: "passed",
      pr_number: 180,
      head_sha: releaseSha,
      manual_ci_workflow: "ci.yml",
      release_please_status: "success",
    },
    qiwe_production_enablement: {
      status: "merged",
      pr_number: 215,
      head_sha: releaseSha,
      listener_service_timer_reviewed: true,
      observation_reviewed: true,
      rollback_reviewed: true,
      exact_allowlists_reviewed: true,
      production_feature_boundary_reviewed: true,
    },
    huabaosi_production_activation: {
      release_sha: productionReleaseSha,
      sidecar_binary_sha256: productionSidecarHash,
      database_url_sha256: productionDatabaseHash,
      image_generation_observation_passed: true,
      image_generation_activation_approved: true,
      feishu_mirror_observation_passed: true,
      feishu_mirror_activation_approved: true,
      first_record_evidence_retained: true,
    },
    real_activity_confirmation: {
      qiwe_group_arrival_confirmed: true,
      confirmed_by: "owner",
      confirmed_at: "2026-07-20T06:30:00Z",
    },
  };
}

function stagingRuntimeReadiness() {
  return {
    success: true,
    worker: "staging-runtime-readiness-evidence",
    action_status: "ready_for_huabaosi_qiwe_staging_smokes",
    test_mode: false,
    release_sha: releaseSha,
    packaged_sidecar_sha256: sidecarHash,
    staging_database_url_sha256: stagingDatabaseHash,
    reports: [
      { label: "prerequisite", success: true },
      { label: "huabaosi_readiness", success: true },
      { label: "qiwe_readiness", success: true },
    ],
    safe_for_review: true,
    limitations: [],
    guardrails: ["read-only readiness evidence aggregation"],
  };
}

function huabaosiStagingOutput() {
  return [
    {
      phase: "preflight",
      success: true,
      worker: "huabaosi-image-generation-worker",
      action_status: "adapter_config_ready",
      adapter_compiled: true,
      config_valid: true,
      database_url_sha256: stagingDatabaseHash,
      generation_enabled: true,
      safe_for_chat: false,
      sidecar_binary_sha256: sidecarHash,
      storage_backend: "feishu-base",
    },
    {
      phase: "generation",
      success: true,
      worker: "huabaosi-image-generation-worker",
      action_status: "generated_image_created",
      apply_requested: true,
      artifact_count: 1,
      byte_size: 123456,
      content_hash: contentHash,
      database_url_sha256: stagingDatabaseHash,
      dry_run: false,
      height: 1024,
      mime_type: "image/jpeg",
      review_status: "pending",
      safe_for_chat: false,
      sidecar_binary_sha256: sidecarHash,
      storage_backend: "feishu-base",
      width: 1024,
      work_item_id: "11111111-2222-4333-8444-555555555555",
    },
  ]
    .map(
      (record) => `huabaosi_image_generation_staging_evidence=${JSON.stringify(record)}`
    )
    .concat(
      "Huabaosi image staging smoke passed: one generated_image remains pending human review; Feishu Base stored the final JPEG; no QiWe or publish adapter was called",
      ""
    )
    .join("\n");
}

function qiweStagingOutput() {
  return [
    {
      success: true,
      worker: "qiwe-image-send-adapter",
      action_status: "staging_adapter_ready",
      adapter_compiled: true,
      feishu_delivery_bridge_compiled: true,
      allowed_group_count: 1,
      allowed_host_count: 1,
      config_valid: true,
      database_boundary_valid: true,
      media_allowed_host_count: 1,
      safe_for_chat: false,
      send_enabled: true,
      sidecar_binary_sha256: sidecarHash,
      webhook_ready: true,
    },
    {
      phase: "upload",
      success: true,
      worker: "qiwe-image-send-adapter",
      action_status: "image_upload_accepted",
      apply_requested: true,
      artifact_content_hash: contentHash,
      callback_received: false,
      dry_run: false,
      external_send_executed: false,
      external_upload_requested: true,
      safe_for_chat: false,
      sidecar_binary_sha256: sidecarHash,
      work_item_id: "22222222-3333-4444-8555-666666666666",
    },
    {
      phase: "callback",
      success: true,
      worker: "qiwe-image-send-adapter",
      action_status: "image_send_completed",
      apply_requested: true,
      artifact_content_hash: contentHash,
      callback_additional_field_count: 0,
      callback_credential_schema: "fileAesKey+fileId+fileMd5+fileSize+filename",
      callback_received: true,
      dry_run: false,
      external_send_executed: true,
      external_upload_requested: false,
      safe_for_chat: false,
      sidecar_binary_sha256: sidecarHash,
      work_item_id: "22222222-3333-4444-8555-666666666666",
    },
  ]
    .map((record) => `qiwe_image_send_staging_evidence=${JSON.stringify(record)}`)
    .concat(
      "QiWe image-send staging preflight passed: configuration is ready; no work item was claimed and no external upload or send was executed",
      "QiWe image-send staging upload passed: awaiting one bounded owner-approved callback; no image send was executed",
      "QiWe image-send staging callback passed: one reviewed image send completed for the isolated allowlisted group",
      ""
    )
    .join("\n");
}

function productionOutput() {
  const sourceEventSignalId = "33333333-4444-4555-8666-777777777777";
  const workflowRootId = "44444444-5555-4666-8777-888888888888";
  const imageWorkItemId = "55555555-6666-4777-8888-999999999999";
  const generatedImageArtifactId = "66666666-7777-4888-8999-aaaaaaaaaaaa";
  const sendReadyWorkItemId = "77777777-8888-4999-8aaa-bbbbbbbbbbbb";
  const common = {
    success: true,
    production_release_sha: productionReleaseSha,
    sidecar_binary_sha256: productionSidecarHash,
    database_url_sha256: productionDatabaseHash,
    release_binary_verified: true,
    approved_sidecar_sha256_matched: true,
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
      artifact_content_hash: contentHash,
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

function deepMerge(base, override) {
  if (!override || typeof override !== "object" || Array.isArray(override)) {
    return override ?? base;
  }
  const result = { ...base };
  for (const [key, value] of Object.entries(override)) {
    result[key] = deepMerge(result[key], value);
  }
  return result;
}
