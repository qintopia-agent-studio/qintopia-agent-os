#!/usr/bin/env node

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";

const repoRoot = process.cwd();
const builder = path.join(
  repoRoot,
  "tools/deploy/build-erhua-member-recognition-roster-audit.mjs"
);
const tmpRoot = fs.mkdtempSync(
  path.join(os.tmpdir(), "erhua-member-recognition-roster-audit-")
);
const ROOM_SCOPE =
  "sha256:c5c4e70d823efa23b83de70ce5008d746e76bdce54e37605b967b4bfd4036356";
const PERSON_PAXON = "223abda2-a6bd-46cc-abbe-ac6ff0b5fc2f";
const PERSON_CICI = "e8b16f22-4cf0-4e41-b87f-79b5e12494e2";
const PERSON_NEW_FRIEND = "b7f1b9f4-c7f2-4898-9c4a-91e1b85a9f6d";

try {
  let files = writeEvidence("valid", coverage(), canaries(), completionSummary());
  let outputPath = path.join(tmpRoot, "valid", "roster-audit.json");
  let result = runBuilder(files, outputPath);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /roster audit passed/);
  let audit = JSON.parse(fs.readFileSync(outputPath, "utf8"));
  assert.equal(audit.schema_version, "erhua_member_recognition_roster_audit_v1");
  assert.equal(audit.passed, true);
  assert.equal(audit.linked_people_total, 2);
  assert.equal(audit.audited_people_total, 2);
  const paxon = audit.people.find((person) => person.canonical_key === "person:paxon");
  assert.ok(paxon);
  assert.equal(paxon.person_ref, personRef(PERSON_PAXON));
  assert.deepEqual(paxon.mentioned_labels, ["Paxon", "小乔"]);
  assert.deepEqual(paxon.required_profile_terms_matched, ["running"]);
  assert.equal(paxon.profile_status, "stable_profile");
  assert.equal(paxon.mentioned_resolved, true);
  assert.equal(paxon.speaker_resolved, true);
  assert.equal(paxon.referenced_resolved, true);
  assert.doesNotMatch(JSON.stringify(audit), new RegExp(PERSON_PAXON, "i"));
  assert.doesNotMatch(JSON.stringify(audit), /"chat_id"\s*:/);
  assert.doesNotMatch(JSON.stringify(audit), /"sender_id"\s*:/);
  assert.doesNotMatch(JSON.stringify(audit), /"channel_user_id"\s*:/);

  files = writeEvidence(
    "missing-speaker",
    coverage(),
    canaries().filter(
      (record) =>
        record.canary_type !== "speaker_self" || record.canonical_key !== "person:cici"
    ),
    completionSummary()
  );
  outputPath = path.join(tmpRoot, "missing-speaker", "roster-audit.json");
  result = runBuilder(files, outputPath);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /missing_speaker_canary person:cici/);
  audit = JSON.parse(fs.readFileSync(outputPath, "utf8"));
  assert.equal(audit.passed, false);
  assert.ok(
    audit.gaps.some(
      (gap) =>
        gap.issue === "missing_speaker_canary" && gap.canonical_key === "person:cici"
    )
  );

  files = writeEvidence(
    "completion-mismatch",
    coverage(),
    canaries(),
    completionSummary({
      linked_people: {
        total: 3,
        with_active_profile: 2,
        without_active_profile: 1,
      },
    })
  );
  result = runBuilder(files, path.join(tmpRoot, "completion-mismatch", "audit.json"));
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /completion linked_people.total/);

  files = writeEvidence(
    "missing-required-term",
    coverage(),
    canaries().map((record) =>
      record.canonical_key === "person:paxon"
        ? withTargetHints(record, {
            safe_summary: "Paxon has safe profile context about collaboration.",
            safe_reply_hints: {
              topics: ["collaboration"],
              stable_profile_notes: ["collaboration"],
            },
          })
        : record
    ),
    completionSummary()
  );
  result = runBuilder(files, path.join(tmpRoot, "missing-required-term", "audit.json"));
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /missing_required_profile_terms person:paxon/);

  files = writeEvidence(
    "identity-only",
    coverage({
      linked_people_total: 1,
      linked_people_with_active_profile: 0,
      linked_people_without_active_profile: 1,
      answer_context_canary_specs_total: 1,
      answer_context_canary_people_total: 1,
      answer_context_speaker_canary_specs_total: 1,
      answer_context_speaker_canary_people_total: 1,
      answer_context_referenced_canary_specs_total: 1,
      answer_context_referenced_canary_people_total: 1,
      answer_context_canary_specs: [mentionSpec(11, "New Friend", "person:new-friend")],
      answer_context_speaker_canary_specs: [
        speakerSpec(101, "New Friend", "person:new-friend"),
      ],
      answer_context_referenced_canary_specs: [
        referencedSpec(201, "New Friend", "person:new-friend"),
      ],
    }),
    [
      identityOnlyCanary("mentioned_member", "expected_mention", "mentioned_members"),
      identityOnlyCanary("speaker_self", "expected_speaker_label", "speaker"),
      identityOnlyCanary(
        "referenced_member",
        "expected_referenced_label",
        "referenced_member"
      ),
    ],
    completionSummary({
      linked_people: {
        total: 1,
        with_active_profile: 0,
        without_active_profile: 1,
      },
      answer_context_canaries: {
        mentioned_records: 1,
        speaker_records: 1,
        referenced_records: 1,
        mentioned_people_resolved: 1,
        speaker_people_resolved: 1,
        referenced_people_resolved: 1,
        linked_people_resolved: 1,
      },
    })
  );
  outputPath = path.join(tmpRoot, "identity-only", "audit.json");
  result = runBuilder(files, outputPath);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /warning: identity_only_profile/);
  audit = JSON.parse(fs.readFileSync(outputPath, "utf8"));
  assert.equal(audit.people[0].profile_status, "identity_only");
  assert.ok(audit.gaps.some((gap) => gap.issue === "identity_only_profile"));

  result = runBuilder(files, path.join(tmpRoot, "identity-only", "strict.json"), {
    requireActiveProfiles: true,
  });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /identity_only_profile/);

  files = writeEvidence(
    "coverage-sample-person-id",
    coverage({
      linked_messages_missing_sender_person_samples: [
        {
          display_name: "Paxon",
          person_id: PERSON_PAXON,
          reason: "linked_messages_missing_sender_person",
        },
        {
          person_id: PERSON_CICI,
          display_name: "Cici",
          reason: "linked_messages_missing_sender_person",
        },
      ],
      running_people_profile_missing_running_hint_samples: [
        {
          display_name: "Paxon",
          person_id: PERSON_PAXON,
          reason: "running_facts_missing_profile_hint",
        },
      ],
    }),
    canaries(),
    completionSummary()
  );
  outputPath = path.join(tmpRoot, "coverage-sample-person-id", "audit.json");
  result = runBuilder(files, outputPath);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /roster audit passed/);
  audit = JSON.parse(fs.readFileSync(outputPath, "utf8"));
  assert.equal(audit.passed, true);
  assert.doesNotMatch(JSON.stringify(audit), new RegExp(PERSON_PAXON, "i"));

  files = writeEvidence("person-id-leak", coverage(), canaries(), completionSummary());
  const leakedCanaryPath = path.join(tmpRoot, "person-id-leak", "canary-leak.jsonl");
  fs.writeFileSync(
    leakedCanaryPath,
    `erhua_member_recognition_canary=${JSON.stringify(canaries()[0])}\n{"person_id":"${PERSON_PAXON}"}\n`,
    "utf8"
  );
  files.canary = leakedCanaryPath;
  result = runBuilder(files, path.join(tmpRoot, "person-id-leak", "audit.json"));
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /forbidden sensitive fragment/);
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Erhua member recognition roster audit test passed.");

