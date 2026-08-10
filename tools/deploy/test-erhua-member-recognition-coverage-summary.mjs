#!/usr/bin/env node

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const checker = path.join(
  repoRoot,
  "tools/deploy/check-erhua-member-recognition-coverage-summary.mjs"
);
const tmpRoot = fs.mkdtempSync(
  path.join(os.tmpdir(), "erhua-member-recognition-coverage-summary-")
);

try {
  let summaryPath = writeSummary("valid-failed", failedSummary());
  let result = runChecker(summaryPath);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /coverage summary check passed/);

  result = runChecker(summaryPath, { expectPass: true });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /--expect-pass requires passed=true/);

  result = runChecker(summaryPath, { requireActiveProfiles: true });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /strict_profile_required=true/);

  summaryPath = writeSummary("strict-failed", {
    ...failedSummary(),
    strict_profile_required: true,
  });
  result = runChecker(summaryPath);
  assert.equal(result.status, 0, result.stderr);
  result = runChecker(summaryPath, { requireActiveProfiles: true });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /without_active_profile = 0/);

  summaryPath = writeSummary("valid-passed", passedSummary());
  result = runChecker(summaryPath, {
    expectPass: true,
    requireActiveProfiles: true,
  });
  assert.equal(result.status, 0, result.stderr);

  summaryPath = writeSummary("expect-pass-rejects-ambiguous-identities", {
    ...passedSummary(),
    warning_count: 1,
    identity_bootstrap: {
      ...passedSummary().identity_bootstrap,
      ambiguous_identities: 1,
    },
  });
  result = runChecker(summaryPath, { expectPass: true });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /ambiguous_identities = 0/);

  summaryPath = writeSummary("secret-leak", {
    ...failedSummary(),
    display_name: "小乔",
  });
  result = runChecker(summaryPath);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /forbidden sensitive fragment/);

  summaryPath = writeSummary("person-id-leak", {
    ...failedSummary(),
    leaked: "223abda2-a6bd-46cc-abbe-ac6ff0b5fc2f",
  });
  result = runChecker(summaryPath);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /forbidden sensitive fragment/);

  summaryPath = writeSummary("identity-count-mismatch", {
    ...failedSummary(),
    current_room_qiwe_identities: {
      ...failedSummary().current_room_qiwe_identities,
      excluded: 3,
    },
  });
  result = runChecker(summaryPath);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /safe and excluded identity counts/);

  summaryPath = writeSummary("readiness-mismatch", {
    ...failedSummary(),
    readiness: {
      ...failedSummary().readiness,
      all_linked_people_have_active_profiles: true,
    },
  });
  result = runChecker(summaryPath);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /readiness\.all_linked_people_have_active_profiles/);

  summaryPath = writeSummary("boundary-mismatch", {
    ...failedSummary(),
    retained_evidence_boundary: {
      ...failedSummary().retained_evidence_boundary,
      includes_profile_text: true,
    },
  });
  result = runChecker(summaryPath);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /includes_profile_text must be false/);
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Erhua member recognition coverage summary test passed.");

function runChecker(summaryPath, options = {}) {
  const args = [checker, summaryPath];
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

function writeSummary(name, summary) {
  const summaryPath = path.join(tmpRoot, `${name}.json`);
  fs.writeFileSync(summaryPath, `${JSON.stringify(summary, null, 2)}\n`, "utf8");
  return summaryPath;
}

function failedSummary() {
  return {
    schema_version: "erhua_member_recognition_coverage_v1",
    passed: false,
    strict_profile_required: false,
    error_count: 1,
    warning_count: 1,
    current_room_qiwe_identities: {
      raw_total: 12,
      safe_total: 10,
      linked: 9,
      excluded: 2,
    },
    current_room_potential_member_identities: {
      total: 10,
      linked: 9,
      unlinked: 1,
      unsafe_display_unlinked: 0,
    },
    identity_bootstrap: {
      non_ambiguous_unlinked_identities: 1,
      ambiguous_identities: 1,
      reused_existing_people: 0,
      reused_existing_names_or_aliases: 0,
    },
    linked_people: {
      total: 4,
      with_active_profile: 3,
      without_active_profile: 1,
      without_qiwe_platform_identity: 0,
      without_answer_context_canary_spec: 0,
    },
    repair_gaps: {
      linked_aliases_missing: 0,
      linked_messages_missing_sender_person: 0,
      qiwe_platform_identities_missing: 0,
      qiwe_platform_identity_ambiguous_users: 0,
      running_people_profile_missing_running_hint: 0,
    },
    answer_context_canary_specs: {
      mentioned_records: 4,
      mentioned_people: 4,
      speaker_records: 4,
      speaker_people: 4,
      referenced_records: 4,
      referenced_people: 4,
    },
    readiness: {
      all_safe_current_room_identities_linked: false,
      all_current_room_potential_members_linked: false,
      all_linked_people_have_active_profiles: false,
      all_linked_people_have_qiwe_platform_identity: true,
      all_linked_people_have_canary_names: true,
      mentioned_speaker_referenced_canaries_cover_linked_people: true,
      running_profile_hints_cover_running_people: true,
    },
    retained_evidence_boundary: {
      sanitized_summary_only: true,
      includes_chat_id: false,
      includes_sender_id: false,
      includes_channel_user_id: false,
      includes_person_id: false,
      includes_raw_messages: false,
      includes_profile_text: false,
      includes_database_url: false,
      includes_tokens: false,
    },
  };
}

function passedSummary() {
  const summary = failedSummary();
  return {
    ...summary,
    passed: true,
    strict_profile_required: true,
    error_count: 0,
    warning_count: 0,
    current_room_qiwe_identities: {
      ...summary.current_room_qiwe_identities,
      linked: 10,
    },
    current_room_potential_member_identities: {
      ...summary.current_room_potential_member_identities,
      linked: 10,
      unlinked: 0,
    },
    identity_bootstrap: {
      ...summary.identity_bootstrap,
      non_ambiguous_unlinked_identities: 0,
      ambiguous_identities: 0,
    },
    linked_people: {
      ...summary.linked_people,
      with_active_profile: 4,
      without_active_profile: 0,
    },
    readiness: {
      ...summary.readiness,
      all_safe_current_room_identities_linked: true,
      all_current_room_potential_members_linked: true,
      all_linked_people_have_active_profiles: true,
    },
  };
}
