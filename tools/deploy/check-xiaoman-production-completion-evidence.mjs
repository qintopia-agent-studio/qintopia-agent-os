#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const args = process.argv.slice(2);
const options = parseArgs(args);

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
  /"target_group_id"\s*:/,
  /"artifact_uri"\s*:/,
  /"provider_response"\s*:/,
  /"raw_chat"\s*:/,
  /"message_id"\s*:/,
  /"callback_event_id"\s*:/,
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

for (const [label, file] of Object.entries(options)) {
  assertNoSensitiveText(label, fs.readFileSync(file, "utf8"));
}

runChecker("Huabaosi staging evidence", [
  "tools/deploy/check-huabaosi-image-staging-evidence.mjs",
  options.huabaosiStaging,
]);
runChecker("QiWe staging evidence", [
  "tools/deploy/check-qiwe-image-staging-evidence.mjs",
  options.qiweStaging,
]);
runChecker("Xiaoman Huabaosi/QiWe staging cross-flow evidence", [
  "tools/deploy/check-xiaoman-image-send-staging-evidence.mjs",
  options.huabaosiStaging,
  options.qiweStaging,
]);
runChecker("Xiaoman real activity production evidence", [
  "tools/deploy/check-xiaoman-real-activity-production-evidence.mjs",
  options.productionRealActivity,
]);

const completion = JSON.parse(fs.readFileSync(options.manifest, "utf8"));
const stagingRuntime = singlePrefixedRecord(
  options.stagingRuntimeReadiness,
  "staging_runtime_readiness_evidence="
);
const huabaosiRecords = prefixedRecords(
  options.huabaosiStaging,
  "huabaosi_image_generation_staging_evidence="
);
const qiweRecords = prefixedRecords(
  options.qiweStaging,
  "qiwe_image_send_staging_evidence="
);
const huabaosiProductionCanaryRecords = prefixedRecords(
  options.huabaosiProductionCanary,
  "huabaosi_image_generation_production_canary_evidence="
);
const productionRecords = prefixedRecords(
  options.productionRealActivity,
  "xiaoman_real_activity_production_evidence="
);

const huabaosiGeneration = single(
  huabaosiRecords.filter((record) => record.phase === "generation"),
  "expected one Huabaosi staging generation record"
);
const qiweCallback = single(
  qiweRecords.filter((record) => record.phase === "callback"),
  "expected one QiWe staging callback record"
);
const productionRetention = single(
  productionRecords.filter((record) => record.phase === "sanitized_evidence_retention"),
  "expected one production sanitized evidence retention record"
);
const productionImageGeneration = single(
  productionRecords.filter((record) => record.phase === "image_generation"),
  "expected one production image generation record"
);
const productionSignal = single(
  productionRecords.filter((record) => record.phase === "signal_intake"),
  "expected one production signal intake record"
);
const huabaosiProductionCanary = assertHuabaosiProductionCanary(
  huabaosiProductionCanaryRecords
);

assertCompletionManifest(completion);
assertStagingRuntime(stagingRuntime);

if (
  stagingRuntime.packaged_sidecar_sha256 !== huabaosiGeneration.sidecar_binary_sha256 ||
  stagingRuntime.packaged_sidecar_sha256 !== qiweCallback.sidecar_binary_sha256
) {
  fail("staging sidecar SHA-256 does not bind readiness, Huabaosi, and QiWe evidence");
}
if (
  stagingRuntime.staging_database_url_sha256 !== huabaosiGeneration.database_url_sha256
) {
  fail("staging database URL SHA-256 does not bind readiness and Huabaosi evidence");
}
if (huabaosiGeneration.content_hash !== qiweCallback.artifact_content_hash) {
  fail("staging Huabaosi and QiWe image hashes differ");
}