function runBuilder(files, outputPath, options = {}) {
  const args = [
    builder,
    "--coverage",
    files.coverage,
    "--canary",
    files.canary,
    "--completion-summary",
    files.completionSummary,
    "--output",
    outputPath,
  ];
  if (options.requireActiveProfiles) {
    args.push("--require-active-profiles");
  }
  return spawnSync("node", args, {
    cwd: repoRoot,
    encoding: "utf8",
  });
}

function writeEvidence(name, coverageContent, canaryContent, summaryContent) {
  const dir = path.join(tmpRoot, name);
  fs.mkdirSync(dir, { recursive: true });
  const coveragePath = path.join(dir, "coverage.json");
  const canaryPath = path.join(dir, "canary.jsonl");
  const completionSummaryPath = path.join(dir, "completion-summary.json");
  fs.writeFileSync(coveragePath, JSON.stringify(coverageContent, null, 2), "utf8");
  fs.writeFileSync(
    canaryPath,
    canaryContent
      .map((record) => `erhua_member_recognition_canary=${JSON.stringify(record)}`)
      .join("\n")
      .concat("\n"),
    "utf8"
  );
  fs.writeFileSync(
    completionSummaryPath,
    JSON.stringify(summaryContent, null, 2),
    "utf8"
  );
  return {
    coverage: coveragePath,
    canary: canaryPath,
    completionSummary: completionSummaryPath,
  };
}

