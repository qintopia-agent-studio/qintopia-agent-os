#!/usr/bin/env node

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const builder = path.join(
  repoRoot,
  "tools/deploy/build-erhua-member-recognition-canary-mcp-input.mjs"
);
const tmpRoot = fs.mkdtempSync(
  path.join(os.tmpdir(), "erhua-member-recognition-canary-mcp-")
);
const scopeFingerprint =
  "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

try {
  const specPath = path.join(tmpRoot, "identity-bootstrap-dry-run.json");
  const speakerMapPath = path.join(tmpRoot, "private-speaker-map.json");
  const outputPath = path.join(tmpRoot, "mcp-input.jsonl");
  fs.writeFileSync(
    specPath,
    JSON.stringify({
      scope_fingerprint: scopeFingerprint,
      answer_context_canary_specs_total: 2,
      answer_context_canary_people_total: 1,
      answer_context_speaker_canary_specs_total: 1,
      answer_context_speaker_canary_people_total: 1,
      answer_context_referenced_canary_specs_total: 1,
      answer_context_referenced_canary_people_total: 1,
      answer_context_canary_specs: [
        {
          id: 2,
          expected_mention: "小乔",
          canonical_key: "person:223abda2-a6bd-46cc-abbe-ac6ff0b5fc2f",
          required_profile_terms: ["跑步"],
        },
        {
          id: 3,
          expected_mention: "Paxon",
          canonical_key: "person:223abda2-a6bd-46cc-abbe-ac6ff0b5fc2f",
          required_profile_terms: ["跑步"],
        },
      ],
      answer_context_speaker_canary_specs: [
        {
          id: 1000002,
          canary_type: "speaker_self",
          expected_speaker_label: "小乔",
          canonical_key: "person:223abda2-a6bd-46cc-abbe-ac6ff0b5fc2f",
          required_profile_terms: ["跑步"],
        },
      ],
      answer_context_referenced_canary_specs: [
        {
          id: 2000002,
          canary_type: "referenced_member",
          expected_referenced_label: "小乔",
          canonical_key: "person:223abda2-a6bd-46cc-abbe-ac6ff0b5fc2f",
          required_profile_terms: ["跑步"],
        },
      ],
    }),
    "utf8"
  );
  fs.writeFileSync(
    speakerMapPath,
    JSON.stringify({
      scope_fingerprint: scopeFingerprint,
      senders: [
        {
          canonical_key: "person:223abda2-a6bd-46cc-abbe-ac6ff0b5fc2f",
          sender_id: "member_sender_example",
        },
      ],
    }),
    "utf8"
  );

  const result = spawnSync(
    "node",
    [
      builder,
      "--spec",
      specPath,
      "--chat-id-env",
      "TEST_ERHUA_CANARY_CHAT_ID",
      "--sender-id-env",
      "TEST_ERHUA_CANARY_SENDER_ID",
      "--speaker-sender-map",
      speakerMapPath,
      "--output",
      outputPath,
    ],
    {
      cwd: repoRoot,
      encoding: "utf8",
      env: {
        ...process.env,
        TEST_ERHUA_CANARY_CHAT_ID: "room_example",
        TEST_ERHUA_CANARY_SENDER_ID: "sender_example",
      },
    }
  );
  assert.equal(result.status, 0, result.stderr);
  const lines = fs
    .readFileSync(outputPath, "utf8")
    .split(/\r?\n/)
    .filter(Boolean)
    .map((line) => JSON.parse(line));
  assert.equal(lines[0].method, "initialize");
  assert.equal(lines[2].id, 2);
  assert.equal(lines[2].params.name, "qintopia_answer_context_prepare");
  assert.equal(lines[2].params.arguments.chat_id, "room_example");
  assert.equal(lines[2].params.arguments.sender_id, "sender_example");
  assert.equal(lines[2].params.arguments.message_text, "小乔是谁");
  assert.deepEqual(lines[2].params.arguments.mentioned_member_names, ["小乔"]);
  assert.equal(lines[3].params.arguments.message_text, "Paxon是谁");
  assert.deepEqual(lines[3].params.arguments.mentioned_member_names, ["Paxon"]);
  assert.equal(lines[4].id, 1000002);
  assert.equal(lines[4].params.arguments.sender_id, "member_sender_example");
  assert.equal(lines[4].params.arguments.message_text, "我是谁");
  assert.equal(lines[4].params.arguments.mentioned_member_names, undefined);
  assert.equal(lines[5].id, 2000002);
  assert.equal(lines[5].params.arguments.sender_id, "sender_example");
  assert.equal(lines[5].params.arguments.referenced_sender_id, "member_sender_example");
  assert.equal(lines[5].params.arguments.message_text, "他是谁");
  assert.equal(lines[5].params.arguments.mentioned_member_names, undefined);

  const omitted = spawnSync(
    "node",
    [
      builder,
      "--spec",
      specPath,
      "--chat-id",
      "room_example",
      "--sender-id",
      "sender_example",
      "--speaker-sender-map",
      speakerMapPath,
      "--omit-mentioned-member-names",
    ],
    {
      cwd: repoRoot,
      encoding: "utf8",
    }
  );
  assert.equal(omitted.status, 0, omitted.stderr);
  assert.doesNotMatch(omitted.stdout, /mentioned_member_names/);

  const missingSpeakerMap = spawnSync(
    "node",
    [
      builder,
      "--spec",
      specPath,
      "--chat-id",
      "room_example",
      "--sender-id",
      "sender_example",
    ],
    {
      cwd: repoRoot,
      encoding: "utf8",
    }
  );
  assert.notEqual(missingSpeakerMap.status, 0);
  assert.match(missingSpeakerMap.stderr, /missing private speaker sender_id/);

  const wrongScopeSpeakerMapPath = path.join(tmpRoot, "wrong-scope-speaker-map.json");
  fs.writeFileSync(
    wrongScopeSpeakerMapPath,
    JSON.stringify({
      scope_fingerprint:
        "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
      senders: [
        {
          canonical_key: "person:223abda2-a6bd-46cc-abbe-ac6ff0b5fc2f",
          sender_id: "member_sender_example",
        },
      ],
    }),
    "utf8"
  );
  const wrongScopeSpeakerMap = spawnSync(
    "node",
    [
      builder,
      "--spec",
      specPath,
      "--chat-id",
      "room_example",
      "--sender-id",
      "sender_example",
      "--speaker-sender-map",
      wrongScopeSpeakerMapPath,
    ],
    {
      cwd: repoRoot,
      encoding: "utf8",
    }
  );
  assert.notEqual(wrongScopeSpeakerMap.status, 0);
  assert.match(wrongScopeSpeakerMap.stderr, /scope_fingerprint must match/);

  const extraSpeakerMapPath = path.join(tmpRoot, "extra-speaker-map.json");
  fs.writeFileSync(
    extraSpeakerMapPath,
    JSON.stringify({
      scope_fingerprint: scopeFingerprint,
      senders: [
        {
          canonical_key: "person:223abda2-a6bd-46cc-abbe-ac6ff0b5fc2f",
          sender_id: "member_sender_example",
        },
        {
          canonical_key: "person:unexpected",
          sender_id: "unexpected_sender_example",
        },
      ],
    }),
    "utf8"
  );
  const extraSpeakerMap = spawnSync(
    "node",
    [
      builder,
      "--spec",
      specPath,
      "--chat-id",
      "room_example",
      "--sender-id",
      "sender_example",
      "--speaker-sender-map",
      extraSpeakerMapPath,
    ],
    {
      cwd: repoRoot,
      encoding: "utf8",
    }
  );
  assert.notEqual(extraSpeakerMap.status, 0);
  assert.match(extraSpeakerMap.stderr, /unexpected canonical_key/);

  const invalid = spawnSync(
    "node",
    [
      builder,
      "--spec",
      specPath,
      "--chat-id",
      "room_example",
      "--sender-id",
      "sender_example",
      "--speaker-sender-map",
      speakerMapPath,
      "--message-template",
      "是谁",
    ],
    {
      cwd: repoRoot,
      encoding: "utf8",
    }
  );
  assert.notEqual(invalid.status, 0);
  assert.match(invalid.stderr, /must include \{name\}/);

  const partialSpecPath = path.join(tmpRoot, "partial-spec.json");
  fs.writeFileSync(
    partialSpecPath,
    JSON.stringify({
      answer_context_canary_specs_total: 2,
      answer_context_canary_specs: [
        {
          id: 2,
          expected_mention: "小乔",
          canonical_key: "person:223abda2-a6bd-46cc-abbe-ac6ff0b5fc2f",
        },
      ],
      answer_context_speaker_canary_specs: [
        {
          id: 1000002,
          canary_type: "speaker_self",
          expected_speaker_label: "小乔",
          canonical_key: "person:223abda2-a6bd-46cc-abbe-ac6ff0b5fc2f",
        },
      ],
      answer_context_referenced_canary_specs: [
        {
          id: 2000002,
          canary_type: "referenced_member",
          expected_referenced_label: "小乔",
          canonical_key: "person:223abda2-a6bd-46cc-abbe-ac6ff0b5fc2f",
        },
      ],
    }),
    "utf8"
  );
  const partial = spawnSync(
    "node",
    [
      builder,
      "--spec",
      partialSpecPath,
      "--chat-id",
      "room_example",
      "--sender-id",
      "sender_example",
      "--speaker-sender-map",
      speakerMapPath,
    ],
    {
      cwd: repoRoot,
      encoding: "utf8",
    }
  );
  assert.notEqual(partial.status, 0);
  assert.match(partial.stderr, /answer_context_canary_specs length/);

  const peopleMismatchSpecPath = path.join(tmpRoot, "people-mismatch-spec.json");
  fs.writeFileSync(
    peopleMismatchSpecPath,
    JSON.stringify({
      answer_context_canary_people_total: 2,
      answer_context_canary_specs: [
        {
          id: 2,
          expected_mention: "小乔",
          canonical_key: "person:223abda2-a6bd-46cc-abbe-ac6ff0b5fc2f",
        },
        {
          id: 3,
          expected_mention: "Paxon",
          canonical_key: "person:223abda2-a6bd-46cc-abbe-ac6ff0b5fc2f",
        },
      ],
      answer_context_speaker_canary_specs: [
        {
          id: 1000002,
          canary_type: "speaker_self",
          expected_speaker_label: "小乔",
          canonical_key: "person:223abda2-a6bd-46cc-abbe-ac6ff0b5fc2f",
        },
      ],
      answer_context_referenced_canary_specs: [
        {
          id: 2000002,
          canary_type: "referenced_member",
          expected_referenced_label: "小乔",
          canonical_key: "person:223abda2-a6bd-46cc-abbe-ac6ff0b5fc2f",
        },
      ],
    }),
    "utf8"
  );
  const peopleMismatch = spawnSync(
    "node",
    [
      builder,
      "--spec",
      peopleMismatchSpecPath,
      "--chat-id",
      "room_example",
      "--sender-id",
      "sender_example",
      "--speaker-sender-map",
      speakerMapPath,
    ],
    {
      cwd: repoRoot,
      encoding: "utf8",
    }
  );
  assert.notEqual(peopleMismatch.status, 0);
  assert.match(peopleMismatch.stderr, /unique people/);

  const missingRouteSpecPath = path.join(tmpRoot, "missing-route-spec.json");
  fs.writeFileSync(
    missingRouteSpecPath,
    JSON.stringify({
      answer_context_canary_specs: [
        {
          id: 2,
          expected_mention: "小乔",
          canonical_key: "person:223abda2-a6bd-46cc-abbe-ac6ff0b5fc2f",
        },
      ],
    }),
    "utf8"
  );
  const missingRoute = spawnSync(
    "node",
    [
      builder,
      "--spec",
      missingRouteSpecPath,
      "--chat-id",
      "room_example",
      "--sender-id",
      "sender_example",
      "--speaker-sender-map",
      speakerMapPath,
    ],
    {
      cwd: repoRoot,
      encoding: "utf8",
    }
  );
  assert.notEqual(missingRoute.status, 0);
  assert.match(missingRoute.stderr, /speaker self-canary records/);

  const routeMismatchSpecPath = path.join(tmpRoot, "route-mismatch-spec.json");
  fs.writeFileSync(
    routeMismatchSpecPath,
    JSON.stringify({
      answer_context_canary_specs: [
        {
          id: 2,
          expected_mention: "小乔",
          canonical_key: "person:223abda2-a6bd-46cc-abbe-ac6ff0b5fc2f",
        },
      ],
      answer_context_speaker_canary_specs: [
        {
          id: 1000002,
          canary_type: "speaker_self",
          expected_speaker_label: "小乔",
          canonical_key: "person:other",
        },
      ],
      answer_context_referenced_canary_specs: [
        {
          id: 2000002,
          canary_type: "referenced_member",
          expected_referenced_label: "小乔",
          canonical_key: "person:223abda2-a6bd-46cc-abbe-ac6ff0b5fc2f",
        },
      ],
    }),
    "utf8"
  );
  const routeMismatch = spawnSync(
    "node",
    [
      builder,
      "--spec",
      routeMismatchSpecPath,
      "--chat-id",
      "room_example",
      "--sender-id",
      "sender_example",
      "--speaker-sender-map",
      speakerMapPath,
    ],
    {
      cwd: repoRoot,
      encoding: "utf8",
    }
  );
  assert.notEqual(routeMismatch.status, 0);
  assert.match(routeMismatch.stderr, /same canonical people/);
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Erhua member recognition canary MCP input test passed.");
