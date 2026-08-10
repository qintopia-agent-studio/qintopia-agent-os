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
  "tools/deploy/build-erhua-member-recognition-canary-evidence.mjs"
);
const checker = path.join(
  repoRoot,
  "tools/deploy/check-erhua-member-recognition-canary.mjs"
);
const tmpRoot = fs.mkdtempSync(
  path.join(os.tmpdir(), "erhua-member-recognition-canary-builder-")
);
const xiaoqiaoPersonId = "223abda2-a6bd-46cc-abbe-ac6ff0b5fc2f";

try {
  const specPath = path.join(tmpRoot, "spec.json");
  const mcpPath = path.join(tmpRoot, "mcp.jsonl");
  const outputPath = path.join(tmpRoot, "canary.jsonl");
  fs.writeFileSync(
    specPath,
    JSON.stringify({
      answer_context_canary_specs_total: 2,
      answer_context_canary_people_total: 1,
      answer_context_speaker_canary_specs_total: 1,
      answer_context_speaker_canary_people_total: 1,
      answer_context_referenced_canary_specs_total: 1,
      answer_context_referenced_canary_people_total: 1,
      answer_context_canary_specs: [
        {
          id: 11,
          canary_type: "mentioned_member",
          expected_mention: "小乔",
          canonical_key: "xiaoqiao",
          required_profile_terms: ["跑步"],
        },
        {
          id: 12,
          canary_type: "mentioned_member",
          expected_mention: "Paxon",
          canonical_key: "xiaoqiao",
          required_profile_terms: ["跑步"],
        },
      ],
      answer_context_speaker_canary_specs: [
        {
          id: 1000011,
          canary_type: "speaker_self",
          expected_speaker_label: "小乔",
          canonical_key: "xiaoqiao",
          required_profile_terms: ["跑步"],
        },
      ],
      answer_context_referenced_canary_specs: [
        {
          id: 2000011,
          canary_type: "referenced_member",
          expected_referenced_label: "小乔",
          canonical_key: "xiaoqiao",
          required_profile_terms: ["跑步"],
        },
      ],
    }),
    "utf8"
  );
  fs.writeFileSync(
    mcpPath,
    [
      JSON.stringify(mcpMessage(11, "小乔", xiaoqiaoPersonId)),
      JSON.stringify({
        jsonrpc: "2.0",
        id: 99,
        result: { content: [{ type: "text", text: "{}" }] },
      }),
      JSON.stringify(mcpMessage(12, "Paxon", xiaoqiaoPersonId)),
      JSON.stringify(speakerMcpMessage(1000011, xiaoqiaoPersonId)),
      JSON.stringify(referencedMcpMessage(2000011, xiaoqiaoPersonId)),
      "",
    ].join("\n"),
    "utf8"
  );

  let result = runBuilder(specPath, mcpPath, outputPath);
  assert.equal(result.status, 0, result.stderr);
  const built = fs.readFileSync(outputPath, "utf8");
  assert.match(built, /erhua_member_recognition_canary=/);
  assert.doesNotMatch(built, /chat_id|sender_id|raw_messages/);
  assert.doesNotMatch(built, /"person_id"\s*:/);
  assert.doesNotMatch(built, new RegExp(xiaoqiaoPersonId, "i"));
  assert.match(built, new RegExp(personRef(xiaoqiaoPersonId)));
  assert.doesNotMatch(built, /17336786728/);
  assert.match(built, /Paxon/);
  assert.match(built, /speaker_self/);
  assert.match(built, /referenced_member/);

  result = spawnSync("node", [checker, outputPath], {
    cwd: repoRoot,
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr);

  const missingOutput = path.join(tmpRoot, "missing.jsonl");
  fs.writeFileSync(
    missingOutput,
    JSON.stringify(mcpMessage(11, "小乔", xiaoqiaoPersonId)),
    "utf8"
  );
  result = runBuilder(specPath, missingOutput, null);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /missing MCP answer_context response/);

  const duplicateSpecPath = path.join(tmpRoot, "duplicate-spec.json");
  fs.writeFileSync(
    duplicateSpecPath,
    JSON.stringify({
      canaries: [
        {
          id: 11,
          canary_type: "mentioned_member",
          expected_mention: "小乔",
        },
        {
          id: 11,
          canary_type: "mentioned_member",
          expected_mention: "Paxon",
        },
      ],
    }),
    "utf8"
  );
  result = runBuilder(duplicateSpecPath, mcpPath, null);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /duplicate canary spec id/);

  const duplicateMcpOutput = path.join(tmpRoot, "duplicate-mcp.jsonl");
  fs.writeFileSync(
    duplicateMcpOutput,
    [
      JSON.stringify(mcpMessage(11, "小乔", xiaoqiaoPersonId)),
      JSON.stringify(mcpMessage(11, "小乔", xiaoqiaoPersonId)),
      JSON.stringify(mcpMessage(12, "Paxon", xiaoqiaoPersonId)),
      JSON.stringify(speakerMcpMessage(1000011, xiaoqiaoPersonId)),
      JSON.stringify(referencedMcpMessage(2000011, xiaoqiaoPersonId)),
      "",
    ].join("\n"),
    "utf8"
  );
  result = runBuilder(specPath, duplicateMcpOutput, null);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /duplicate MCP answer_context response/);

  const partialSpecPath = path.join(tmpRoot, "partial-spec.json");
  fs.writeFileSync(
    partialSpecPath,
    JSON.stringify({
      answer_context_canary_specs_total: 2,
      answer_context_canary_specs: [
        {
          id: 11,
          canary_type: "mentioned_member",
          expected_mention: "小乔",
          canonical_key: "xiaoqiao",
        },
      ],
      answer_context_speaker_canary_specs: [
        {
          id: 1000011,
          canary_type: "speaker_self",
          expected_speaker_label: "小乔",
          canonical_key: "xiaoqiao",
        },
      ],
      answer_context_referenced_canary_specs: [
        {
          id: 2000011,
          canary_type: "referenced_member",
          expected_referenced_label: "小乔",
          canonical_key: "xiaoqiao",
        },
      ],
    }),
    "utf8"
  );
  result = runBuilder(partialSpecPath, mcpPath, null);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /answer_context_canary_specs length/);

  const peopleMismatchSpecPath = path.join(tmpRoot, "people-mismatch-spec.json");
  fs.writeFileSync(
    peopleMismatchSpecPath,
    JSON.stringify({
      answer_context_canary_people_total: 2,
      answer_context_canary_specs: [
        {
          id: 11,
          canary_type: "mentioned_member",
          expected_mention: "小乔",
          canonical_key: "xiaoqiao",
        },
        {
          id: 12,
          canary_type: "mentioned_member",
          expected_mention: "Paxon",
          canonical_key: "xiaoqiao",
        },
      ],
      answer_context_speaker_canary_specs: [
        {
          id: 1000011,
          canary_type: "speaker_self",
          expected_speaker_label: "小乔",
          canonical_key: "xiaoqiao",
        },
      ],
      answer_context_referenced_canary_specs: [
        {
          id: 2000011,
          canary_type: "referenced_member",
          expected_referenced_label: "小乔",
          canonical_key: "xiaoqiao",
        },
      ],
    }),
    "utf8"
  );
  result = runBuilder(peopleMismatchSpecPath, mcpPath, null);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /unique people/);

  const missingRouteSpecPath = path.join(tmpRoot, "missing-route-spec.json");
  fs.writeFileSync(
    missingRouteSpecPath,
    JSON.stringify({
      answer_context_canary_specs: [
        {
          id: 11,
          canary_type: "mentioned_member",
          expected_mention: "小乔",
          canonical_key: "xiaoqiao",
        },
      ],
    }),
    "utf8"
  );
  result = runBuilder(missingRouteSpecPath, mcpPath, null);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /speaker self-canary records/);

  const routeMismatchSpecPath = path.join(tmpRoot, "route-mismatch-spec.json");
  fs.writeFileSync(
    routeMismatchSpecPath,
    JSON.stringify({
      answer_context_canary_specs: [
        {
          id: 11,
          canary_type: "mentioned_member",
          expected_mention: "小乔",
          canonical_key: "xiaoqiao",
        },
      ],
      answer_context_speaker_canary_specs: [
        {
          id: 1000011,
          canary_type: "speaker_self",
          expected_speaker_label: "小乔",
          canonical_key: "other",
        },
      ],
      answer_context_referenced_canary_specs: [
        {
          id: 2000011,
          canary_type: "referenced_member",
          expected_referenced_label: "小乔",
          canonical_key: "xiaoqiao",
        },
      ],
    }),
    "utf8"
  );
  result = runBuilder(routeMismatchSpecPath, mcpPath, null);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /same canonical people/);
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Erhua member recognition canary builder test passed.");

