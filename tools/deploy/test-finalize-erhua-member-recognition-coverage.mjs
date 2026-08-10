#!/usr/bin/env node

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const finalizer = path.join(
  repoRoot,
  "tools/deploy/finalize-erhua-member-recognition-coverage.mjs"
);
const tmpRoot = fs.mkdtempSync(
  path.join(os.tmpdir(), "finalize-erhua-member-recognition-coverage-")
);

try {
  let files = writeEvidence("identity-gap", coverage({ total_channel_identities: 1 }));
  let summaryOutput = path.join(tmpRoot, "identity-gap", "coverage-summary.json");
  let result = runFinalizer(files.coverage, summaryOutput);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /identity bootstrap apply is still required/);
  assert.match(result.stdout, /coverage summary check passed/);
  let summary = JSON.parse(fs.readFileSync(summaryOutput, "utf8"));
  assert.equal(summary.schema_version, "erhua_member_recognition_coverage_v1");
  assert.equal(summary.passed, false);
  assert.equal(summary.identity_bootstrap.non_ambiguous_unlinked_identities, 1);

  files = writeEvidence("strict-passed", coverage());
  summaryOutput = path.join(tmpRoot, "strict-passed", "coverage-summary.json");
  result = runFinalizer(files.coverage, summaryOutput, {
    expectPass: true,
    requireActiveProfiles: true,
  });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /coverage check passed/);
  assert.match(result.stdout, /coverage summary check passed/);
  assert.match(result.stdout, /coverage finalized/);
  summary = JSON.parse(fs.readFileSync(summaryOutput, "utf8"));
  assert.equal(summary.passed, true);
  assert.equal(summary.strict_profile_required, true);
  assert.equal(summary.readiness.all_linked_people_have_active_profiles, true);

  result = runFinalizer(files.coverage, path.join(tmpRoot, "missing-dir", "x.json"));
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /summary output directory does not exist/);

  result = runFinalizer(files.coverage, "/dev/null", {
    expectPass: true,
    requireActiveProfiles: true,
  });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /coverage summary is not valid JSON/);
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Erhua member recognition coverage finalizer test passed.");

function runFinalizer(coveragePath, summaryOutput, options = {}) {
  const args = [
    finalizer,
    "--coverage",
    coveragePath,
    "--summary-output",
    summaryOutput,
  ];
  if (options.expectPass) {
    args.push("--expect-pass");
  }
  if (options.requireActiveProfiles) {
    args.push("--require-active-profiles");
  }
  return spawnSync("node", args, {
    cwd: repoRoot,
    encoding: "utf8",
  });
}

function writeEvidence(name, coverageContent) {
  const dir = path.join(tmpRoot, name);
  fs.mkdirSync(dir, { recursive: true });
  const coveragePath = path.join(dir, "coverage.json");
  fs.writeFileSync(coveragePath, JSON.stringify(coverageContent, null, 2), "utf8");
  return { coverage: coveragePath };
}

function coverage(overrides = {}) {
  return {
    qiwe_channel_identities_raw_total: 3,
    qiwe_room_channel_identities_raw_total: 3,
    qiwe_room_channel_identities_total: 3,
    qiwe_room_channel_identities_linked: 3,
    qiwe_room_channel_identities_excluded: 0,
    qiwe_room_potential_member_identities_total: 3,
    qiwe_room_potential_member_identities_linked: 3,
    qiwe_room_potential_member_identities_unlinked: 0,
    total_channel_identities: 0,
    qiwe_channel_identities_total: 3,
    qiwe_channel_identities_linked: 3,
    qiwe_channel_identities_excluded: 0,
    channel_identities_with_existing_person: 0,
    channel_identities_with_existing_name: 0,
    ambiguous_channel_identities_skipped: 0,
    linked_aliases_missing: 0,
    linked_messages_missing_sender_person: 0,
    linked_people_total: 2,
    linked_people_with_active_profile: 2,
    linked_people_without_active_profile: 0,
    qiwe_platform_identity_materializable_users: 2,
    qiwe_platform_identities_missing: 0,
    qiwe_platform_identity_ambiguous_users: 0,
    linked_people_without_qiwe_platform_identity: 0,
    linked_people_with_running_facts: 1,
    running_people_with_profile_running_hint: 1,
    running_people_profile_missing_running_hint: 0,
    answer_context_canary_specs_total: 2,
    answer_context_canary_people_total: 2,
    answer_context_speaker_canary_specs_total: 2,
    answer_context_speaker_canary_people_total: 2,
    answer_context_referenced_canary_specs_total: 2,
    answer_context_referenced_canary_people_total: 2,
    linked_people_without_answer_context_canary_spec: 0,
    answer_context_canary_specs: mentionCanarySpecs(),
    answer_context_speaker_canary_specs: speakerCanarySpecs(),
    answer_context_referenced_canary_specs: referencedCanarySpecs(),
    dry_run: true,
    ...overrides,
  };
}

function mentionCanarySpecs() {
  return [
    {
      id: 11,
      canary_type: "mentioned_member",
      expected_mention: "小乔",
      canonical_key: "person:xiaoqiao",
    },
    {
      id: 12,
      canary_type: "mentioned_member",
      expected_mention: "Paxon",
      canonical_key: "person:paxon",
    },
  ];
}

function speakerCanarySpecs() {
  return [
    {
      id: 101,
      canary_type: "speaker_self",
      expected_speaker_label: "小乔",
      canonical_key: "person:xiaoqiao",
    },
    {
      id: 102,
      canary_type: "speaker_self",
      expected_speaker_label: "Paxon",
      canonical_key: "person:paxon",
    },
  ];
}

function referencedCanarySpecs() {
  return [
    {
      id: 201,
      canary_type: "referenced_member",
      expected_referenced_label: "小乔",
      canonical_key: "person:xiaoqiao",
    },
    {
      id: 202,
      canary_type: "referenced_member",
      expected_referenced_label: "Paxon",
      canonical_key: "person:paxon",
    },
  ];
}
