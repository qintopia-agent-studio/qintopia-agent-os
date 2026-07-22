#!/usr/bin/env node

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const builder = path.join(
  repoRoot,
  "tools/deploy/build-xiaoman-production-completion-manifest.mjs"
);
const checker = path.join(
  repoRoot,
  "tools/deploy/check-xiaoman-production-completion-evidence.mjs"
);
const tmpRoot = fs.mkdtempSync(
  path.join(os.tmpdir(), "xiaoman-production-completion-manifest-")
);
const fakeBin = path.join(tmpRoot, "bin");
const stagingReleaseSha = "0123456789abcdef0123456789abcdef01234567";
const productionReleaseSha = "89abcdef012345670123456789abcdef01234567";
const releaseTag = "v0.2.21";
const releasePleaseHeadSha = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const qiweEnablementHeadSha = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const sidecarHash = "1".repeat(64);
const productionSidecarHash = "2".repeat(64);
const stagingDatabaseHash = "3".repeat(64);
const productionDatabaseHash = "4".repeat(64);
const contentHash = `sha256:${"a".repeat(64)}`;

try {
  writeFakeGh();
  assertFakeGh();
  const files = writeEvidenceFiles();
  const manifest = path.join(tmpRoot, "completion-manifest.json");
  let result = runBuilder(files, manifest);
  assert.equal(result.status, 0, result.stderr);

  const generated = JSON.parse(fs.readFileSync(manifest, "utf8"));
  assert.equal(generated.release_please_validation.pr_number, 246);
  assert.equal(generated.release_please_validation.head_sha, releasePleaseHeadSha);
  assert.equal(generated.release_please_validation.release_tag, releaseTag);
  assert.equal(
    generated.release_please_validation.released_commit_sha,
    productionReleaseSha
  );
  assert.equal(generated.qiwe_production_enablement.pr_number, 244);
  assert.equal(generated.qiwe_production_enablement.head_sha, qiweEnablementHeadSha);
  assert.equal(
    generated.huabaosi_production_activation.sidecar_binary_sha256,
    productionSidecarHash
  );
  assert.deepEqual(generated.real_activity_confirmation, {
    qiwe_group_arrival_confirmed: true,
    confirmed_by: "owner",
    confirmed_at: "2026-07-20T06:30:00Z",
  });

  result = runCompletionChecker({ ...files, manifest });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /Xiaoman production completion evidence check passed/);

  const drifted = writeEvidenceFiles({
    production: { common: { production_release_sha: stagingReleaseSha } },
  });
  result = runBuilder(drifted, path.join(tmpRoot, "drifted-manifest.json"));
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /evidence release SHA does not match/);

  result = runBuilder(files, path.join(tmpRoot, "bad-release-head-manifest.json"), {
    FAKE_RELEASE_PR_HEAD_SHA: "c".repeat(40),
  });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Release Please PR head SHA does not match/);

  result = runBuilder(files, path.join(tmpRoot, "draft-release-manifest.json"), {
    FAKE_RELEASE_DRAFT: "true",
  });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /published GitHub Release must exist/);

  result = runBuilder(files, path.join(tmpRoot, "sensitive-gh-failure-manifest.json"), {
    FAKE_GH_SENSITIVE_FAILURE: "1",
  });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /redacted-sensitive-diagnostic/);
  assert.doesNotMatch(result.stderr, /fileAesKey|request_id|live-secret/);

  result = runBuilder(files, path.join(tmpRoot, "release-tag-drift-manifest.json"), {
    FAKE_RELEASE_TAG_SHA: stagingReleaseSha,
  });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /published GitHub Release tag does not point/);

  result = runBuilder(
    files,
    path.join(tmpRoot, "annotated-release-tag-manifest.json"),
    {
      FAKE_RELEASE_TAG_TYPE: "tag",
    }
  );
  assert.equal(result.status, 0, result.stderr);

  result = runBuilder(files, path.join(tmpRoot, "open-qiwe-pr-manifest.json"), {
    FAKE_QIWE_PR_STATE: "OPEN",
  });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /QiWe production enablement PR must be merged/);

  result = runBuilder(files, path.join(tmpRoot, "unreleased-qiwe-pr-manifest.json"), {
    FAKE_COMPARE_STATUS: "diverged",
  });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /QiWe production enablement PR head is not included/);

  console.log("Xiaoman production completion manifest builder test passed.");
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

function runBuilder(files, output, env = {}) {
  return spawnSync(
    "node",
    [
      builder,
      "--release-please-pr-number",
      "246",
      "--release-please-head-sha",
      releasePleaseHeadSha,
      "--release-tag",
      releaseTag,
      "--released-commit-sha",
      productionReleaseSha,
      "--qiwe-production-enablement-pr-number",
      "244",
      "--qiwe-production-enablement-head-sha",
      qiweEnablementHeadSha,
      "--huabaosi-production-canary",
      files.huabaosiProductionCanary,
      "--production-real-activity",
      files.production,
      "--qiwe-group-arrival-confirmation",
      files.qiweGroupArrivalConfirmation,
      "--output",
      output,
    ],
    {
      cwd: repoRoot,
      encoding: "utf8",
      env: {
        ...process.env,
        ...env,
        PATH: `${fakeBin}${path.delimiter}${process.env.PATH ?? ""}`,
      },
    }
  );
}