const production = completion.huabaosi_production_activation;
if (
  production.release_sha !== productionRetention.production_release_sha ||
  production.sidecar_binary_sha256 !== productionRetention.sidecar_binary_sha256 ||
  production.database_url_sha256 !== productionRetention.database_url_sha256 ||
  production.release_sha !== huabaosiProductionCanary.release_sha ||
  production.sidecar_binary_sha256 !== huabaosiProductionCanary.sidecar_binary_sha256 ||
  production.database_url_sha256 !== huabaosiProductionCanary.database_url_sha256
) {
  fail("Huabaosi production activation facts do not bind to retained evidence");
}

if (
  huabaosiProductionCanary.artifact_id !==
    productionImageGeneration.generated_image_artifact_id ||
  huabaosiProductionCanary.content_hash !==
    productionImageGeneration.artifact_content_hash ||
  huabaosiProductionCanary.artifact_id !==
    productionRetention.generated_image_artifact_id ||
  huabaosiProductionCanary.content_hash !== productionRetention.artifact_content_hash
) {
  fail("Huabaosi production canary first record does not bind to real activity image");
}

if (
  productionRetention.external_send_executed !== true ||
  productionRetention.raw_secret_fields_retained !== false ||
  productionRetention.release_binary_verified !== true ||
  productionRetention.approved_sidecar_sha256_matched !== true ||
  productionRetention.approved_database_url_sha256_matched !== true
) {
  fail("production real activity retention does not prove the final evidence boundary");
}
if (
  !["pre_event", "post_event"].includes(productionSignal.activity_phase) ||
  !["activity_promotion", "activity_recap"].includes(productionSignal.activity_route) ||
  !activityRouteMatchesPhase(
    productionSignal.activity_phase,
    productionSignal.activity_route
  )
) {
  fail("production signal intake did not bind an eligible Xiaoman activity route");
}

console.log("Xiaoman production completion evidence check passed.");

function parseArgs(argv) {
  const parsed = {};
  for (let index = 0; index < argv.length; index += 2) {
    const key = argv[index];
    const value = argv[index + 1];
    if (!key?.startsWith("--") || !value) {
      usage();
    }
    parsed[key.slice(2)] = path.resolve(value);
  }
  const required = [
    "manifest",
    "staging-runtime-readiness",
    "huabaosi-staging",
    "qiwe-staging",
    "huabaosi-production-canary",
    "production-real-activity",
  ];
  for (const key of required) {
    if (!parsed[key] || !fs.existsSync(parsed[key])) {
      usage();
    }
  }
  return {
    manifest: parsed.manifest,
    stagingRuntimeReadiness: parsed["staging-runtime-readiness"],
    huabaosiStaging: parsed["huabaosi-staging"],
    qiweStaging: parsed["qiwe-staging"],
    huabaosiProductionCanary: parsed["huabaosi-production-canary"],
    productionRealActivity: parsed["production-real-activity"],
  };
}

function usage() {
  fail(
    "usage: node tools/deploy/check-xiaoman-production-completion-evidence.mjs --manifest <completion-manifest.json> --staging-runtime-readiness <readiness-output.txt> --huabaosi-staging <huabaosi-output.txt> --qiwe-staging <qiwe-output.txt> --huabaosi-production-canary <huabaosi-production-canary-output.txt> --production-real-activity <production-output.txt>"
  );
}

