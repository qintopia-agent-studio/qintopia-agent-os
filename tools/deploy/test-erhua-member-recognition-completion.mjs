#!/usr/bin/env node

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";

const repoRoot = process.cwd();
const checker = path.join(
  repoRoot,
  "tools/deploy/check-erhua-member-recognition-completion.mjs"
);
const tmpRoot = fs.mkdtempSync(
  path.join(os.tmpdir(), "erhua-member-recognition-completion-")
);
const PERSON_PAXON = "223abda2-a6bd-46cc-abbe-ac6ff0b5fc2f";
const PERSON_CICI = "123e4567-e89b-42d3-a456-426614174000";
const PERSON_NEW_FRIEND = "b7f1b9f4-c7f2-4898-9c4a-91e1b85a9f6d";
const ROOM_SCOPE =
  "sha256:c5c4e70d823efa23b83de70ce5008d746e76bdce54e37605b967b4bfd4036356";
const OTHER_ROOM_SCOPE =
  "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

try {
  let files = writeEvidence("valid", coverage(), canaries());
  const summaryPath = path.join(tmpRoot, "valid-completion-summary.json");
  let result = runChecker(files.roomSync, files.profile, files.coverage, files.canary, {
    summaryOutput: summaryPath,
  });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /completion check passed/);
  assert.match(result.stdout, /12 synced room members/);
  assert.match(
    result.stdout,
    /3 mentioned canaries, 2 speaker canaries, 2 referenced canaries/
  );
  assert.match(result.stdout, /2\/2 linked people resolved/);
  const summary = JSON.parse(fs.readFileSync(summaryPath, "utf8"));
  assert.equal(summary.schema_version, "erhua_member_recognition_completion_v1");
  assert.equal(summary.passed, true);
  assert.equal(summary.scope_fingerprint, ROOM_SCOPE);
  assert.equal(summary.room_sync.room_members_discovered, 12);
  assert.equal(summary.current_room_qiwe_identities.safe_total, 10);
  assert.equal(summary.current_room_qiwe_identities.unsafe_display_unlinked, 0);
  assert.equal(summary.linked_people.total, 2);
  assert.equal(summary.profile_repair.requested_message_limit, 5000);
  assert.equal(summary.answer_context_canaries.mentioned_records, 3);
  assert.equal(summary.answer_context_canaries.speaker_records, 2);
  assert.equal(summary.answer_context_canaries.referenced_records, 2);
  assert.equal(summary.answer_context_canaries.linked_profile_hint_people, 1);
  assert.equal(summary.retained_evidence_boundary.includes_person_id, false);
  assert.doesNotMatch(JSON.stringify(summary), new RegExp(PERSON_PAXON, "i"));

  result = runChecker(files.roomSync, files.profile, files.coverage, files.canary, {
    requireActiveProfiles: true,
  });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /non-empty safe profile hints/);

  files = writeEvidence(
    "valid-active-profile-strict",
    coverage(),
    fullProfileCanaries()
  );
  result = runChecker(files.roomSync, files.profile, files.coverage, files.canary, {
    requireActiveProfiles: true,
  });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /completion check passed/);

  result = runCheckerWithoutRoomSync(files.profile, files.coverage, files.canary);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /--room-sync/);

  files = writeEvidence(
    "valid-identity-only",
    coverage({
      linked_people_total: 3,
      linked_people_with_active_profile: 2,
      linked_people_without_active_profile: 1,
      qiwe_platform_identity_materializable_users: 3,
      answer_context_canary_specs_total: 4,
      answer_context_canary_people_total: 3,
      answer_context_speaker_canary_specs_total: 3,
      answer_context_speaker_canary_people_total: 3,
      answer_context_referenced_canary_specs_total: 3,
      answer_context_referenced_canary_people_total: 3,
      answer_context_canary_specs: [
        ...defaultMentionCanarySpecs(),
        newFriendMentionCanarySpec(),
      ],
      answer_context_speaker_canary_specs: [
        ...defaultSpeakerCanarySpecs(),
        newFriendSpeakerCanarySpec(),
      ],
      answer_context_referenced_canary_specs: [
        ...defaultReferencedCanarySpecs(),
        newFriendReferencedCanarySpec(),
      ],
    }),
    [
      ...canaries(),
      identityOnlyCanary("新朋友", PERSON_NEW_FRIEND),
      identityOnlySpeakerCanary("新朋友", "person:new-friend", PERSON_NEW_FRIEND),
      identityOnlyReferencedCanary("新朋友", "person:new-friend", PERSON_NEW_FRIEND),
    ]
  );
  result = runChecker(files.roomSync, files.profile, files.coverage, files.canary);
  assert.equal(result.status, 0, result.stderr);
  assert.match(
    result.stdout,
    /4 mentioned canaries, 3 speaker canaries, 3 referenced canaries/
  );
  assert.match(result.stdout, /3\/3 linked people resolved/);
  result = runChecker(files.roomSync, files.profile, files.coverage, files.canary, {
    requireActiveProfiles: true,
  });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /active reply_context profiles/);
  assert.match(result.stderr, /identity-only canary people must be zero/);

  files = writeEvidence(
    "valid-all-identity-only",
    coverage({
      linked_people_total: 1,
      linked_people_with_active_profile: 0,
      linked_people_without_active_profile: 1,
      qiwe_platform_identity_materializable_users: 1,
      answer_context_canary_specs_total: 1,
      answer_context_canary_people_total: 1,
      answer_context_speaker_canary_specs_total: 1,
      answer_context_speaker_canary_people_total: 1,
      answer_context_referenced_canary_specs_total: 1,
      answer_context_referenced_canary_people_total: 1,
      linked_people_with_running_facts: 0,
      running_people_with_profile_running_hint: 0,
      answer_context_canary_specs: [newFriendMentionCanarySpec()],
      answer_context_speaker_canary_specs: [newFriendSpeakerCanarySpec()],
      answer_context_referenced_canary_specs: [newFriendReferencedCanarySpec()],
    }),
    [
      identityOnlyCanary("新朋友", PERSON_NEW_FRIEND),
      identityOnlySpeakerCanary("新朋友", "person:new-friend", PERSON_NEW_FRIEND),
      identityOnlyReferencedCanary("新朋友", "person:new-friend", PERSON_NEW_FRIEND),
    ],
    roomSync(),
    profile({
      valuable_messages: 0,
      candidate_fact_count: 0,
      facts_inserted: 0,
      summaries_inserted: 0,
      snapshots_inserted: 0,
    })
  );
  result = runChecker(files.roomSync, files.profile, files.coverage, files.canary);
  assert.equal(result.status, 0, result.stderr);
  assert.match(
    result.stdout,
    /1 mentioned canaries, 1 speaker canaries, 1 referenced canaries/
  );
  assert.match(result.stdout, /1\/1 linked people resolved/);

  files = writeEvidence(
    "unlinked",
    coverage({
      total_channel_identities: 1,
      qiwe_channel_identities_linked: 9,
    }),
    canaries()
  );
  result = runChecker(files.roomSync, files.profile, files.coverage, files.canary);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /non-ambiguous unlinked identities/);

  files = writeEvidence(
    "potential-member-unlinked",
    coverage({
      qiwe_room_potential_member_identities_linked: 9,
      qiwe_room_potential_member_identities_unlinked: 1,
    }),
    canaries()
  );
  result = runChecker(files.roomSync, files.profile, files.coverage, files.canary);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /potential member identities/);

  files = writeEvidence(
    "canary-count-mismatch",
    coverage({ answer_context_canary_specs_total: 4 }),
    canaries()
  );
  result = runChecker(files.roomSync, files.profile, files.coverage, files.canary);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /canary record count/);

  files = writeEvidence(
    "speaker-canary-count-mismatch",
    coverage({ answer_context_speaker_canary_specs_total: 3 }),
    canaries()
  );
  result = runChecker(files.roomSync, files.profile, files.coverage, files.canary);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /speaker self-canary record count/);

  files = writeEvidence(
    "coverage-canary-set-mismatch",
    coverage({
      answer_context_speaker_canary_specs: [
        {
          id: 101,
          canary_type: "speaker_self",
          expected_speaker_label: "小乔",
          canonical_key: "person:paxon",
        },
        {
          id: 102,
          canary_type: "speaker_self",
          expected_speaker_label: "错位成员",
          canonical_key: "person:wrong-member",
        },
      ],
    }),
    canaries()
  );
  result = runChecker(files.roomSync, files.profile, files.coverage, files.canary);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /same canonical people/);

  files = writeEvidence(
    "speaker-platform-identity-missing",
    coverage({ linked_people_without_qiwe_platform_identity: 1 }),
    canaries()
  );
  result = runChecker(files.roomSync, files.profile, files.coverage, files.canary);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /speaker recognition/);

  files = writeEvidence(
    "profile-hint-route-mismatch",
    coverage(),
    profileHintRouteMismatchCanaries()
  );
  result = runChecker(files.roomSync, files.profile, files.coverage, files.canary);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /profile hint evidence must cover the same people/);

  files = writeEvidence("speaker-resolved-people-mismatch", coverage(), [
    canary("小乔", PERSON_PAXON, { canonical_key: "person:paxon" }),
    canary("Paxon", PERSON_PAXON, { canonical_key: "person:paxon" }),
    canary("Cici", PERSON_CICI, {
      canonical_key: "person:cici",
      required_profile_terms: ["跑步"],
      safe_summary: "Cici 最近的安全上下文与跑步活动有关。",
      safe_reply_hints: {
        topics: ["跑步活动"],
        stable_profile_notes: ["多次参与跑步活动"],
      },
    }),
    speakerCanary("小乔", "person:paxon", PERSON_PAXON),
    speakerCanary("Cici", "person:cici", PERSON_NEW_FRIEND, {
      required_profile_terms: ["跑步"],
      safe_summary: "Cici 最近的安全上下文与跑步活动有关。",
      safe_reply_hints: {
        topics: ["跑步活动"],
        stable_profile_notes: ["多次参与跑步活动"],
      },
    }),
  ]);
  result = runChecker(files.roomSync, files.profile, files.coverage, files.canary);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /resolve the same people/);

  files = writeEvidence("missing-running-term", coverage(), [
    canary("小乔", PERSON_PAXON, { canonical_key: "person:paxon" }),
    canary("Paxon", PERSON_PAXON, { canonical_key: "person:paxon" }),
    canary("Cici", PERSON_CICI, {
      canonical_key: "person:cici",
      required_profile_terms: ["跑步"],
      safe_summary: "Cici 已识别为群内成员。",
    }),
    ...speakerCanaries(),
  ]);
  result = runChecker(files.roomSync, files.profile, files.coverage, files.canary);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /missing required term "跑步"/);

  files = writeEvidence("mention-text-mismatch", coverage(), [
    canary("小乔", PERSON_PAXON, {
      canonical_key: "person:paxon",
      mention_text: "乔",
      display_name: "小乔",
    }),
    canary("Paxon", PERSON_PAXON, { canonical_key: "person:paxon" }),
    canary("Cici", PERSON_CICI, {
      canonical_key: "person:cici",
      required_profile_terms: ["跑步"],
      safe_summary: "Cici 最近的安全上下文与跑步活动有关。",
      safe_reply_hints: {
        topics: ["跑步活动"],
        stable_profile_notes: ["多次参与跑步活动"],
      },
    }),
    ...speakerCanaries(),
  ]);
  result = runChecker(files.roomSync, files.profile, files.coverage, files.canary);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /mentioned member was not returned/);

  files = writeEvidence("wrong-generated-canary-name", coverage(), [
    canary("小乔", PERSON_PAXON, { canonical_key: "person:paxon" }),
    canary("Paxonn", PERSON_PAXON, { canonical_key: "person:paxon" }),
    canary("Cici", PERSON_CICI, {
      canonical_key: "person:cici",
      required_profile_terms: ["跑步"],
      safe_summary: "Cici 最近的安全上下文与跑步活动有关。",
      safe_reply_hints: {
        topics: ["跑步活动"],
        stable_profile_notes: ["多次参与跑步活动"],
      },
    }),
    ...speakerCanaries(),
  ]);
  result = runChecker(files.roomSync, files.profile, files.coverage, files.canary);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /was not generated by coverage canary specs/);
  assert.match(result.stderr, /missing canary evidence/);

  files = writeEvidence("required-profile-terms-omitted-from-evidence", coverage(), [
    canary("小乔", PERSON_PAXON, { canonical_key: "person:paxon" }),
    canary("Paxon", PERSON_PAXON, { canonical_key: "person:paxon" }),
    canary("Cici", PERSON_CICI, {
      canonical_key: "person:cici",
      safe_summary: "Cici 最近的安全上下文与跑步活动有关。",
      safe_reply_hints: {
        topics: ["跑步活动"],
        stable_profile_notes: ["多次参与跑步活动"],
      },
    }),
    speakerCanary("小乔", "person:paxon", PERSON_PAXON),
    speakerCanary("Cici", "person:cici", PERSON_CICI, {
      safe_summary: "Cici 最近的安全上下文与跑步活动有关。",
      safe_reply_hints: {
        topics: ["跑步活动"],
        stable_profile_notes: ["多次参与跑步活动"],
      },
    }),
  ]);
  result = runChecker(files.roomSync, files.profile, files.coverage, files.canary);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /required_profile_terms must match coverage spec/);

  files = writeEvidence("resolved-match-count-not-unique", coverage(), [
    canary("小乔", PERSON_PAXON, {
      canonical_key: "person:paxon",
      match_count: 2,
    }),
    canary("Paxon", PERSON_PAXON, { canonical_key: "person:paxon" }),
    canary("Cici", PERSON_CICI, {
      canonical_key: "person:cici",
      required_profile_terms: ["跑步"],
      safe_summary: "Cici 最近的安全上下文与跑步活动有关。",
      safe_reply_hints: {
        topics: ["跑步活动"],
        stable_profile_notes: ["多次参与跑步活动"],
      },
    }),
    ...speakerCanaries(),
  ]);
  result = runChecker(files.roomSync, files.profile, files.coverage, files.canary);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /match_count=1/);

  files = writeEvidence(
    "missing-identity-only-canary",
    coverage({
      linked_people_total: 3,
      linked_people_with_active_profile: 2,
      linked_people_without_active_profile: 1,
      qiwe_platform_identity_materializable_users: 3,
      answer_context_canary_specs_total: 4,
      answer_context_canary_people_total: 3,
      answer_context_speaker_canary_specs_total: 3,
      answer_context_speaker_canary_people_total: 3,
      answer_context_canary_specs: [
        ...defaultMentionCanarySpecs(),
        newFriendMentionCanarySpec(),
      ],
      answer_context_speaker_canary_specs: [
        ...defaultSpeakerCanarySpecs(),
        newFriendSpeakerCanarySpec(),
      ],
    }),
    [
      ...canaries(),
      canary("新朋友", PERSON_NEW_FRIEND, {
        canonical_key: "person:new-friend",
      }),
      speakerCanary("新朋友", "person:new-friend", PERSON_NEW_FRIEND),
    ]
  );
  result = runChecker(files.roomSync, files.profile, files.coverage, files.canary);
  assert.notEqual(result.status, 0);
  assert.match(
    result.stderr,
    /identity-only canary people must match linked people without active profiles/
  );

  files = writeEvidence(
    "identity-only-missing-do-not-infer",
    coverage({
      linked_people_total: 3,
      linked_people_with_active_profile: 2,
      linked_people_without_active_profile: 1,
      qiwe_platform_identity_materializable_users: 3,
      answer_context_canary_specs_total: 4,
      answer_context_canary_people_total: 3,
      answer_context_speaker_canary_specs_total: 3,
      answer_context_speaker_canary_people_total: 3,
      answer_context_canary_specs: [
        ...defaultMentionCanarySpecs(),
        newFriendMentionCanarySpec(),
      ],
      answer_context_speaker_canary_specs: [
        ...defaultSpeakerCanarySpecs(),
        newFriendSpeakerCanarySpec(),
      ],
    }),
    [
      ...canaries(),
      canary("新朋友", PERSON_NEW_FRIEND, {
        canonical_key: "person:new-friend",
        safe_summary: "新朋友 已识别为群内成员，但暂无足够稳定的安全画像。",
        safe_reply_hints: {
          profile_status: "identity_only",
          topics: [],
          stable_profile_notes: [],
        },
      }),
      identityOnlySpeakerCanary("新朋友", "person:new-friend", PERSON_NEW_FRIEND),
    ]
  );
  result = runChecker(files.roomSync, files.profile, files.coverage, files.canary);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /do_not_infer_missing_profile=true/);

  files = writeEvidence(
    "dry-run-room-sync",
    coverage(),
    canaries(),
    roomSync({ dry_run: true, room_member_identities_upserted: 0 })
  );
  result = runChecker(files.roomSync, files.profile, files.coverage, files.canary);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /must be an applied run/);

  files = writeEvidence(
    "dry-run-profile",
    coverage(),
    canaries(),
    roomSync(),
    profile({ dry_run: true })
  );
  result = runChecker(files.roomSync, files.profile, files.coverage, files.canary);
  assert.notEqual(result.status, 0);
  assert.match(
    result.stderr,
    /member profile evidence for completion must be an applied run/
  );

  files = writeEvidence(
    "low-profile-scan-limit",
    coverage(),
    canaries(),
    roomSync(),
    profile({ requested_message_limit: 500 })
  );
  result = runChecker(files.roomSync, files.profile, files.coverage, files.canary);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /--limit 5000/);

  files = writeEvidence(
    "profile-no-valuable-messages",
    coverage(),
    canaries(),
    roomSync(),
    profile({
      valuable_messages: 0,
      candidate_fact_count: 0,
      facts_inserted: 0,
      summaries_inserted: 0,
      snapshots_inserted: 0,
    })
  );
  result = runChecker(files.roomSync, files.profile, files.coverage, files.canary);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /must include valuable messages/);

  files = writeEvidence(
    "scope-mismatch",
    coverage({ scope_fingerprint: OTHER_ROOM_SCOPE }),
    canaries()
  );
  result = runChecker(files.roomSync, files.profile, files.coverage, files.canary);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /scope_fingerprint must match/);

  files = writeEvidence(
    "profile-scope-mismatch",
    coverage(),
    canaries(),
    roomSync(),
    profile({ scope_fingerprints: [OTHER_ROOM_SCOPE] })
  );
  result = runChecker(files.roomSync, files.profile, files.coverage, files.canary);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /member profile scope_fingerprint/);

  files = writeEvidence(
    "profile-multiple-scopes",
    coverage(),
    canaries(),
    roomSync(),
    profile({ scope_fingerprints: [ROOM_SCOPE, OTHER_ROOM_SCOPE] })
  );
  result = runChecker(files.roomSync, files.profile, files.coverage, files.canary);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /exactly one scope_fingerprint/);

  files = writeEvidence(
    "profile-full-report",
    coverage(),
    canaries(),
    roomSync(),
    `${JSON.stringify({ ...profile(), target_chat_ids: ["secret-room"] })}\n`
  );
  result = runChecker(files.roomSync, files.profile, files.coverage, files.canary);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /forbidden sensitive fragment/);

  files = writeEvidence(
    "missing-coverage-scope",
    coverage({ scope_fingerprint: undefined }),
    canaries()
  );
  result = runChecker(files.roomSync, files.profile, files.coverage, files.canary);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /coverage scope_fingerprint/);

  files = writeEvidence(
    "coverage-misses-roster",
    coverage({ qiwe_room_channel_identities_raw_total: 11 }),
    canaries()
  );
  result = runChecker(files.roomSync, files.profile, files.coverage, files.canary);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /must match synced room roster/);

  files = writeEvidence(
    "coverage-overstates-roster",
    coverage({ qiwe_room_channel_identities_raw_total: 13 }),
    canaries()
  );
  result = runChecker(files.roomSync, files.profile, files.coverage, files.canary);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /must match synced room roster/);

  files = writeEvidence(
    "room-identity-count-inflated-by-platform-identities",
    coverage({
      qiwe_channel_identities_raw_total: 12,
      qiwe_room_channel_identities_raw_total: 10,
      qiwe_channel_identities_total: 10,
      qiwe_channel_identities_linked: 10,
      qiwe_channel_identities_excluded: 2,
      qiwe_room_channel_identities_total: 8,
      qiwe_room_channel_identities_linked: 8,
      qiwe_room_channel_identities_excluded: 2,
    }),
    canaries()
  );
  result = runChecker(files.roomSync, files.profile, files.coverage, files.canary);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /current-room raw QiWe identity count/);

  files = writeEvidence(
    "room-safe-identity-unlinked",
    coverage({
      qiwe_room_channel_identities_linked: 9,
    }),
    canaries()
  );
  result = runChecker(files.roomSync, files.profile, files.coverage, files.canary);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /current-room safe QiWe channel identities/);

  files = writeEvidence(
    "secret-leak",
    `${JSON.stringify(coverage())}\nDATABASE_URL=postgresql://example`,
    canaries()
  );
  result = runChecker(files.roomSync, files.profile, files.coverage, files.canary);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /forbidden sensitive fragment/);
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Erhua member recognition completion test passed.");