function runCompletionChecker(files) {
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
      "--huabaosi-production-canary",
      files.huabaosiProductionCanary,
      "--production-real-activity",
      files.production,
      "--qiwe-group-arrival-confirmation",
      files.qiweGroupArrivalConfirmation,
    ],
    { cwd: repoRoot, encoding: "utf8" }
  );
}

function writeFakeGh() {
  fs.mkdirSync(fakeBin, { recursive: true });
  const fakeGh = path.join(fakeBin, "gh");
  fs.writeFileSync(
    fakeGh,
    `#!/usr/bin/env node
const args = process.argv.slice(2);
const releaseTag = ${JSON.stringify(releaseTag)};
const releasePleaseHeadSha = ${JSON.stringify(releasePleaseHeadSha)};
const productionReleaseSha = ${JSON.stringify(productionReleaseSha)};
const qiweEnablementHeadSha = ${JSON.stringify(qiweEnablementHeadSha)};

function write(payload) {
  process.stdout.write(JSON.stringify(payload));
}

if (process.env.FAKE_GH_SENSITIVE_FAILURE === "1") {
  console.error('{"fileAesKey":"live-secret","request_id":"raw-request"}');
  process.exit(1);
}

if (args[0] === "pr" && args[1] === "view") {
  const number = Number.parseInt(args[2], 10);
  if (number === 246) {
    const missingCheck = process.env.FAKE_RELEASE_PR_MISSING_CHECK;
    const checks = [
      { name: "changes", conclusion: "SUCCESS" },
      { name: "check", conclusion: "SUCCESS" },
      { context: "Release Please validation", state: "SUCCESS" },
    ].filter((check) => !missingCheck || (check.name !== missingCheck && check.context !== missingCheck));
    write({
      number,
      state: process.env.FAKE_RELEASE_PR_STATE || "MERGED",
      baseRefName: "master",
      headRefOid: process.env.FAKE_RELEASE_PR_HEAD_SHA || releasePleaseHeadSha,
      mergeCommit: { oid: process.env.FAKE_RELEASE_PR_MERGE_SHA || productionReleaseSha },
      statusCheckRollup: checks,
    });
    process.exit(0);
  }
  if (number === 244) {
    write({
      number,
      state: process.env.FAKE_QIWE_PR_STATE || "MERGED",
      baseRefName: "master",
      headRefOid: process.env.FAKE_QIWE_PR_HEAD_SHA || qiweEnablementHeadSha,
      mergeCommit: { oid: "dddddddddddddddddddddddddddddddddddddddd" },
    });
    process.exit(0);
  }
}

if (args[0] === "api" && args[1] === \`repos/:owner/:repo/releases/tags/\${releaseTag}\`) {
  write({
    tag_name: releaseTag,
    draft: process.env.FAKE_RELEASE_DRAFT === "true",
    prerelease: process.env.FAKE_RELEASE_PRERELEASE === "true",
  });
  process.exit(0);
}

if (args[0] === "api" && args[1] === \`repos/:owner/:repo/git/ref/tags/\${releaseTag}\`) {
  write({
    ref: \`refs/tags/\${releaseTag}\`,
    object: {
      type: process.env.FAKE_RELEASE_TAG_TYPE || "commit",
      sha: process.env.FAKE_RELEASE_TAG_TYPE === "tag"
        ? "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
        : process.env.FAKE_RELEASE_TAG_SHA || productionReleaseSha,
    },
  });
  process.exit(0);
}

if (args[0] === "api" && args[1] === "repos/:owner/:repo/git/tags/eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee") {
  write({
    object: {
      type: "commit",
      sha: process.env.FAKE_ANNOTATED_TAG_SHA || productionReleaseSha,
    },
  });
  process.exit(0);
}

if (args[0] === "api" && args[1]?.includes("/compare/")) {
  write({ status: process.env.FAKE_COMPARE_STATUS || "ahead" });
  process.exit(0);
}

console.error("unexpected fake gh call:", args.join(" "));
process.exit(1);
`,
    { mode: 0o755 }
  );
}

function assertFakeGh() {
  const result = spawnSync(
    "gh",
    [
      "pr",
      "view",
      "246",
      "--json",
      "number,state,baseRefName,headRefOid,mergeCommit,statusCheckRollup",
    ],
    {
      cwd: repoRoot,
      encoding: "utf8",
      env: {
        ...process.env,
        PATH: `${fakeBin}${path.delimiter}${process.env.PATH ?? ""}`,
      },
    }
  );
  assert.equal(result.status, 0, result.stderr);
  const payload = JSON.parse(result.stdout);
  assert.ok(payload.statusCheckRollup?.[0], result.stdout);
  assert.equal(payload.statusCheckRollup[0].name, "changes");
}

