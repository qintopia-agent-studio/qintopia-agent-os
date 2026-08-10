#!/usr/bin/env node

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const checker = path.join(
  repoRoot,
  "tools/deploy/check-erhua-member-recognition-coverage.mjs"
);
const identityBootstrapSource = fs.readFileSync(
  path.join(repoRoot, "runtime/sidecar/src/identity_bootstrap.rs"),
  "utf8"
);
const tmpRoot = fs.mkdtempSync(
  path.join(os.tmpdir(), "erhua-member-recognition-coverage-")
);

try {
  assert.doesNotMatch(
    identityBootstrapSource,
    /\$1::text IS NULL OR (?:ci\.)?chat_id = \$1 OR (?:ci\.)?chat_id = ''/,
    "identity bootstrap coverage scope must not include platform identities in the current-room denominator"
  );
  assert.match(
    identityBootstrapSource,
    /\$1::text IS NULL AND ci\.chat_id <> ''/,
    "identity bootstrap should keep no-chat-id scans room-scoped instead of platform-scoped"
  );
  assert.match(
    identityBootstrapSource,
    /ci\.metadata->>'current_qiwe_room_member' = 'true'/,
    "identity bootstrap should count only latest room-roster-marked identities for same-chat member recognition"
  );

  let evidence = writeCase("valid.json", validCoverage());
  let result = runChecker(evidence);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /Erhua member recognition coverage check passed/);
  assert.match(result.stdout, /manual merge needed/);
  assert.match(result.stdout, /no active reply_context profile/);

  const summaryPath = path.join(tmpRoot, "valid-summary.json");
  result = runChecker(evidence, { summaryOutput: summaryPath });
  assert.equal(result.status, 0, result.stderr);
  let summary = JSON.parse(fs.readFileSync(summaryPath, "utf8"));
  assert.equal(summary.schema_version, "erhua_member_recognition_coverage_v1");
  assert.equal(summary.passed, true);
  assert.equal(summary.strict_profile_required, false);
  assert.equal(summary.linked_people.total, 4);
  assert.equal(summary.linked_people.without_active_profile, 1);
  assert.equal(summary.readiness.all_linked_people_have_active_profiles, false);
  assert.equal(summary.retained_evidence_boundary.includes_person_id, false);
  assert.doesNotMatch(
    JSON.stringify(summary),
    /新朋友|重名成员|223abda2-a6bd-46cc-abbe-ac6ff0b5fc2f/i
  );

  const strictSummaryPath = path.join(tmpRoot, "strict-summary.json");
  result = runChecker(evidence, {
    requireActiveProfiles: true,
    summaryOutput: strictSummaryPath,
  });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /full-profile coverage requires active profiles/);
  assert.match(result.stderr, /no active reply_context profile/);
  summary = JSON.parse(fs.readFileSync(strictSummaryPath, "utf8"));
  assert.equal(summary.passed, false);
  assert.equal(summary.strict_profile_required, true);
  assert.equal(summary.error_count, 1);
  assert.equal(summary.linked_people.without_active_profile, 1);
  assert.equal(summary.readiness.all_linked_people_have_active_profiles, false);
  assert.doesNotMatch(
    JSON.stringify(summary),
    /新朋友|重名成员|223abda2-a6bd-46cc-abbe-ac6ff0b5fc2f/i
  );

  evidence = writeCase(
    "prefixed.txt",
    `erhua_member_recognition_coverage=${JSON.stringify(validCoverage())}\n`
  );
  result = runChecker(evidence);
  assert.equal(result.status, 0, result.stderr);

  evidence = writeCase(
    "noisy-cli-output.txt",
    `2026-08-09T16:22:54Z WARN sqlx::query: slow statement\n${JSON.stringify(
      validCoverage(),
      null,
      2
    )}\n`
  );
  result = runChecker(evidence);
  assert.equal(result.status, 0, result.stderr);

  evidence = writeCase(
    "alias-missing.json",
    validCoverage({
      linked_aliases_missing: 1,
      linked_aliases_missing_samples: [
        {
          display_name: "Paxon",
          person_id: "223abda2-a6bd-46cc-abbe-ac6ff0b5fc2f",
        },
      ],
    })
  );
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /missing person aliases/);
  assert.match(result.stderr, /Paxon/);

  evidence = writeCase(
    "raw-total-mismatch.json",
    validCoverage({
      qiwe_channel_identities_raw_total: 13,
    })
  );
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /safe and excluded QiWe identity counts/);

  evidence = writeCase(
    "room-raw-total-mismatch.json",
    validCoverage({
      qiwe_room_channel_identities_raw_total: 13,
    })
  );
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /current-room QiWe identity counts/);

  evidence = writeCase(
    "unsafe-potential-member-unlinked.json",
    validCoverage({
      qiwe_room_potential_member_identities_total: 11,
      qiwe_room_potential_member_identities_linked: 9,
      qiwe_room_potential_member_identities_unlinked: 2,
      qiwe_room_potential_member_identities_unlinked_samples: [
        {
          display_name: "[敏感数字]",
          identity_key: "abc123def456",
          reason: "potential_member_identity_unlinked",
        },
      ],
    })
  );
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /unsafe-display potential member identities/);
  assert.match(result.stderr, /abc123def456/);

  evidence = writeCase(
    "message-backfill-missing.json",
    validCoverage({
      linked_messages_missing_sender_person: 2,
      linked_messages_missing_sender_person_samples: [
        { display_name: "小乔", person_id: "223abda2-a6bd-46cc-abbe-ac6ff0b5fc2f" },
      ],
    })
  );
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /miss sender_person_id/);
  assert.match(result.stderr, /小乔/);

  evidence = writeCase(
    "platform-identity-missing.json",
    validCoverage({
      qiwe_platform_identity_materializable_users: 4,
      qiwe_platform_identities_missing: 1,
      qiwe_platform_identities_missing_samples: [
        {
          display_name: "小白君",
          person_id: "123e4567-e89b-12d3-a456-426614174000",
        },
      ],
    })
  );
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /missing platform identities/);
  assert.match(result.stderr, /小白君/);

  evidence = writeCase(
    "speaker-platform-identity-missing.json",
    validCoverage({
      linked_people_without_qiwe_platform_identity: 1,
    })
  );
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /speaker recognition/);

  evidence = writeCase("missing-canary-spec-array.json", {
    ...validCoverage(),
    answer_context_canary_specs: undefined,
  });
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /answer_context_canary_specs must be an array/);

  evidence = writeCase(
    "canary-spec-length-mismatch.json",
    validCoverage({
      answer_context_canary_specs_total: 5,
    })
  );
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /answer_context_canary_specs length/);

  evidence = writeCase(
    "speaker-canary-people-mismatch.json",
    validCoverage({
      answer_context_speaker_canary_specs: [
        ...speakerCanarySpecs().slice(0, 3),
        {
          id: 104,
          canary_type: "speaker_self",
          expected_speaker_label: "小乔备用",
          canonical_key: "person:xiaoqiao",
        },
      ],
    })
  );
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /speaker self-canary specs unique people count/);

  evidence = writeCase(
    "speaker-canary-set-mismatch.json",
    validCoverage({
      answer_context_speaker_canary_specs: [
        ...speakerCanarySpecs().slice(0, 3),
        {
          id: 104,
          canary_type: "speaker_self",
          expected_speaker_label: "错位成员",
          canonical_key: "person:wrong-member",
        },
      ],
    })
  );
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /same canonical people/);

  evidence = writeCase(
    "running-profile-hint-missing.json",
    validCoverage({
      linked_people_with_running_facts: 2,
      running_people_with_profile_running_hint: 1,
      running_people_profile_missing_running_hint: 1,
      running_people_profile_missing_running_hint_samples: [
        {
          display_name: "小乔",
          person_id: "223abda2-a6bd-46cc-abbe-ac6ff0b5fc2f",
          count: 4,
        },
      ],
    })
  );
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /running facts but no running profile hint/);
  assert.match(result.stderr, /小乔/);

  evidence = writeCase(
    "bootstrap-pending.json",
    validCoverage({
      total_channel_identities: 4,
      ambiguous_channel_identities_skipped: 1,
      unlinked_channel_identity_samples: [{ display_name: "新成员" }],
    })
  );
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /identity bootstrap apply is still required/);
  assert.match(result.stderr, /新成员/);

  evidence = writeCase(
    "missing-canary-name.json",
    validCoverage({
      linked_people_total: 4,
      answer_context_canary_people_total: 3,
      answer_context_speaker_canary_people_total: 3,
      linked_people_without_answer_context_canary_spec: 1,
      linked_people_without_answer_context_canary_spec_samples: [
        {
          display_name: "000",
          person_key: "fc2c1a46c0af",
          reason: "missing_safe_answer_context_canary_name",
        },
      ],
    })
  );
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /no safe answer-context canary name/);
  assert.match(result.stderr, /fc2c1a46c0af/);

  evidence = writeCase(
    "missing-speaker-self-canary.json",
    validCoverage({
      answer_context_speaker_canary_specs_total: 3,
      answer_context_speaker_canary_people_total: 3,
    })
  );
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /speaker self-canary specs/);

  evidence = writeCase("missing-field.json", {
    ...validCoverage(),
    qiwe_channel_identities_total: undefined,
  });
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /qiwe_channel_identities_total/);

  evidence = writeCase(
    "secret-leak.json",
    `${JSON.stringify(validCoverage())}\nDATABASE_URL=postgresql://example\n`
  );
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /forbidden sensitive fragment/);

  evidence = writeCase(
    "phone-leak.json",
    validCoverage({
      linked_aliases_missing_samples: [
        {
          display_name: "Joey17336786728",
          person_id: "123e4567-e89b-12d3-a456-426614174000",
        },
      ],
    })
  );
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /forbidden sensitive fragment/);
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Erhua member recognition coverage test passed.");

