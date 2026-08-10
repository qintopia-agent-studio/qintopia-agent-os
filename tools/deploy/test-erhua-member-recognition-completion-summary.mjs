#!/usr/bin/env node

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const checker = path.join(
  repoRoot,
  "tools/deploy/check-erhua-member-recognition-completion-summary.mjs"
);
const tmpRoot = fs.mkdtempSync(
  path.join(os.tmpdir(), "erhua-member-recognition-completion-summary-")
);

try {
  let summaryPath = writeSummary("valid", validSummary());
  let result = runChecker(summaryPath);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /completion summary check passed/);
  assert.match(result.stdout, /12 synced room members/);
  assert.match(result.stdout, /2 linked people/);

  result = runChecker(summaryPath, { requireActiveProfiles: true });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /completion summary check passed/);

  summaryPath = writeSummary("identity-only-default-allowed", identityOnlySummary());
  result = runChecker(summaryPath);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /completion summary check passed/);

  result = runChecker(summaryPath, { requireActiveProfiles: true });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /active reply_context profiles/);
  assert.match(result.stderr, /identity-only canary people must be zero/);

  summaryPath = writeSummary("person-id-leak", {
    ...validSummary(),
    person_id: "223abda2-a6bd-46cc-abbe-ac6ff0b5fc2f",
  });
  result = runChecker(summaryPath);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /forbidden sensitive fragment/);

  summaryPath = writeSummary("unsupported-display-name", {
    ...validSummary(),
    display_name: "小乔",
  });
  result = runChecker(summaryPath);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /unsupported field: display_name/);

  summaryPath = writeSummary("unsupported-profile-note", {
    ...validSummary(),
    linked_people: {
      ...validSummary().linked_people,
      safe_summary: "不应该把画像文本放进完成摘要",
    },
  });
  result = runChecker(summaryPath);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /unsupported field: safe_summary/);

  summaryPath = writeSummary("identity-count-mismatch", {
    ...validSummary(),
    current_room_qiwe_identities: {
      ...validSummary().current_room_qiwe_identities,
      excluded: 3,
    },
  });
  result = runChecker(summaryPath);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /safe and excluded identity counts/);

  summaryPath = writeSummary("potential-member-denominator-too-small", {
    ...validSummary(),
    current_room_qiwe_identities: {
      ...validSummary().current_room_qiwe_identities,
      potential_member_total: 9,
      potential_member_linked: 9,
    },
  });
  result = runChecker(summaryPath);
  assert.notEqual(result.status, 0);
  assert.match(
    result.stderr,
    /potential member identity total must be at least safe_total/
  );

  summaryPath = writeSummary("linked-people-exceed-potential-members", {
    ...validSummary(),
    current_room_qiwe_identities: {
      ...validSummary().current_room_qiwe_identities,
      potential_member_total: 10,
      potential_member_linked: 1,
      potential_member_unlinked: 9,
    },
  });
  result = runChecker(summaryPath);
  assert.notEqual(result.status, 0);
  assert.match(
    result.stderr,
    /linked_people\.total exceeds linked potential member identities/
  );

  summaryPath = writeSummary("missing-speaker-route", {
    ...validSummary(),
    answer_context_canaries: {
      ...validSummary().answer_context_canaries,
      speaker_people_resolved: 1,
    },
  });
  result = runChecker(summaryPath);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /speaker_people_resolved/);

  summaryPath = writeSummary("profile-hint-route-mismatch", {
    ...validSummary(),
    answer_context_canaries: {
      ...validSummary().answer_context_canaries,
      speaker_profile_hint_people: 1,
    },
  });
  result = runChecker(summaryPath);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /speaker_profile_hint_people/);
  assert.match(result.stderr, /mentioned_profile_hint_people/);

  summaryPath = writeSummary("unsafe-boundary", {
    ...validSummary(),
    retained_evidence_boundary: {
      ...validSummary().retained_evidence_boundary,
      includes_sender_id: true,
    },
  });
  result = runChecker(summaryPath);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /includes_sender_id must be false/);
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Erhua member recognition completion summary test passed.");

function runChecker(summaryPath, options = {}) {
  const args = [checker, summaryPath];
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

function validSummary() {
  return {
    schema_version: "erhua_member_recognition_completion_v1",
    passed: true,
    scope_fingerprint:
      "sha256:c5c4e70d823efa23b83de70ce5008d746e76bdce54e37605b967b4bfd4036356",
    room_sync: {
      source: "current_qiwe_room_member_roster",
      dry_run: false,
      room_members_discovered: 12,
      room_member_identities_upserted: 12,
      stale_room_member_identities_marked: 3,
    },
    current_room_qiwe_identities: {
      raw_total: 12,
      safe_total: 10,
      linked: 10,
      excluded: 2,
      potential_member_total: 10,
      potential_member_linked: 10,
      potential_member_unlinked: 0,
    },
    linked_people: {
      total: 2,
      with_active_profile: 2,
      without_active_profile: 0,
      without_qiwe_platform_identity: 0,
      without_answer_context_canary_spec: 0,
    },
    profile_repair: {
      dry_run: false,
      requested_message_limit: 5000,
      messages_scanned: 20,
      valuable_messages: 2,
    },
    running_profile_hints: {
      linked_people_with_running_facts: 1,
      running_people_with_profile_running_hint: 1,
      running_people_profile_missing_running_hint: 0,
    },
    answer_context_canaries: {
      mentioned_records: 3,
      speaker_records: 2,
      referenced_records: 2,
      mentioned_people_resolved: 2,
      speaker_people_resolved: 2,
      referenced_people_resolved: 2,
      linked_people_resolved: 2,
      mentioned_profile_hint_people: 2,
      speaker_profile_hint_people: 2,
      referenced_profile_hint_people: 2,
      linked_profile_hint_people: 2,
      identity_only_people: 0,
    },
    retained_evidence_boundary: {
      sanitized_summary_only: true,
      includes_chat_id: false,
      includes_sender_id: false,
      includes_channel_user_id: false,
      includes_person_id: false,
      includes_raw_messages: false,
      includes_hidden_profile_details: false,
      includes_database_url: false,
      includes_tokens: false,
    },
  };
}

function identityOnlySummary() {
  const summary = validSummary();
  return {
    ...summary,
    linked_people: {
      ...summary.linked_people,
      with_active_profile: 1,
      without_active_profile: 1,
    },
    answer_context_canaries: {
      ...summary.answer_context_canaries,
      mentioned_profile_hint_people: 1,
      speaker_profile_hint_people: 1,
      referenced_profile_hint_people: 1,
      linked_profile_hint_people: 1,
      identity_only_people: 1,
    },
  };
}
