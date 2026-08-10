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
  "tools/deploy/check-erhua-member-recognition-canary.mjs"
);
const tmpRoot = fs.mkdtempSync(
  path.join(os.tmpdir(), "erhua-member-recognition-canary-")
);
const xiaoqiaoPersonId = "223abda2-a6bd-46cc-abbe-ac6ff0b5fc2f";
const ciciPersonId = "e8b16f22-4cf0-4e41-b87f-79b5e12494e2";
const newFriendPersonId = "b7f1b9f4-c7f2-4898-9c4a-91e1b85a9f6d";

try {
  let evidence = writeCase("valid.jsonl", validJsonl());
  let result = runChecker(evidence);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /canary check passed/);
  assert.match(result.stdout, /7 records/);
  assert.match(result.stdout, /3 mentioned, 2 speaker, 2 referenced/);

  evidence = writeCase("valid-array.json", JSON.stringify(validRecords(), null, 2));
  result = runChecker(evidence);
  assert.equal(result.status, 0, result.stderr);

  evidence = writeCase(
    "valid-person-ref-phone-like-hash.json",
    JSON.stringify(validRecords().map(withPhoneLikePersonRef), null, 2)
  );
  result = runChecker(evidence);
  assert.equal(result.status, 0, result.stderr);

  evidence = writeCase(
    "valid-identity-only.json",
    JSON.stringify([
      ...validRecords(),
      identityOnlyRecord("新朋友", newFriendPersonId),
      identityOnlySpeakerRecord("新朋友", "new-friend", newFriendPersonId),
      identityOnlyReferencedRecord("新朋友", "new-friend", newFriendPersonId),
    ])
  );
  result = runChecker(evidence);
  assert.equal(result.status, 0, result.stderr);

  evidence = writeCase(
    "mcp-response.json",
    JSON.stringify([
      {
        expected_mention: "Paxon",
        canonical_key: "xiaoqiao",
        required_profile_terms: ["跑步"],
        mcp_response: mcpResponse(answerContext("Paxon", "小乔", xiaoqiaoPersonId)),
      },
      speakerRecord("小乔", "xiaoqiao", xiaoqiaoPersonId, ["跑步"]),
      referencedRecord("小乔", "xiaoqiao", xiaoqiaoPersonId, ["跑步"]),
    ])
  );
  result = runChecker(evidence);
  assert.equal(result.status, 0, result.stderr);

  evidence = writeCase(
    "mention-only.json",
    JSON.stringify([record("Paxon", "xiaoqiao", xiaoqiaoPersonId, ["跑步"])])
  );
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /speaker self-canary records/);

  evidence = writeCase(
    "unresolved.json",
    JSON.stringify([
      {
        expected_mention: "Paxon",
        answer_context: {
          success: true,
          mentioned_members: [
            {
              mention_text: "Paxon",
              resolved: false,
              resolution_status: "unresolved",
              match_count: 0,
            },
          ],
        },
      },
    ])
  );
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /did not resolve/);

  evidence = writeCase(
    "mentioned-speaker-set-mismatch.json",
    JSON.stringify([...validRecords(), identityOnlyRecord("新朋友", newFriendPersonId)])
  );
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /resolve the same people/);

  evidence = writeCase(
    "profile-hint-set-mismatch.json",
    JSON.stringify([
      ...validRecords(),
      identityOnlyRecord("新朋友", newFriendPersonId),
      speakerRecord("新朋友", "new-friend", newFriendPersonId, []),
      referencedRecord("新朋友", "new-friend", newFriendPersonId, []),
    ])
  );
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /profile hint evidence must cover the same people/);

  evidence = writeCase(
    "empty-profile-hints.json",
    JSON.stringify([
      record("小乔", "xiaoqiao", xiaoqiaoPersonId, [], {
        safe_summary: "小乔 最近的安全上下文已识别。",
        safe_reply_hints: {
          topics: [],
          stable_profile_notes: [],
          temporary_communication_notes: [],
        },
      }),
      speakerRecord("小乔", "xiaoqiao", xiaoqiaoPersonId, [], {
        safe_summary: "小乔 最近的安全上下文已识别。",
        safe_reply_hints: {
          topics: [],
          stable_profile_notes: [],
          temporary_communication_notes: [],
        },
      }),
      referencedRecord("小乔", "xiaoqiao", xiaoqiaoPersonId, [], {
        safe_summary: "小乔 最近的安全上下文已识别。",
        safe_reply_hints: {
          topics: [],
          stable_profile_notes: [],
          temporary_communication_notes: [],
        },
      }),
    ])
  );
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /missing non-empty safe profile hints/);

  evidence = writeCase(
    "alias-split.json",
    JSON.stringify([
      record("小乔", "xiaoqiao", xiaoqiaoPersonId, ["跑步"]),
      record("Paxon", "xiaoqiao", ciciPersonId, ["跑步"]),
    ])
  );
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /canonical key xiaoqiao/);

  evidence = writeCase(
    "missing-running.json",
    JSON.stringify([
      record("Paxon", "xiaoqiao", xiaoqiaoPersonId, ["跑步"], {
        safe_summary: "小乔 最近的安全上下文主要与 活动 有关。",
        safe_reply_hints: { topics: ["活动"] },
      }),
    ])
  );
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /missing required term "跑步"/);

  evidence = writeCase(
    "speaker-missing-running.json",
    JSON.stringify([
      speakerRecord("小乔", "xiaoqiao", xiaoqiaoPersonId, ["跑步"], {
        safe_summary: "小乔 最近的安全上下文主要与 活动 有关。",
        safe_reply_hints: { topics: ["活动"] },
      }),
    ])
  );
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /missing required term "跑步"/);

  evidence = writeCase(
    "speaker-missing.json",
    JSON.stringify([
      {
        canary_type: "speaker_self",
        expected_speaker_label: "小乔",
        canonical_key: "xiaoqiao",
        answer_context: {
          success: true,
          mentioned_members: [],
        },
      },
    ])
  );
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /speaker was not returned/);

  evidence = writeCase(
    "mention-text-mismatch.json",
    JSON.stringify([
      record("小乔", "xiaoqiao", xiaoqiaoPersonId, ["跑步"], {
        mention_text: "乔",
        display_name: "小乔",
      }),
    ])
  );
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /mentioned member was not returned/);

  evidence = writeCase(
    "resolved-match-count-not-unique.json",
    JSON.stringify([
      record("小乔", "xiaoqiao", xiaoqiaoPersonId, ["跑步"], {
        match_count: 2,
      }),
    ])
  );
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /match_count=1/);

  evidence = writeCase(
    "missing-safe-interest-term.json",
    JSON.stringify([
      record("小乔", "xiaoqiao", xiaoqiaoPersonId, ["摄影", "AI", "写作"], {
        safe_summary:
          "小乔 最近的安全画像：多次表达与 AI、摄影 相关的兴趣、技能或可提供帮助。",
        safe_reply_hints: {
          topics: ["兴趣技能"],
          stable_profile_notes: ["多次表达与 AI、摄影 相关的兴趣、技能或可提供帮助"],
        },
      }),
    ])
  );
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /missing required term "写作"/);

  evidence = writeCase(
    "identity-only-missing-do-not-infer.json",
    JSON.stringify([
      record("新朋友", "new-friend", newFriendPersonId, [], {
        safe_summary: "新朋友 已识别为群内成员，但暂无足够稳定的安全画像。",
        safe_reply_hints: {
          profile_status: "identity_only",
          topics: [],
          stable_profile_notes: [],
        },
      }),
    ])
  );
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /do_not_infer_missing_profile=true/);

  evidence = writeCase(
    "secret-leak.json",
    `${validJsonl()}\nDATABASE_URL=postgresql://example\n`
  );
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /forbidden sensitive fragment/);

  evidence = writeCase(
    "person-id-leak.json",
    `${validJsonl()}\n{"person_id":"${xiaoqiaoPersonId}"}\n`
  );
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /forbidden sensitive fragment/);

  evidence = writeCase(
    "phone-leak.json",
    JSON.stringify([
      record("Joey", "joey", xiaoqiaoPersonId, [], {
        display_name: "Joey17336786728",
        safe_summary: "Joey17336786728 最近的安全上下文主要与 活动 有关。",
      }),
    ])
  );
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /forbidden sensitive fragment/);
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Erhua member recognition canary test passed.");

