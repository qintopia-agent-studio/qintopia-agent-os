#!/usr/bin/env node

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const checker = path.join(
  repoRoot,
  "tools/deploy/check-huabaosi-image-production-canary-evidence.mjs"
);
const tmpRoot = fs.mkdtempSync(
  path.join(os.tmpdir(), "huabaosi-production-canary-evidence-")
);
const releaseSha = "0123456789abcdef0123456789abcdef01234567";
const sidecarHash = "1".repeat(64);
const databaseHash = "2".repeat(64);
const contentHash = `sha256:${"a".repeat(64)}`;
const otherContentHash = `sha256:${"b".repeat(64)}`;

try {
  const validEvidence = writeEvidence("valid.txt");
  let result = runChecker(validEvidence);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /Huabaosi image production canary evidence check passed/);

  const hashMismatch = writeEvidence("hash-mismatch.txt", {
    revalidation: { content_hash: otherContentHash },
  });
  result = runChecker(hashMismatch);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /authenticated same-byte readback/);

  const redactionMismatch = writeEvidence("redaction-mismatch.txt", {
    revalidation: { sensitive_fields_redacted: false },
  });
  result = runChecker(redactionMismatch);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /authenticated same-byte readback/);

  const timerEnabledMismatch = writeEvidence("timer-enabled-mismatch.txt", {
    preflight: { timer_enabled: true },
  });
  result = runChecker(timerEnabledMismatch);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /adapter readiness/);

  const rawSecret = path.join(tmpRoot, "raw-secret.txt");
  fs.writeFileSync(
    rawSecret,
    `${productionOutput()}\n{"artifact_uri":"https://media.example/private.jpg"}\n`,
    "utf8"
  );
  result = runChecker(rawSecret);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /forbidden sensitive fragment/);

  const missingPhase = path.join(tmpRoot, "missing-phase.txt");
  fs.writeFileSync(
    missingPhase,
    productionOutput()
      .split(/\r?\n/)
      .filter((line) => !line.includes('"phase":"brief_review"'))
      .join("\n"),
    "utf8"
  );
  result = runChecker(missingPhase);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /exactly five fixed phase records/);

  const routeDrift = writeEvidence("request-drift.txt", {
    request_intake: {
      brief_work_item_id: "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee",
    },
  });
  result = runChecker(routeDrift);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /does not bind the approved brief/);

  const sendLeak = writeEvidence("send-leak.txt", {
    generation: { external_send_executed: true },
  });
  result = runChecker(sendLeak);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /unexpected key/);

  const missingCompletion = path.join(tmpRoot, "missing-completion.txt");
  fs.writeFileSync(
    missingCompletion,
    productionOutput()
      .split(/\r?\n/)
      .filter((line) => !line.startsWith("Huabaosi production canary passed:"))
      .join("\n"),
    "utf8"
  );
  result = runChecker(missingCompletion);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /exactly one fixed completion line/);

  const mutableBoundary = writeEvidence("mutable-boundary.txt", {
    generation: { release_binary_verified: false },
  });
  result = runChecker(mutableBoundary);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /shared production boundary/);

  const sidecarBoundary = writeEvidence("sidecar-boundary.txt", {
    revalidation: { approved_sidecar_sha256_matched: false },
  });
  result = runChecker(sidecarBoundary);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /shared production boundary/);

  const databaseBoundary = writeEvidence("database-boundary.txt", {
    preflight: { approved_database_url_sha256_matched: false },
  });
  result = runChecker(databaseBoundary);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /shared production boundary/);

  const profileBoundary = writeEvidence("profile-boundary.txt", {
    generation: { artifact_profile: "qiwe-production" },
  });
  result = runChecker(profileBoundary);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /shared production boundary/);
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Huabaosi image production canary evidence test passed.");

function runChecker(evidencePath) {
  return spawnSync("node", [checker, evidencePath], {
    cwd: repoRoot,
    encoding: "utf8",
  });
}

function writeEvidence(name, overrides = {}) {
  const evidencePath = path.join(tmpRoot, name);
  fs.writeFileSync(evidencePath, productionOutput(overrides), "utf8");
  return evidencePath;
}

function productionOutput(overrides = {}) {
  const briefArtifactId = "11111111-2222-4333-8444-555555555555";
  const briefWorkItemId = "22222222-3333-4444-8555-666666666666";
  const imageWorkItemId = "33333333-4444-4555-8666-777777777777";
  const artifactId = "44444444-5555-4666-8777-888888888888";
  const common = {
    approved_database_url_sha256_matched: true,
    approved_sidecar_sha256_matched: true,
    artifact_profile: "huabaosi-production",
    database_url_sha256: databaseHash,
    release_binary_verified: true,
    release_sha: releaseSha,
    sidecar_binary_sha256: sidecarHash,
    success: true,
  };
  const records = [
    {
      ...common,
      phase: "preflight",
      action_status: "adapter_config_ready",
      timer_active: false,
      timer_enabled: false,
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
      artifact_id: artifactId,
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
      artifact_id: artifactId,
      byte_size: 123456,
      content_hash: contentHash,
      database_writes_executed: false,
      external_calls_executed: true,
      height: 1024,
      sensitive_fields_redacted: true,
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
