#!/usr/bin/env node

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";

const repoRoot = process.cwd();
const finalizer = path.join(
  repoRoot,
  "tools/deploy/finalize-erhua-member-recognition-completion.mjs"
);
const tmpRoot = fs.mkdtempSync(
  path.join(os.tmpdir(), "finalize-erhua-member-recognition-completion-")
);
const PERSON_PAXON = "223abda2-a6bd-46cc-abbe-ac6ff0b5fc2f";
const ROOM_SCOPE =
  "sha256:c5c4e70d823efa23b83de70ce5008d746e76bdce54e37605b967b4bfd4036356";

try {
  const files = writeEvidence("valid", coverage(), canaries());
  const summaryOutput = path.join(tmpRoot, "valid", "completion-summary.json");
  let result = runFinalizer(files, summaryOutput);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /completion check passed/);
  assert.match(result.stdout, /completion summary check passed/);
  assert.match(result.stdout, /completion finalized/);
  const summary = JSON.parse(fs.readFileSync(summaryOutput, "utf8"));
  assert.equal(summary.schema_version, "erhua_member_recognition_completion_v1");
  assert.equal(summary.passed, true);
  assert.equal(summary.current_room_qiwe_identities.unsafe_display_unlinked, 0);
  assert.equal(summary.linked_people.total, 1);
  assert.doesNotMatch(JSON.stringify(summary), new RegExp(PERSON_PAXON, "i"));

  result = runFinalizer(
    files,
    path.join(tmpRoot, "valid", "identity-only-strict-summary.json"),
    { requireActiveProfiles: true }
  );
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /active reply_context profiles/);
  assert.match(result.stderr, /identity-only canary people must be zero/);

  const activeProfileFiles = writeEvidence(
    "active-profile-strict",
    coverage({
      linked_people_with_active_profile: 1,
      linked_people_without_active_profile: 0,
    }),
    activeProfileCanaries(),
    profile({
      valuable_messages: 1,
      candidate_fact_count: 1,
      facts_inserted: 1,
      summaries_inserted: 1,
      snapshots_inserted: 1,
    })
  );
  result = runFinalizer(
    activeProfileFiles,
    path.join(tmpRoot, "active-profile-strict", "completion-summary.json"),
    { requireActiveProfiles: true }
  );
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /completion finalized/);

  const emptyHintFiles = writeEvidence(
    "empty-hints-strict",
    coverage({
      linked_people_with_active_profile: 1,
      linked_people_without_active_profile: 0,
    }),
    emptyHintCanaries(),
    profile({
      valuable_messages: 1,
      candidate_fact_count: 1,
      facts_inserted: 1,
      summaries_inserted: 1,
      snapshots_inserted: 1,
    })
  );
  result = runFinalizer(
    emptyHintFiles,
    path.join(tmpRoot, "empty-hints-strict", "completion-summary.json"),
    { requireActiveProfiles: true }
  );
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /non-empty safe profile hints/);

  result = runFinalizer(
    writeEvidence("completion-failure", coverage({ total_channel_identities: 1 }), [
      ...canaries(),
    ]),
    path.join(tmpRoot, "completion-failure", "completion-summary.json")
  );
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /non-ambiguous unlinked identities/);

  result = runFinalizer(files, path.join(tmpRoot, "missing-dir", "summary.json"));
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /summary output directory does not exist/);

  result = runFinalizer(files, "/dev/null");
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /completion summary is not valid JSON/);
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Erhua member recognition completion finalizer test passed.");

function runFinalizer(files, summaryOutput, options = {}) {
  const args = [
    finalizer,
    "--room-sync",
    files.roomSync,
    "--profile",
    files.profile,
    "--coverage",
    files.coverage,
    "--canary",
    files.canary,
    "--summary-output",
    summaryOutput,
  ];
  if (options.requireActiveProfiles) {
    args.push("--require-active-profiles");
  }
  return spawnSync("node", args, {
    cwd: repoRoot,
    encoding: "utf8",
  });
}

function writeEvidence(
  name,
  coverageContent,
  canaryContent,
  profileContent = profile()
) {
  const dir = path.join(tmpRoot, name);
  fs.mkdirSync(dir, { recursive: true });
  const roomSyncPath = path.join(dir, "room-sync.json");
  const profilePath = path.join(dir, "profile.json");
  const coveragePath = path.join(dir, "coverage.json");
  const canaryPath = path.join(dir, "canary.jsonl");
  fs.writeFileSync(roomSyncPath, JSON.stringify(roomSync(), null, 2), "utf8");
  fs.writeFileSync(profilePath, JSON.stringify(profileContent, null, 2), "utf8");
  fs.writeFileSync(coveragePath, JSON.stringify(coverageContent, null, 2), "utf8");
  fs.writeFileSync(
    canaryPath,
    canaryContent
      .map((record) => `erhua_member_recognition_canary=${JSON.stringify(record)}`)
      .join("\n")
      .concat("\n"),
    "utf8"
  );
  return {
    roomSync: roomSyncPath,
    profile: profilePath,
    coverage: coveragePath,
    canary: canaryPath,
  };
}

function roomSync() {
  return {
    room_members_discovered: 1,
    room_member_identities_upserted: 1,
    stale_room_member_identities_marked: 0,
    source: "current_qiwe_room_member_roster",
    scope_fingerprint: ROOM_SCOPE,
    dry_run: false,
  };
}

function profile(overrides = {}) {
  return {
    dry_run: false,
    scope_fingerprints: [ROOM_SCOPE],
    requested_message_limit: 5000,
    messages_scanned: 1,
    messages_skipped_without_person: 0,
    messages_skipped_excluded_identity: 0,
    valuable_messages: 0,
    candidate_fact_count: 0,
    facts_inserted: 0,
    summaries_inserted: 0,
    snapshots_inserted: 0,
    ...overrides,
  };
}