function runChecker(roomSyncPath, profilePath, coveragePath, canaryPath, options = {}) {
  const args = [
    checker,
    "--room-sync",
    roomSyncPath,
    "--profile",
    profilePath,
    "--coverage",
    coveragePath,
    "--canary",
    canaryPath,
  ];
  if (options.summaryOutput) {
    args.push("--summary-output", options.summaryOutput);
  }
  if (options.requireActiveProfiles) {
    args.push("--require-active-profiles");
  }
  return spawnSync("node", args, {
    cwd: repoRoot,
    encoding: "utf8",
  });
}

function runCheckerWithoutRoomSync(profilePath, coveragePath, canaryPath) {
  return spawnSync(
    "node",
    [
      checker,
      "--profile",
      profilePath,
      "--coverage",
      coveragePath,
      "--canary",
      canaryPath,
    ],
    {
      cwd: repoRoot,
      encoding: "utf8",
    }
  );
}

function writeEvidence(
  name,
  coverageContent,
  canaryContent,
  roomSyncContent = roomSync(),
  profileContent = profile()
) {
  const dir = path.join(tmpRoot, name);
  fs.mkdirSync(dir, { recursive: true });
  const roomSyncPath = path.join(dir, "room-sync.json");
  const profilePath = path.join(dir, "profile.json");
  const coveragePath = path.join(dir, "coverage.json");
  const canaryPath = path.join(dir, "canary.jsonl");
  fs.writeFileSync(
    roomSyncPath,
    typeof roomSyncContent === "string"
      ? roomSyncContent
      : JSON.stringify(roomSyncContent, null, 2),
    "utf8"
  );
  fs.writeFileSync(
    profilePath,
    typeof profileContent === "string"
      ? profileContent
      : JSON.stringify(profileContent, null, 2),
    "utf8"
  );
  fs.writeFileSync(
    coveragePath,
    typeof coverageContent === "string"
      ? coverageContent
      : JSON.stringify(coverageContent, null, 2),
    "utf8"
  );
  fs.writeFileSync(
    canaryPath,
    (Array.isArray(canaryContent) ? canaryContent : [canaryContent])
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

function roomSync(overrides = {}) {
  return {
    total_identity_keys: 0,
    resolved: 0,
    unresolved: 0,
    room_members_discovered: 12,
    room_member_identities_upserted: 12,
    stale_room_member_identities_marked: 3,
    messages_updated: 0,
    platform_identities_materialized: 0,
    source: "current_qiwe_room_member_roster",
    scope_fingerprint: ROOM_SCOPE,
    dry_run: false,
    unresolved_keys: [],
    ...overrides,
  };
}

function profile(overrides = {}) {
  return {
    dry_run: false,
    scope_fingerprints: [ROOM_SCOPE],
    requested_message_limit: 5000,
    messages_scanned: 20,
    messages_skipped_without_person: 0,
    messages_skipped_excluded_identity: 0,
    valuable_messages: 2,
    candidate_fact_count: 2,
    filtered_labels: { noise_or_low_value: 18 },
    facts_inserted: 2,
    summaries_inserted: 2,
    snapshots_inserted: 2,
    ...overrides,
  };
}

function coverage(overrides = {}) {
  return {
    scope_fingerprint: ROOM_SCOPE,
    qiwe_channel_identities_raw_total: 12,
    qiwe_room_channel_identities_raw_total: 12,
    qiwe_room_channel_identities_total: 10,
    qiwe_room_channel_identities_linked: 10,
    qiwe_room_channel_identities_excluded: 2,
    qiwe_room_potential_member_identities_total: 10,
    qiwe_room_potential_member_identities_linked: 10,
    qiwe_room_potential_member_identities_unlinked: 0,
    qiwe_channel_identities_total: 10,
    qiwe_channel_identities_linked: 10,
    qiwe_channel_identities_excluded: 2,
    total_channel_identities: 0,
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
    answer_context_canary_specs_total: 3,
    answer_context_canary_people_total: 2,
    answer_context_speaker_canary_specs_total: 2,
    answer_context_speaker_canary_people_total: 2,
    answer_context_referenced_canary_specs_total: 2,
    answer_context_referenced_canary_people_total: 2,
    linked_people_without_answer_context_canary_spec: 0,
    answer_context_canary_specs: defaultMentionCanarySpecs(),
    answer_context_speaker_canary_specs: defaultSpeakerCanarySpecs(),
    answer_context_referenced_canary_specs: defaultReferencedCanarySpecs(),
    ...overrides,
  };
}

function defaultMentionCanarySpecs() {
  return [
    {
      id: 11,
      canary_type: "mentioned_member",
      expected_mention: "小乔",
      canonical_key: "person:paxon",
    },
    {
      id: 12,
      canary_type: "mentioned_member",
      expected_mention: "Paxon",
      canonical_key: "person:paxon",
    },
    {
      id: 13,
      canary_type: "mentioned_member",
      expected_mention: "Cici",
      canonical_key: "person:cici",
      required_profile_terms: ["跑步"],
    },
  ];
}

function defaultSpeakerCanarySpecs() {
  return [
    {
      id: 101,
      canary_type: "speaker_self",
      expected_speaker_label: "小乔",
      canonical_key: "person:paxon",
    },
    {
      id: 102,
      canary_type: "speaker_self",
      expected_speaker_label: "Cici",
      canonical_key: "person:cici",
      required_profile_terms: ["跑步"],
    },
  ];
}

function defaultReferencedCanarySpecs() {
  return [
    {
      id: 201,
      canary_type: "referenced_member",
      expected_referenced_label: "小乔",
      canonical_key: "person:paxon",
    },
    {
      id: 202,
      canary_type: "referenced_member",
      expected_referenced_label: "Cici",
      canonical_key: "person:cici",
      required_profile_terms: ["跑步"],
    },
  ];
}

function newFriendMentionCanarySpec() {
  return {
    id: 14,
    canary_type: "mentioned_member",
    expected_mention: "新朋友",
    canonical_key: "person:new-friend",
  };
}

function newFriendSpeakerCanarySpec() {
  return {
    id: 103,
    canary_type: "speaker_self",
    expected_speaker_label: "新朋友",
    canonical_key: "person:new-friend",
  };
}

function newFriendReferencedCanarySpec() {
  return {
    id: 203,
    canary_type: "referenced_member",
    expected_referenced_label: "新朋友",
    canonical_key: "person:new-friend",
  };
}

function canaries() {
  return [
    canary("小乔", PERSON_PAXON, { canonical_key: "person:paxon" }),
    canary("Paxon", PERSON_PAXON, { canonical_key: "person:paxon" }),
    canary("Cici", PERSON_CICI, {
      canonical_key: "person:cici",
      required_profile_terms: ["跑步"],
      safe_summary: "Cici 最近的安全上下文与跑步活动有关。",
      safe_reply_hints: {
        topics: ["跑步活动"],
        stable_profile_notes: ["多次参与跑步活动"],
      },
    }),
    ...speakerCanaries(),
    ...referencedCanaries(),
  ];
}

function fullProfileCanaries() {
  const paxonProfile = {
    safe_summary: "Paxon 最近的安全上下文与项目协作有关。",
    safe_reply_hints: {
      topics: ["项目协作"],
      stable_profile_notes: ["多次参与项目协作"],
    },
  };
  const ciciProfile = {
    required_profile_terms: ["跑步"],
    safe_summary: "Cici 最近的安全上下文与跑步活动有关。",
    safe_reply_hints: {
      topics: ["跑步活动"],
      stable_profile_notes: ["多次参与跑步活动"],
    },
  };
  return [
    canary("小乔", PERSON_PAXON, {
      canonical_key: "person:paxon",
      ...paxonProfile,
    }),
    canary("Paxon", PERSON_PAXON, {
      canonical_key: "person:paxon",
      ...paxonProfile,
    }),
    canary("Cici", PERSON_CICI, {
      canonical_key: "person:cici",
      ...ciciProfile,
    }),
    speakerCanary("小乔", "person:paxon", PERSON_PAXON, paxonProfile),
    speakerCanary("Cici", "person:cici", PERSON_CICI, ciciProfile),
    referencedCanary("小乔", "person:paxon", PERSON_PAXON, paxonProfile),
    referencedCanary("Cici", "person:cici", PERSON_CICI, ciciProfile),
  ];
}

function profileHintRouteMismatchCanaries() {
  const paxonProfile = {
    safe_summary: "Paxon 最近的安全上下文与项目协作有关。",
    safe_reply_hints: {
      topics: ["项目协作"],
      stable_profile_notes: ["多次参与项目协作"],
    },
  };
  const ciciProfile = {
    required_profile_terms: ["跑步"],
    safe_summary: "Cici 最近的安全上下文与跑步活动有关。",
    safe_reply_hints: {
      topics: ["跑步活动"],
      stable_profile_notes: ["多次参与跑步活动"],
    },
  };
  return [
    canary("小乔", PERSON_PAXON, {
      canonical_key: "person:paxon",
      ...paxonProfile,
    }),
    canary("Paxon", PERSON_PAXON, {
      canonical_key: "person:paxon",
      ...paxonProfile,
    }),
    canary("Cici", PERSON_CICI, {
      canonical_key: "person:cici",
      ...ciciProfile,
    }),
    speakerCanary("小乔", "person:paxon", PERSON_PAXON),
    speakerCanary("Cici", "person:cici", PERSON_CICI, ciciProfile),
    referencedCanary("小乔", "person:paxon", PERSON_PAXON),
    referencedCanary("Cici", "person:cici", PERSON_CICI, ciciProfile),
  ];
}

function speakerCanaries() {
  return [
    speakerCanary("小乔", "person:paxon", PERSON_PAXON),
    speakerCanary("Cici", "person:cici", PERSON_CICI, {
      required_profile_terms: ["跑步"],
      safe_summary: "Cici 最近的安全上下文与跑步活动有关。",
      safe_reply_hints: {
        topics: ["跑步活动"],
        stable_profile_notes: ["多次参与跑步活动"],
      },
    }),
  ];
}

function referencedCanaries() {
  return [
    referencedCanary("小乔", "person:paxon", PERSON_PAXON),
    referencedCanary("Cici", "person:cici", PERSON_CICI, {
      required_profile_terms: ["跑步"],
      safe_summary: "Cici 最近的安全上下文与跑步活动有关。",
      safe_reply_hints: {
        topics: ["跑步活动"],
        stable_profile_notes: ["多次参与跑步活动"],
      },
    }),
  ];
}

function identityOnlyCanary(
  expectedMention,
  personId,
  canonicalKey = "person:new-friend"
) {
  return canary(expectedMention, personId, {
    canonical_key: canonicalKey,
    safe_summary: `${expectedMention} 已识别为群内成员，但暂无足够稳定的安全画像。`,
    safe_reply_hints: {
      profile_status: "identity_only",
      topics: [],
      stable_profile_notes: [],
      do_not_infer_missing_profile: true,
    },
  });
}

function identityOnlySpeakerCanary(expectedLabel, canonicalKey, personId) {
  return speakerCanary(expectedLabel, canonicalKey, personId, {
    safe_summary: `${expectedLabel} 已识别为群内成员，但暂无足够稳定的安全画像。`,
    safe_reply_hints: {
      profile_status: "identity_only",
      topics: [],
      stable_profile_notes: [],
      do_not_infer_missing_profile: true,
    },
  });
}

function identityOnlyReferencedCanary(expectedLabel, canonicalKey, personId) {
  return referencedCanary(expectedLabel, canonicalKey, personId, {
    safe_summary: `${expectedLabel} 已识别为群内成员，但暂无足够稳定的安全画像。`,
    safe_reply_hints: {
      profile_status: "identity_only",
      topics: [],
      stable_profile_notes: [],
      do_not_infer_missing_profile: true,
    },
  });
}

function canary(expectedMention, personId, overrides = {}) {
  const safeSummary = overrides.safe_summary ?? `${expectedMention} 已识别为群内成员。`;
  const safeReplyHints = overrides.safe_reply_hints ?? {
    topics: [],
    stable_profile_notes: [],
  };
  return {
    canary_type: "mentioned_member",
    expected_mention: expectedMention,
    ...(overrides.canonical_key ? { canonical_key: overrides.canonical_key } : {}),
    ...(overrides.required_profile_terms
      ? { required_profile_terms: overrides.required_profile_terms }
      : {}),
    answer_context: {
      success: true,
      mentioned_members: [
        {
          mention_text: overrides.mention_text ?? expectedMention,
          resolved: true,
          resolution_status: "resolved",
          match_count: overrides.match_count ?? 1,
          display_name: overrides.display_name ?? expectedMention,
          person_ref: personRef(personId),
          safe_summary: safeSummary,
          safe_reply_hints: safeReplyHints,
        },
      ],
    },
  };
}

function speakerCanary(expectedLabel, canonicalKey, personId, overrides = {}) {
  const safeSummary = overrides.safe_summary ?? `${expectedLabel} 已识别为群内成员。`;
  const safeReplyHints = overrides.safe_reply_hints ?? {
    topics: [],
    stable_profile_notes: [],
  };
  return {
    canary_type: "speaker_self",
    expected_speaker_label: expectedLabel,
    canonical_key: canonicalKey,
    ...(overrides.required_profile_terms
      ? { required_profile_terms: overrides.required_profile_terms }
      : {}),
    answer_context: {
      success: true,
      speaker: {
        resolved: true,
        resolution_scope: overrides.resolution_scope ?? "exact_chat",
        display_name: overrides.display_name ?? expectedLabel,
        person_ref: personRef(personId),
        safe_summary: safeSummary,
        safe_reply_hints: safeReplyHints,
      },
      mentioned_members: [],
    },
  };
}

function referencedCanary(expectedLabel, canonicalKey, personId, overrides = {}) {
  const safeSummary = overrides.safe_summary ?? `${expectedLabel} 已识别为群内成员。`;
  const safeReplyHints = overrides.safe_reply_hints ?? {
    topics: [],
    stable_profile_notes: [],
  };
  return {
    canary_type: "referenced_member",
    expected_referenced_label: expectedLabel,
    canonical_key: canonicalKey,
    ...(overrides.required_profile_terms
      ? { required_profile_terms: overrides.required_profile_terms }
      : {}),
    answer_context: {
      success: true,
      referenced_member: {
        resolved: true,
        resolution_scope: overrides.resolution_scope ?? "exact_chat",
        display_name: overrides.display_name ?? expectedLabel,
        person_ref: personRef(personId),
        safe_summary: safeSummary,
        safe_reply_hints: safeReplyHints,
      },
      mentioned_members: [],
    },
  };
}

function personRef(personId) {
  return `sha256:${createHash("sha256")
    .update(`erhua-member-recognition-person-ref-v1:${personId.toLowerCase()}`)
    .digest("hex")}`;
}