function runBuilder(specPath, mcpPath, outputPath) {
  const args = [builder, "--spec", specPath, "--mcp-output", mcpPath];
  if (outputPath) {
    args.push("--output", outputPath);
  }
  return spawnSync("node", args, {
    cwd: repoRoot,
    encoding: "utf8",
  });
}

function mcpMessage(id, mentionText, personId) {
  const displayName = mentionText === "Paxon" ? "小乔" : mentionText;
  return {
    jsonrpc: "2.0",
    id,
    result: {
      content: [
        {
          type: "text",
          text: JSON.stringify({
            success: true,
            speaker: {
              resolved: false,
              resolution_scope: "unresolved",
            },
            mentioned_members: [
              {
                mention_text: mentionText,
                resolved: true,
                resolution_status: "resolved",
                match_count: 1,
                display_name: displayName,
                person_id: personId,
                safe_summary:
                  "小乔17336786728 最近的安全画像：多次出现跑步活动相关信号，可自然理解为和社区跑步活动联系较多。",
                safe_reply_hints: {
                  topics: ["跑步活动"],
                  stable_profile_notes: [
                    "多次出现跑步活动相关信号，可自然理解为和社区跑步活动联系较多，勿露 17336786728",
                  ],
                },
              },
            ],
            answer_rules: {
              do_not_expose_raw_history: true,
            },
          }),
        },
      ],
      isError: false,
    },
  };
}