function coverage(overrides = {}) {
  return {
    scope_fingerprint: ROOM_SCOPE,
    qiwe_channel_identities_raw_total: 1,
    qiwe_room_channel_identities_raw_total: 1,
    qiwe_room_channel_identities_total: 1,
    qiwe_room_channel_identities_linked: 1,
    qiwe_room_channel_identities_excluded: 0,
    qiwe_room_potential_member_identities_total: 1,
    qiwe_room_potential_member_identities_linked: 1,
    qiwe_room_potential_member_identities_unlinked: 0,
    qiwe_channel_identities_total: 1,
    qiwe_channel_identities_linked: 1,
    qiwe_channel_identities_excluded: 0,
    total_channel_identities: 0,
    ambiguous_channel_identities_skipped: 0,
    linked_aliases_missing: 0,
    linked_messages_missing_sender_person: 0,
    linked_people_total: 1,
    linked_people_with_active_profile: 0,
    linked_people_without_active_profile: 1,
    qiwe_platform_identity_materializable_users: 1,
    qiwe_platform_identities_missing: 0,
    qiwe_platform_identity_ambiguous_users: 0,
    linked_people_without_qiwe_platform_identity: 0,
    linked_people_with_running_facts: 0,
    running_people_with_profile_running_hint: 0,
    running_people_profile_missing_running_hint: 0,
    answer_context_canary_specs_total: 1,
    answer_context_canary_people_total: 1,
    answer_context_speaker_canary_specs_total: 1,
    answer_context_speaker_canary_people_total: 1,
    answer_context_referenced_canary_specs_total: 1,
    answer_context_referenced_canary_people_total: 1,
    linked_people_without_answer_context_canary_spec: 0,
    answer_context_canary_specs: [
      {
        id: 11,
        canary_type: "mentioned_member",
        expected_mention: "Paxon",
        canonical_key: "person:paxon",
      },
    ],
    answer_context_speaker_canary_specs: [
      {
        id: 101,
        canary_type: "speaker_self",
        expected_speaker_label: "Paxon",
        canonical_key: "person:paxon",
      },
    ],
    answer_context_referenced_canary_specs: [
      {
        id: 201,
        canary_type: "referenced_member",
        expected_referenced_label: "Paxon",
        canonical_key: "person:paxon",
      },
    ],
    ...overrides,
  };
}

function canaries() {
  return [
    canary("mentioned_member", "expected_mention", "mentioned_members"),
    canary("speaker_self", "expected_speaker_label", "speaker"),
    canary("referenced_member", "expected_referenced_label", "referenced_member"),
  ];
}

function activeProfileCanaries() {
  return [
    canary("mentioned_member", "expected_mention", "mentioned_members", {
      safe_summary: "Paxon 已识别为群内成员，近期安全画像包含跑步活动。",
      safe_reply_hints: {
        topics: ["跑步活动"],
        stable_profile_notes: ["多次参与跑步活动"],
      },
    }),
    canary("speaker_self", "expected_speaker_label", "speaker", {
      safe_summary: "Paxon 已识别为群内成员，近期安全画像包含跑步活动。",
      safe_reply_hints: {
        topics: ["跑步活动"],
        stable_profile_notes: ["多次参与跑步活动"],
      },
    }),
    canary("referenced_member", "expected_referenced_label", "referenced_member", {
      safe_summary: "Paxon 已识别为群内成员，近期安全画像包含跑步活动。",
      safe_reply_hints: {
        topics: ["跑步活动"],
        stable_profile_notes: ["多次参与跑步活动"],
      },
    }),
  ];
}

function emptyHintCanaries() {
  return [
    canary("mentioned_member", "expected_mention", "mentioned_members", {
      safe_summary: "Paxon 已识别为群内成员。",
      safe_reply_hints: {
        topics: [],
        stable_profile_notes: [],
      },
    }),
    canary("speaker_self", "expected_speaker_label", "speaker", {
      safe_summary: "Paxon 已识别为群内成员。",
      safe_reply_hints: {
        topics: [],
        stable_profile_notes: [],
      },
    }),
    canary("referenced_member", "expected_referenced_label", "referenced_member", {
      safe_summary: "Paxon 已识别为群内成员。",
      safe_reply_hints: {
        topics: [],
        stable_profile_notes: [],
      },
    }),
  ];
}

function canary(canaryType, labelField, targetField, overrides = {}) {
  const target = {
    resolved: true,
    resolution_status: "resolved",
    resolution_scope: "exact_chat",
    mention_text: "Paxon",
    match_count: 1,
    display_name: "Paxon",
    person_ref: personRef(PERSON_PAXON),
    safe_summary:
      overrides.safe_summary ?? "Paxon 已识别为群内成员，但暂无足够稳定的安全画像。",
    safe_reply_hints: overrides.safe_reply_hints ?? {
      profile_status: "identity_only",
      topics: [],
      stable_profile_notes: [],
      do_not_infer_missing_profile: true,
    },
  };
  return {
    canary_type: canaryType,
    [labelField]: "Paxon",
    canonical_key: "person:paxon",
    answer_context:
      targetField === "mentioned_members"
        ? { success: true, mentioned_members: [target] }
        : { success: true, [targetField]: target, mentioned_members: [] },
  };
}

function personRef(personId) {
  return `sha256:${createHash("sha256")
    .update(`erhua-member-recognition-person-ref-v1:${personId.toLowerCase()}`)
    .digest("hex")}`;
}