function assertHuabaosiProductionCanary(records) {
  const expectedPhases = [
    "preflight",
    "brief_review",
    "request_intake",
    "generation",
    "revalidation",
  ];
  if (records.length !== expectedPhases.length) {
    fail("Huabaosi production canary evidence must contain five fixed phases");
  }
  for (const [index, phase] of expectedPhases.entries()) {
    if (records[index]?.phase !== phase) {
      fail(`unexpected Huabaosi production canary phase at index ${index + 1}`);
    }
  }

  const [preflight, briefReview, requestIntake, generation, revalidation] = records;
  for (const record of records) {
    if (
      record.success !== true ||
      !isGitSha(record.release_sha) ||
      !isSha256(record.sidecar_binary_sha256) ||
      !isSha256(record.database_url_sha256)
    ) {
      fail("Huabaosi production canary shared production boundary is incomplete");
    }
  }
  for (const key of ["release_sha", "sidecar_binary_sha256", "database_url_sha256"]) {
    if (!records.every((record) => record[key] === records[0][key])) {
      fail(`Huabaosi production canary ${key} differs across phases`);
    }
  }

  assertExactKeys(
    preflight,
    new Set([
      "database_url_sha256",
      "phase",
      "release_sha",
      "sidecar_binary_sha256",
      "success",
      "action_status",
      "timer_active",
    ]),
    "Huabaosi production canary preflight"
  );
  if (
    preflight.action_status !== "adapter_config_ready" ||
    preflight.timer_active !== false
  ) {
    fail(
      "Huabaosi production canary preflight does not prove disabled timer readiness"
    );
  }

  assertExactKeys(
    briefReview,
    new Set([
      "database_url_sha256",
      "phase",
      "release_sha",
      "sidecar_binary_sha256",
      "success",
      "action_status",
      "brief_artifact_id",
      "brief_work_item_id",
      "review_status",
      "reviewer_id",
    ]),
    "Huabaosi production canary brief review"
  );
  if (
    briefReview.action_status !== "review_recorded" ||
    !isUuid(briefReview.brief_artifact_id) ||
    !isUuid(briefReview.brief_work_item_id) ||
    briefReview.review_status !== "approved" ||
    briefReview.reviewer_id !== "trainer"
  ) {
    fail("Huabaosi production canary brief review evidence is incomplete");
  }

  assertExactKeys(
    requestIntake,
    new Set([
      "database_url_sha256",
      "phase",
      "release_sha",
      "sidecar_binary_sha256",
      "success",
      "action_status",
      "brief_artifact_id",
      "brief_work_item_id",
      "image_generation_work_item_id",
      "request_created",
    ]),
    "Huabaosi production canary request intake"
  );
  if (
    requestIntake.action_status !== "image_generation_requests_created" ||
    requestIntake.brief_artifact_id !== briefReview.brief_artifact_id ||
    requestIntake.brief_work_item_id !== briefReview.brief_work_item_id ||
    !isUuid(requestIntake.image_generation_work_item_id) ||
    requestIntake.request_created !== true
  ) {
    fail("Huabaosi production canary request intake does not bind approved brief");
  }

  assertExactKeys(
    generation,
    new Set([
      "database_url_sha256",
      "phase",
      "release_sha",
      "sidecar_binary_sha256",
      "success",
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
    "Huabaosi production canary generation"
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
    fail("Huabaosi production canary generation does not prove a pending JPEG");
  }

  assertExactKeys(
    revalidation,
    new Set([
      "database_url_sha256",
      "phase",
      "release_sha",
      "sidecar_binary_sha256",
      "success",
      "action_status",
      "artifact_id",
      "byte_size",
      "content_hash",
      "database_writes_executed",
      "external_calls_executed",
      "height",
      "width",
    ]),
    "Huabaosi production canary revalidation"
  );
  if (
    revalidation.action_status !== "feishu_primary_storage_revalidated" ||
    revalidation.artifact_id !== generation.artifact_id ||
    revalidation.byte_size !== generation.byte_size ||
    revalidation.content_hash !== generation.content_hash ||
    revalidation.width !== generation.width ||
    revalidation.height !== generation.height ||
    revalidation.database_writes_executed !== false ||
    revalidation.external_calls_executed !== true
  ) {
    fail("Huabaosi production canary revalidation does not prove same-byte readback");
  }

  return {
    artifact_id: generation.artifact_id,
    content_hash: generation.content_hash,
    database_url_sha256: generation.database_url_sha256,
    release_sha: generation.release_sha,
    sidecar_binary_sha256: generation.sidecar_binary_sha256,
  };
}

function runChecker(label, checkerArgs) {
  const result = spawnSync("node", checkerArgs, {
    cwd: repoRoot,
    encoding: "utf8",
  });
  if (result.status !== 0) {
    fail(`${label} failed: ${safeDiagnostic(result.stderr || result.stdout)}`);
  }
}

function assertCompletionManifest(manifest) {
  assertExactKeys(
    manifest,
    new Set([
      "schema",
      "release_please_validation",
      "qiwe_production_enablement",
      "huabaosi_production_activation",
      "real_activity_confirmation",
    ]),
    "completion manifest"
  );
  if (manifest.schema !== "xiaoman-production-completion-evidence-v1") {
    fail("completion manifest schema is invalid");
  }
  assertReleasePlease(manifest.release_please_validation);
  assertQiweProductionEnablement(manifest.qiwe_production_enablement);
  assertHuabaosiProductionActivation(manifest.huabaosi_production_activation);
  assertRealActivityConfirmation(manifest.real_activity_confirmation);
}

function assertReleasePlease(record) {
  assertExactKeys(
    record,
    new Set([
      "status",
      "pr_number",
      "head_sha",
      "manual_ci_workflow",
      "release_please_status",
    ]),
    "Release Please validation"
  );
  if (
    record.status !== "passed" ||
    !positiveInteger(record.pr_number) ||
    !isGitSha(record.head_sha) ||
    record.manual_ci_workflow !== "ci.yml" ||
    record.release_please_status !== "success"
  ) {
    fail("Release Please validation evidence is incomplete");
  }
}

function assertQiweProductionEnablement(record) {
  assertExactKeys(
    record,
    new Set([
      "status",
      "pr_number",
      "head_sha",
      "listener_service_timer_reviewed",
      "observation_reviewed",
      "rollback_reviewed",
      "exact_allowlists_reviewed",
      "production_feature_boundary_reviewed",
    ]),
    "QiWe production enablement"
  );
  if (
    record.status !== "merged" ||
    !positiveInteger(record.pr_number) ||
    !isGitSha(record.head_sha) ||
    record.listener_service_timer_reviewed !== true ||
    record.observation_reviewed !== true ||
    record.rollback_reviewed !== true ||
    record.exact_allowlists_reviewed !== true ||
    record.production_feature_boundary_reviewed !== true
  ) {
    fail("QiWe production enablement evidence is incomplete");
  }
}

function assertHuabaosiProductionActivation(record) {
  assertExactKeys(
    record,
    new Set([
      "release_sha",
      "sidecar_binary_sha256",
      "database_url_sha256",
      "image_generation_observation_passed",
      "image_generation_activation_approved",
      "feishu_mirror_observation_passed",
      "feishu_mirror_activation_approved",
      "first_record_evidence_retained",
    ]),
    "Huabaosi production activation"
  );
  if (
    !isGitSha(record.release_sha) ||
    !isSha256(record.sidecar_binary_sha256) ||
    !isSha256(record.database_url_sha256) ||
    record.image_generation_observation_passed !== true ||
    record.image_generation_activation_approved !== true ||
    record.feishu_mirror_observation_passed !== true ||
    record.feishu_mirror_activation_approved !== true ||
    record.first_record_evidence_retained !== true
  ) {
    fail("Huabaosi production activation evidence is incomplete");
  }
}

function assertRealActivityConfirmation(record) {
  assertExactKeys(
    record,
    new Set(["qiwe_group_arrival_confirmed", "confirmed_by", "confirmed_at"]),
    "real activity confirmation"
  );
  if (
    record.qiwe_group_arrival_confirmed !== true ||
    !isSafeLabel(record.confirmed_by) ||
    !isUtcSecondTimestamp(record.confirmed_at)
  ) {
    fail("real activity human group-arrival confirmation is incomplete");
  }
}

function assertStagingRuntime(record) {
  assertExactKeys(
    record,
    new Set([
      "success",
      "worker",
      "action_status",
      "test_mode",
      "release_sha",
      "packaged_sidecar_sha256",
      "staging_database_url_sha256",
      "reports",
      "safe_for_review",
      "limitations",
      "guardrails",
    ]),
    "staging runtime readiness"
  );
  const expectedReportLabels = new Set([
    "prerequisite",
    "huabaosi_readiness",
    "qiwe_readiness",
  ]);
  const reportLabels = new Set(
    Array.isArray(record.reports) ? record.reports.map((entry) => entry.label) : []
  );
  if (
    record.success !== true ||
    record.worker !== "staging-runtime-readiness-evidence" ||
    record.action_status !== "ready_for_huabaosi_qiwe_staging_smokes" ||
    record.test_mode !== false ||
    !isGitSha(record.release_sha) ||
    !isSha256(record.packaged_sidecar_sha256) ||
    !isSha256(record.staging_database_url_sha256) ||
    record.safe_for_review !== true ||
    !Array.isArray(record.reports) ||
    record.reports.length !== 3 ||
    reportLabels.size !== expectedReportLabels.size ||
    ![...expectedReportLabels].every((label) => reportLabels.has(label)) ||
    !record.reports.every((entry) => {
      assertExactKeys(entry, new Set(["label", "success"]), "staging readiness report");
      return expectedReportLabels.has(entry.label) && entry.success === true;
    }) ||
    !Array.isArray(record.limitations) ||
    record.limitations.length !== 0
  ) {
    fail("staging runtime readiness does not prove real staging smoke readiness");
  }
}

function prefixedRecords(file, prefix) {
  return fs
    .readFileSync(file, "utf8")
    .split(/\r?\n/)
    .filter((line) => line.startsWith(prefix))
    .map((line) => {
      const record = JSON.parse(line.slice(prefix.length));
      if (!record || typeof record !== "object" || Array.isArray(record)) {
        fail(`${prefix} record must be a JSON object`);
      }
      return record;
    });
}

function singlePrefixedRecord(file, prefix) {
  return single(prefixedRecords(file, prefix), `expected one ${prefix} record`);
}

function single(records, message) {
  if (records.length !== 1) {
    fail(message);
  }
  return records[0];
}

function assertExactKeys(entry, allowed, label) {
  if (!entry || typeof entry !== "object" || Array.isArray(entry)) {
    fail(`${label} must be a JSON object`);
  }
  for (const key of Object.keys(entry ?? {})) {
    if (!allowed.has(key)) {
      fail(`${label} includes unexpected key: ${key}`);
    }
  }
  for (const key of allowed) {
    if (!(key in (entry ?? {}))) {
      fail(`${label} is missing key: ${key}`);
    }
  }
}

function assertNoSensitiveText(label, text) {
  for (const pattern of forbiddenPatterns) {
    if (pattern.test(text)) {
      fail(`${label} contains forbidden sensitive fragment: ${pattern}`);
    }
  }
}

function safeDiagnostic(text) {
  const firstLine =
    String(text ?? "")
      .split(/\r?\n/)
      .find(Boolean) ?? "no diagnostic";
  return firstLine.replace(/[^\w .:()/=-]/g, "").slice(0, 240);
}

function isGitSha(value) {
  return /^[0-9a-f]{40}$/.test(String(value ?? ""));
}

function isSha256(value) {
  return /^[0-9a-f]{64}$/.test(String(value ?? ""));
}

function positiveInteger(value) {
  return Number.isInteger(value) && value > 0;
}

function isSafeLabel(value) {
  return /^[a-z0-9][a-z0-9_-]{1,63}$/i.test(String(value ?? ""));
}

function isUuid(value) {
  return /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(
    String(value ?? "")
  );
}

function isCanonicalContentHash(value) {
  return /^sha256:[0-9a-f]{64}$/.test(String(value ?? ""));
}

function isUtcSecondTimestamp(value) {
  if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/.test(String(value ?? ""))) {
    return false;
  }
  const parsed = new Date(value);
  return (
    !Number.isNaN(parsed.valueOf()) &&
    parsed.toISOString().replace(".000Z", "Z") === value
  );
}

function activityRouteMatchesPhase(phase, route) {
  return (
    (phase === "pre_event" && route === "activity_promotion") ||
    (phase === "post_event" && route === "activity_recap")
  );
}

function fail(message) {
  console.error(`Xiaoman production completion evidence check failed: ${message}`);
  process.exit(1);
}