function coverage(overrides = {}) {
  return {
    scope_fingerprint: ROOM_SCOPE,
    linked_people_total: 2,
    linked_people_with_active_profile: 2,
    linked_people_without_active_profile: 0,
    linked_people_without_answer_context_canary_spec: 0,
    answer_context_canary_specs_total: 3,
    answer_context_canary_people_total: 2,
    answer_context_speaker_canary_specs_total: 2,
    answer_context_speaker_canary_people_total: 2,
    answer_context_referenced_canary_specs_total: 2,
    answer_context_referenced_canary_people_total: 2,
    answer_context_canary_specs: [
      mentionSpec(11, "Paxon", "person:paxon", ["running"]),
      mentionSpec(12, "小乔", "person:paxon", ["running"]),
      mentionSpec(13, "Cici", "person:cici"),
    ],
    answer_context_speaker_canary_specs: [
      speakerSpec(101, "Paxon", "person:paxon", ["running"]),
      speakerSpec(102, "Cici", "person:cici"),
    ],
    answer_context_referenced_canary_specs: [
      referencedSpec(201, "Paxon", "person:paxon", ["running"]),
      referencedSpec(202, "Cici", "person:cici"),
    ],
    ...overrides,
  };
}

function completionSummary(overrides = {}) {
  return {
    schema_version: "erhua_member_recognition_completion_v1",
    passed: true,
    scope_fingerprint: ROOM_SCOPE,
    current_room_qiwe_identities: {
      unsafe_display_unlinked: 0,
    },
    linked_people: {
      total: 2,
      with_active_profile: 2,
      without_active_profile: 0,
    },
    answer_context_canaries: {
      mentioned_records: 3,
      speaker_records: 2,
      referenced_records: 2,
      mentioned_people_resolved: 2,
      speaker_people_resolved: 2,
      referenced_people_resolved: 2,
      linked_people_resolved: 2,
    },
    ...overrides,
  };
}

function mentionSpec(id, label, canonicalKey, requiredTerms = []) {
  return {
    id,
    canary_type: "mentioned_member",
    expected_mention: label,
    canonical_key: canonicalKey,
    required_profile_terms: requiredTerms,
  };
}

function speakerSpec(id, label, canonicalKey, requiredTerms = []) {
  return {
    id,
    canary_type: "speaker_self",
    expected_speaker_label: label,
    canonical_key: canonicalKey,
    required_profile_terms: requiredTerms,
  };
}

function referencedSpec(id, label, canonicalKey, requiredTerms = []) {
  return {
    id,
    canary_type: "referenced_member",
    expected_referenced_label: label,
    canonical_key: canonicalKey,
    required_profile_terms: requiredTerms,
  };
}