function speakerMcpMessage(id, personId) {
  return {
    jsonrpc: "2.0",
    id,
    result: {
      content: [
        {
          type: "text",
          text: JSON.stringify({
            success: true,
            speaker: {
              resolved: true,
              resolution_scope: "exact_chat",
              display_name: "小乔17336786728",
              person_id: personId,
              safe_summary:
                "小乔17336786728 最近的安全画像：多次出现跑步活动相关信号，可自然理解为和社区跑步活动联系较多。",
              safe_reply_hints: {
                topics: ["跑步活动"],
                stable_profile_notes: [
                  "多次出现跑步活动相关信号，可自然理解为和社区跑步活动联系较多，勿露 17336786728",
                ],
              },
            },
            mentioned_members: [],
            answer_rules: {
              do_not_expose_raw_history: true,
            },
          }),
        },
      ],
      isError: false,
    },
  };
}

function referencedMcpMessage(id, personId) {
  return {
    jsonrpc: "2.0",
    id,
    result: {
      content: [
        {
          type: "text",
          text: JSON.stringify({
            success: true,
            speaker: {
              resolved: false,
              resolution_scope: "unresolved",
            },
            referenced_member: {
              resolved: true,
              resolution_scope: "exact_chat",
              display_name: "小乔17336786728",
              person_id: personId,
              safe_summary:
                "小乔17336786728 最近的安全画像：多次出现跑步活动相关信号，可自然理解为和社区跑步活动联系较多。",
              safe_reply_hints: {
                topics: ["跑步活动"],
                stable_profile_notes: [
                  "多次出现跑步活动相关信号，可自然理解为和社区跑步活动联系较多，勿露 17336786728",
                ],
              },
            },
            mentioned_members: [],
            answer_rules: {
              do_not_expose_raw_history: true,
            },
          }),
        },
      ],
      isError: false,
    },
  };
}

function personRef(personId) {
  return `sha256:${createHash("sha256")
    .update(`erhua-member-recognition-person-ref-v1:${personId.toLowerCase()}`)
    .digest("hex")}`;
}