function runChecker(evidencePath) {
  return spawnSync("node", [checker, evidencePath], {
    cwd: repoRoot,
    encoding: "utf8",
  });
}

function writeCase(name, content) {
  const evidencePath = path.join(tmpRoot, name);
  fs.writeFileSync(evidencePath, content, "utf8");
  return evidencePath;
}

function validJsonl() {
  return (
    validRecords()
      .map((item) => `erhua_member_recognition_canary=${JSON.stringify(item)}`)
      .join("\n") + "\n"
  );
}

function validRecords() {
  return [
    record("小乔", "xiaoqiao", xiaoqiaoPersonId, ["跑步"]),
    record("Paxon", "xiaoqiao", xiaoqiaoPersonId, ["跑步"]),
    record("Cici", "cici", ciciPersonId, []),
    speakerRecord("小乔", "xiaoqiao", xiaoqiaoPersonId, ["跑步"]),
    speakerRecord("Cici", "cici", ciciPersonId, []),
    referencedRecord("小乔", "xiaoqiao", xiaoqiaoPersonId, ["跑步"]),
    referencedRecord("Cici", "cici", ciciPersonId, []),
  ];
}

function identityOnlyRecord(name, personId) {
  return record(name, "new-friend", personId, [], {
    safe_summary: `${name} 已识别为群内成员，但暂无足够稳定的安全画像。`,
    safe_reply_hints: {
      profile_status: "identity_only",
      topics: [],
      stable_profile_notes: [],
      do_not_infer_missing_profile: true,
    },
  });
}