function canaries() {
  return [
    canary("mentioned_member", "expected_mention", "mentioned_members", {
      label: "Paxon",
      canonicalKey: "person:paxon",
      personId: PERSON_PAXON,
      requiredTerms: ["running"],
    }),
    canary("mentioned_member", "expected_mention", "mentioned_members", {
      label: "小乔",
      canonicalKey: "person:paxon",
      personId: PERSON_PAXON,
      requiredTerms: ["running"],
    }),
    canary("mentioned_member", "expected_mention", "mentioned_members", {
      label: "Cici",
      canonicalKey: "person:cici",
      personId: PERSON_CICI,
    }),
    canary("speaker_self", "expected_speaker_label", "speaker", {
      label: "Paxon",
      canonicalKey: "person:paxon",
      personId: PERSON_PAXON,
      requiredTerms: ["running"],
    }),
    canary("speaker_self", "expected_speaker_label", "speaker", {
      label: "Cici",
      canonicalKey: "person:cici",
      personId: PERSON_CICI,
    }),
    canary("referenced_member", "expected_referenced_label", "referenced_member", {
      label: "Paxon",
      canonicalKey: "person:paxon",
      personId: PERSON_PAXON,
      requiredTerms: ["running"],
    }),
    canary("referenced_member", "expected_referenced_label", "referenced_member", {
      label: "Cici",
      canonicalKey: "person:cici",
      personId: PERSON_CICI,
    }),
  ];
}

function canary(canaryType, labelField, targetField, options) {
  const target = activeTarget(options.label, options.personId, options.requiredTerms);
  return {
    canary_type: canaryType,
    [labelField]: options.label,
    canonical_key: options.canonicalKey,
    required_profile_terms: options.requiredTerms ?? [],
    answer_context:
      targetField === "mentioned_members"
        ? { success: true, mentioned_members: [target] }
        : { success: true, [targetField]: target, mentioned_members: [] },
  };
}

function activeTarget(label, personId, requiredTerms = []) {
  return {
    resolved: true,
    resolution_status: "resolved",
    resolution_scope: "exact_chat",
    mention_text: label,
    match_count: 1,
    display_name: label,
    person_ref: personRef(personId),
    safe_summary: `${label} has safe profile context about running and collaboration.`,
    safe_reply_hints: {
      topics: requiredTerms.length > 0 ? requiredTerms : ["collaboration"],
      stable_profile_notes:
        requiredTerms.length > 0 ? ["running activity"] : ["collaboration"],
    },
  };
}

function identityOnlyCanary(canaryType, labelField, targetField) {
  const target = {
    resolved: true,
    resolution_status: "resolved",
    resolution_scope: "exact_chat",
    mention_text: "New Friend",
    match_count: 1,
    display_name: "New Friend",
    person_ref: personRef(PERSON_NEW_FRIEND),
    safe_summary:
      "New Friend is identified as a room member, but no stable profile is available.",
    safe_reply_hints: {
      profile_status: "identity_only",
      topics: [],
      stable_profile_notes: [],
      do_not_infer_missing_profile: true,
    },
  };
  return {
    canary_type: canaryType,
    [labelField]: "New Friend",
    canonical_key: "person:new-friend",
    answer_context:
      targetField === "mentioned_members"
        ? { success: true, mentioned_members: [target] }
        : { success: true, [targetField]: target, mentioned_members: [] },
  };
}

function withTargetHints(record, overrides) {
  const answerContext = structuredClone(record.answer_context);
  if (record.canary_type === "mentioned_member") {
    answerContext.mentioned_members = answerContext.mentioned_members.map((member) => ({
      ...member,
      ...overrides,
    }));
  } else if (record.canary_type === "speaker_self") {
    answerContext.speaker = {
      ...answerContext.speaker,
      ...overrides,
    };
  } else {
    answerContext.referenced_member = {
      ...answerContext.referenced_member,
      ...overrides,
    };
  }
  return {
    ...record,
    answer_context: answerContext,
  };
}

function personRef(personId) {
  return `sha256:${createHash("sha256")
    .update(`erhua-member-recognition-person-ref-v1:${personId.toLowerCase()}`)
    .digest("hex")}`;
}