function writeEvidenceFiles(overrides = {}) {
  const files = {
    stagingRuntime: path.join(tmpRoot, `staging-${cryptoSuffix()}.txt`),
    huabaosi: path.join(tmpRoot, `huabaosi-${cryptoSuffix()}.txt`),
    qiwe: path.join(tmpRoot, `qiwe-${cryptoSuffix()}.txt`),
    huabaosiProductionCanary: path.join(tmpRoot, `canary-${cryptoSuffix()}.txt`),
    production: path.join(tmpRoot, `production-${cryptoSuffix()}.txt`),
    qiweGroupArrivalConfirmation: path.join(
      tmpRoot,
      `confirmation-${cryptoSuffix()}.txt`
    ),
  };
  fs.writeFileSync(
    files.stagingRuntime,
    `staging_runtime_readiness_evidence=${JSON.stringify(stagingRuntimeReadiness())}\n`,
    "utf8"
  );
  fs.writeFileSync(files.huabaosi, huabaosiStagingOutput(), "utf8");
  fs.writeFileSync(files.qiwe, qiweStagingOutput(), "utf8");
  fs.writeFileSync(
    files.huabaosiProductionCanary,
    huabaosiProductionCanaryOutput(overrides.huabaosiProductionCanary ?? {}),
    "utf8"
  );
  fs.writeFileSync(
    files.production,
    productionOutput(overrides.production ?? {}),
    "utf8"
  );
  fs.writeFileSync(
    files.qiweGroupArrivalConfirmation,
    qiweGroupArrivalConfirmationOutput(overrides.qiweGroupArrivalConfirmation ?? {}),
    "utf8"
  );
  return files;
}

function cryptoSuffix() {
  return Math.random().toString(16).slice(2);
}

function stagingRuntimeReadiness() {
  return {
    success: true,
    worker: "staging-runtime-readiness-evidence",
    action_status: "ready_for_huabaosi_qiwe_staging_smokes",
    test_mode: false,
    release_sha: stagingReleaseSha,
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

function huabaosiProductionCanaryOutput(overrides = {}) {
  const briefArtifactId = "88888888-9999-4aaa-8bbb-cccccccccccc";
  const briefWorkItemId = "99999999-aaaa-4bbb-8ccc-dddddddddddd";
  const imageWorkItemId = "55555555-6666-4777-8888-999999999999";
  const generatedImageArtifactId = "66666666-7777-4888-8999-aaaaaaaaaaaa";
  const common = {
    approved_database_url_sha256_matched: true,
    approved_sidecar_sha256_matched: true,
    database_url_sha256: productionDatabaseHash,
    release_binary_verified: true,
    release_sha: productionReleaseSha,
    sidecar_binary_sha256: productionSidecarHash,
    success: true,
  };
  const records = [
    {
      ...common,
      phase: "preflight",
      action_status: "adapter_config_ready",
      timer_active: false,
    },
    {
      ...common,
      phase: "brief_review",
      action_status: "review_recorded",
      brief_artifact_id: briefArtifactId,
      brief_work_item_id: briefWorkItemId,
      review_status: "approved",
      reviewer_id: "trainer",
    },
    {
      ...common,
      phase: "request_intake",
      action_status: "image_generation_requests_created",
      brief_artifact_id: briefArtifactId,
      brief_work_item_id: briefWorkItemId,
      image_generation_work_item_id: imageWorkItemId,
      request_created: true,
    },
    {
      ...common,
      phase: "generation",
      action_status: "generated_image_created",
      artifact_id: generatedImageArtifactId,
      byte_size: 123456,
      content_hash: contentHash,
      height: 1024,
      image_generation_work_item_id: imageWorkItemId,
      mime_type: "image/jpeg",
      review_status: "pending",
      storage_backend: "feishu-base",
      width: 1024,
    },
    {
      ...common,
      phase: "revalidation",
      action_status: "feishu_primary_storage_revalidated",
      artifact_id: generatedImageArtifactId,
      byte_size: 123456,
      content_hash: contentHash,
      database_writes_executed: false,
      external_calls_executed: true,
      height: 1024,
      width: 1024,
    },
  ].map((record) => deepMerge(record, overrides[record.phase] ?? {}));
  return [
    ...records.map(
      (record) =>
        `huabaosi_image_generation_production_canary_evidence=${JSON.stringify(record)}`
    ),
    "Huabaosi production canary passed: one Feishu-backed JPEG remains pending human review; no generated-image approval, mirror, publish, QiWe, or send was executed",
    "",
  ].join("\n");
}

function productionOutput(overrides = {}) {
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
    approved_database_url_sha256_matched: true,
    safe_for_chat: false,
    ...(overrides.common ?? {}),
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

function qiweGroupArrivalConfirmationOutput(overrides = {}) {
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
      workflow_root_id: "44444444-5555-4666-8777-888888888888",
      send_ready_work_item_id: "77777777-8888-4999-8aaa-bbbbbbbbbbbb",
      generated_image_artifact_id: "66666666-7777-4888-8999-aaaaaaaaaaaa",
      artifact_content_hash: contentHash,
      external_send_executed: true,
      raw_secret_fields_retained: false,
      ...overrides,
    })}`,
    "",
  ].join("\n");
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