function identityOnlySpeakerRecord(label, canonicalKey, personId) {
  return speakerRecord(label, canonicalKey, personId, [], {
    safe_summary: `${label} 已识别为群内成员，但暂无足够稳定的安全画像。`,
    safe_reply_hints: {
      profile_status: "identity_only",
      topics: [],
      stable_profile_notes: [],
      do_not_infer_missing_profile: true,
    },
  });
}

function identityOnlyReferencedRecord(label, canonicalKey, personId) {
  return referencedRecord(label, canonicalKey, personId, [], {
    safe_summary: `${label} 已识别为群内成员，但暂无足够稳定的安全画像。`,
    safe_reply_hints: {
      profile_status: "identity_only",
      topics: [],
      stable_profile_notes: [],
      do_not_infer_missing_profile: true,
    },
  });
}

function record(name, canonicalKey, personId, requiredTerms, overrides = {}) {
  return {
    canary_type: "mentioned_member",
    expected_mention: name,
    canonical_key: canonicalKey,
    required_profile_terms: requiredTerms,
    answer_context: answerContext(name, displayName(name), personId, overrides),
  };
}

function speakerRecord(label, canonicalKey, personId, requiredTerms, overrides = {}) {
  const displayNameValue = displayName(label);
  return {
    canary_type: "speaker_self",
    expected_speaker_label: label,
    canonical_key: canonicalKey,
    required_profile_terms: requiredTerms,
    answer_context: {
      success: true,
      speaker: {
        resolved: true,
        resolution_scope: overrides.resolution_scope ?? "exact_chat",
        display_name: overrides.display_name ?? displayNameValue,
        person_ref: personRef(personId),
        safe_summary:
          overrides.safe_summary ??
          `${displayNameValue} 最近的安全画像：多次出现跑步活动相关信号，可自然理解为和社区跑步活动联系较多。`,
        safe_reply_hints: overrides.safe_reply_hints ?? {
          topics: ["跑步活动"],
          stable_profile_notes: [
            "多次出现跑步活动相关信号，可自然理解为和社区跑步活动联系较多",
          ],
        },
      },
      mentioned_members: [],
    },
  };
}

function referencedRecord(
  label,
  canonicalKey,
  personId,
  requiredTerms,
  overrides = {}
) {
  const displayNameValue = displayName(label);
  return {
    canary_type: "referenced_member",
    expected_referenced_label: label,
    canonical_key: canonicalKey,
    required_profile_terms: requiredTerms,
    answer_context: {
      success: true,
      referenced_member: {
        resolved: true,
        resolution_scope: overrides.resolution_scope ?? "exact_chat",
        display_name: overrides.display_name ?? displayNameValue,
        person_ref: personRef(personId),
        safe_summary:
          overrides.safe_summary ??
          `${displayNameValue} 最近的安全画像：多次出现跑步活动相关信号，可自然理解为和社区跑步活动联系较多。`,
        safe_reply_hints: overrides.safe_reply_hints ?? {
          topics: ["跑步活动"],
          stable_profile_notes: [
            "多次出现跑步活动相关信号，可自然理解为和社区跑步活动联系较多",
          ],
        },
      },
      mentioned_members: [],
    },
  };
}

function answerContext(name, displayNameValue, personId, overrides = {}) {
  return {
    success: true,
    mentioned_members: [
      {
        mention_text: overrides.mention_text ?? name,
        resolved: true,
        resolution_status: "resolved",
        match_count: overrides.match_count ?? 1,
        display_name: overrides.display_name ?? displayNameValue,
        person_ref: personRef(personId),
        safe_summary:
          overrides.safe_summary ??
          `${displayNameValue} 最近的安全画像：多次出现跑步活动相关信号，可自然理解为和社区跑步活动联系较多。`,
        safe_reply_hints: overrides.safe_reply_hints ?? {
          topics: ["跑步活动"],
          stable_profile_notes: [
            "多次出现跑步活动相关信号，可自然理解为和社区跑步活动联系较多",
          ],
        },
      },
    ],
  };
}

function mcpResponse(context) {
  return {
    jsonrpc: "2.0",
    id: 2,
    result: {
      content: [
        {
          type: "text",
          text: JSON.stringify(context),
        },
      ],
      isError: false,
    },
  };
}

function displayName(name) {
  if (name === "Paxon") {
    return "小乔";
  }
  if (name === "Cici") {
    return "Cici（27-29止语）";
  }
  return name;
}

function personRef(personId) {
  return `sha256:${createHash("sha256")
    .update(`erhua-member-recognition-person-ref-v1:${personId.toLowerCase()}`)
    .digest("hex")}`;
}

function withPhoneLikePersonRef(record) {
  const next = JSON.parse(JSON.stringify(record));
  const replacement = "sha256:17336786728" + "a".repeat(53);
  if (next.answer_context?.speaker) {
    next.answer_context.speaker.person_ref = replacement;
  }
  if (next.answer_context?.referenced_member) {
    next.answer_context.referenced_member.person_ref = replacement;
  }
  for (const member of next.answer_context?.mentioned_members ?? []) {
    member.person_ref = replacement;
  }
  return next;
}