function runChecker(evidencePath, options = {}) {
  const args = [checker, evidencePath];
  if (options.requireActiveProfiles) {
    args.push("--require-active-profiles");
  }
  if (options.summaryOutput) {
    args.push("--summary-output", options.summaryOutput);
  }
  return spawnSync("node", args, {
    cwd: repoRoot,
    encoding: "utf8",
  });
}

function writeCase(name, content) {
  const evidencePath = path.join(tmpRoot, name);
  const text = typeof content === "string" ? content : JSON.stringify(content, null, 2);
  fs.writeFileSync(evidencePath, text, "utf8");
  return evidencePath;
}

function validCoverage(overrides = {}) {
  return {
    qiwe_channel_identities_raw_total: 12,
    qiwe_room_channel_identities_raw_total: 12,
    qiwe_room_channel_identities_total: 10,
    qiwe_room_channel_identities_linked: 9,
    qiwe_room_channel_identities_excluded: 2,
    qiwe_room_potential_member_identities_total: 10,
    qiwe_room_potential_member_identities_linked: 9,
    qiwe_room_potential_member_identities_unlinked: 1,
    total_channel_identities: 1,
    qiwe_channel_identities_total: 10,
    qiwe_channel_identities_linked: 9,
    qiwe_channel_identities_excluded: 2,
    channel_identities_with_existing_person: 0,
    channel_identities_with_existing_name: 0,
    ambiguous_channel_identities_skipped: 1,
    linked_aliases_missing: 0,
    linked_messages_missing_sender_person: 0,
    linked_people_total: 4,
    linked_people_with_active_profile: 3,
    linked_people_without_active_profile: 1,
    qiwe_platform_identity_materializable_users: 4,
    qiwe_platform_identities_missing: 0,
    qiwe_platform_identity_ambiguous_users: 0,
    linked_people_without_qiwe_platform_identity: 0,
    linked_people_with_running_facts: 0,
    running_people_with_profile_running_hint: 0,
    running_people_profile_missing_running_hint: 0,
    answer_context_canary_specs_total: 4,
    answer_context_canary_people_total: 4,
    answer_context_speaker_canary_specs_total: 4,
    answer_context_speaker_canary_people_total: 4,
    answer_context_referenced_canary_specs_total: 4,
    answer_context_referenced_canary_people_total: 4,
    linked_people_without_answer_context_canary_spec: 0,
    answer_context_canary_specs: mentionCanarySpecs(),
    answer_context_speaker_canary_specs: speakerCanarySpecs(),
    answer_context_referenced_canary_specs: referencedCanarySpecs(),
    dry_run: true,
    ambiguous_channel_identity_samples: [
      { display_name: "重名成员", reason: "multiple existing people" },
    ],
    linked_people_without_active_profile_samples: [{ display_name: "新朋友" }],
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
    {
      id: 13,
      canary_type: "mentioned_member",
      expected_mention: "Cici",
      canonical_key: "person:cici",
      required_profile_terms: ["跑步"],
    },
    {
      id: 14,
      canary_type: "mentioned_member",
      expected_mention: "新朋友",
      canonical_key: "person:new-friend",
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
    {
      id: 103,
      canary_type: "speaker_self",
      expected_speaker_label: "Cici",
      canonical_key: "person:cici",
      required_profile_terms: ["跑步"],
    },
    {
      id: 104,
      canary_type: "speaker_self",
      expected_speaker_label: "新朋友",
      canonical_key: "person:new-friend",
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
    {
      id: 203,
      canary_type: "referenced_member",
      expected_referenced_label: "Cici",
      canonical_key: "person:cici",
      required_profile_terms: ["跑步"],
    },
    {
      id: 204,
      canary_type: "referenced_member",
      expected_referenced_label: "新朋友",
      canonical_key: "person:new-friend",
    },
  ];
}
