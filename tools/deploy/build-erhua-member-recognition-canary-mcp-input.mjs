#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const args = parseArgs(process.argv.slice(2));
if (!args.spec) {
  fail(
    "usage: node tools/deploy/build-erhua-member-recognition-canary-mcp-input.mjs --spec <canary-spec.json> (--chat-id <chat-id>|--chat-id-env <env>) (--sender-id <sender-id>|--sender-id-env <env>) [--speaker-sender-map <private-json>] [--message-template <template>] [--speaker-message-template <template>] [--referenced-message-template <template>] [--omit-mentioned-member-names] [--output <mcp-input.jsonl>]"
  );
}

const chatId = readValue("chat id", args.chatId, args.chatIdEnv);
const senderId = readValue("sender id", args.senderId, args.senderIdEnv);
const messageTemplate = args.messageTemplate || "{name}是谁";
if (!messageTemplate.includes("{name}")) {
  fail("--message-template must include {name}");
}
const speakerMessageTemplate = args.speakerMessageTemplate || "我是谁";
if (!speakerMessageTemplate.trim()) {
  fail("--speaker-message-template must not be empty");
}
const referencedMessageTemplate = args.referencedMessageTemplate || "他是谁";
if (!referencedMessageTemplate.trim()) {
  fail("--referenced-message-template must not be empty");
}

const specDocument = parseSpecDocument(
  fs.readFileSync(path.resolve(args.spec), "utf8")
);
const canaries = specDocument.canaries;
const speakerSenderMap = args.speakerSenderMap
  ? parseSpeakerSenderMap(
      fs.readFileSync(path.resolve(args.speakerSenderMap), "utf8"),
      specDocument.scopeFingerprint
    )
  : new Map();
validateSpeakerSenderMapCoverage(speakerSenderMap, canaries);
const ids = new Set(["1"]);
const payloads = [
  {
    jsonrpc: "2.0",
    id: 1,
    method: "initialize",
    params: {
      protocolVersion: "2025-06-18",
      capabilities: {},
      clientInfo: {
        name: "erhua-member-recognition-canary",
        version: "0.1.0",
      },
    },
  },
  { jsonrpc: "2.0", method: "notifications/initialized", params: {} },
];

for (const canary of canaries) {
  const canaryType = canaryTypeOf(canary);
  const id = canary.id;
  if (id === undefined || id === null || `${id}`.trim() === "") {
    fail("each canary spec item must include id");
  }
  const idKey = `${id}`;
  if (ids.has(idKey)) {
    fail(`duplicate or reserved canary id: ${idKey}`);
  }
  ids.add(idKey);
  let argumentsPayload;
  if (canaryType === "speaker_self") {
    const canonicalKey = asText(canary.canonical_key ?? canary.same_person_key);
    if (!canonicalKey) {
      fail(`speaker self canary id ${idKey} must include canonical_key`);
    }
    const speakerSenderId = speakerSenderMap.get(canonicalKey);
    if (!speakerSenderId) {
      fail(`missing private speaker sender_id for ${canonicalKey}`);
    }
    argumentsPayload = {
      caller_profile: "erhua",
      platform: "qiwe",
      chat_id: chatId,
      sender_id: speakerSenderId,
      message_text: speakerMessageTemplate,
      purpose: "production Erhua speaker self-recognition canary",
    };
  } else if (canaryType === "referenced_member") {
    const canonicalKey = asText(canary.canonical_key ?? canary.same_person_key);
    if (!canonicalKey) {
      fail(`referenced member canary id ${idKey} must include canonical_key`);
    }
    const referencedSenderId = speakerSenderMap.get(canonicalKey);
    if (!referencedSenderId) {
      fail(`missing private referenced sender_id for ${canonicalKey}`);
    }
    argumentsPayload = {
      caller_profile: "erhua",
      platform: "qiwe",
      chat_id: chatId,
      sender_id: senderId,
      referenced_sender_id: referencedSenderId,
      message_text: referencedMessageTemplate,
      purpose: "production Erhua referenced member recognition canary",
    };
  } else {
    const expectedMention = asText(canary.expected_mention ?? canary.name);
    if (!expectedMention) {
      fail(`canary id ${idKey} must include expected_mention or name`);
    }
    argumentsPayload = {
      caller_profile: "erhua",
      platform: "qiwe",
      chat_id: chatId,
      sender_id: senderId,
      message_text: messageTemplate.replaceAll("{name}", expectedMention),
      purpose: "production Erhua member recognition canary",
    };
    if (!args.omitMentionedMemberNames) {
      argumentsPayload.mentioned_member_names = [expectedMention];
    }
  }
  payloads.push({
    jsonrpc: "2.0",
    id,
    method: "tools/call",
    params: {
      name: "qintopia_answer_context_prepare",
      arguments: argumentsPayload,
    },
  });
}

const output = payloads
  .map((payload) => JSON.stringify(payload))
  .join("\n")
  .concat("\n");

if (args.output) {
  fs.writeFileSync(path.resolve(args.output), output, "utf8");
} else {
  process.stdout.write(output);
}

function parseArgs(argv) {
  const parsed = {};
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--spec") {
      parsed.spec = argv[++index];
    } else if (arg === "--chat-id") {
      parsed.chatId = argv[++index];
    } else if (arg === "--chat-id-env") {
      parsed.chatIdEnv = argv[++index];
    } else if (arg === "--sender-id") {
      parsed.senderId = argv[++index];
    } else if (arg === "--sender-id-env") {
      parsed.senderIdEnv = argv[++index];
    } else if (arg === "--message-template") {
      parsed.messageTemplate = argv[++index];
    } else if (arg === "--speaker-message-template") {
      parsed.speakerMessageTemplate = argv[++index];
    } else if (arg === "--referenced-message-template") {
      parsed.referencedMessageTemplate = argv[++index];
    } else if (arg === "--speaker-sender-map") {
      parsed.speakerSenderMap = argv[++index];
    } else if (arg === "--omit-mentioned-member-names") {
      parsed.omitMentionedMemberNames = true;
    } else if (arg === "--output") {
      parsed.output = argv[++index];
    } else {
      fail(`unknown argument: ${arg}`);
    }
  }
  return parsed;
}

function readValue(label, directValue, envName) {
  if (directValue && envName) {
    fail(
      `use either --${label.replaceAll(" ", "-")} or --${label.replaceAll(" ", "-")}-env, not both`
    );
  }
  const value = directValue ?? (envName ? process.env[envName] : "");
  const text = asText(value);
  if (!text) {
    fail(`missing ${label}`);
  }
  return text;
}

function parseSpecDocument(text) {
  const parsed = parseJson(text, "canary spec");
  const canaries = Array.isArray(parsed) ? parsed : parsed.canaries;
  if (Array.isArray(canaries)) {
    return {
      canaries: requireCanaryArray(canaries),
      scopeFingerprint: asText(parsed.scope_fingerprint ?? parsed.scopeFingerprint),
    };
  }
  const mentionCanaries = Array.isArray(parsed.answer_context_canary_specs)
    ? parsed.answer_context_canary_specs
    : [];
  const speakerCanaries = Array.isArray(parsed.answer_context_speaker_canary_specs)
    ? parsed.answer_context_speaker_canary_specs
    : [];
  const referencedCanaries = Array.isArray(
    parsed.answer_context_referenced_canary_specs
  )
    ? parsed.answer_context_referenced_canary_specs
    : [];
  validateCoverageCanaryTotals(
    parsed,
    mentionCanaries,
    speakerCanaries,
    referencedCanaries
  );
  return {
    canaries: requireCanaryArray([
      ...mentionCanaries,
      ...speakerCanaries,
      ...referencedCanaries,
    ]),
    scopeFingerprint: asText(parsed.scope_fingerprint ?? parsed.scopeFingerprint),
  };
}

function requireCanaryArray(canaries) {
  if (!Array.isArray(canaries) || canaries.length === 0) {
    fail(
      "canary spec must be a non-empty array, {canaries:[...]}, or {answer_context_canary_specs:[...]}"
    );
  }
  return canaries;
}

function parseSpeakerSenderMap(text, expectedScopeFingerprint) {
  const parsed = parseJson(text, "speaker sender map");
  const scopeFingerprint =
    parsed && typeof parsed === "object" && !Array.isArray(parsed)
      ? asText(parsed.scope_fingerprint ?? parsed.scopeFingerprint)
      : "";
  if (expectedScopeFingerprint) {
    if (!scopeFingerprint) {
      fail(
        "speaker sender map scope_fingerprint must match coverage scope_fingerprint"
      );
    }
    if (scopeFingerprint !== expectedScopeFingerprint) {
      fail(
        `speaker sender map scope_fingerprint must match coverage scope_fingerprint: expected ${expectedScopeFingerprint}, got ${scopeFingerprint}`
      );
    }
  }
  const entries = Array.isArray(parsed)
    ? parsed
    : Array.isArray(parsed.senders)
      ? parsed.senders
      : Object.entries(parsed).map(([canonicalKey, senderId]) => ({
          canonical_key: canonicalKey,
          sender_id: senderId,
        }));
  const map = new Map();
  for (const [index, entry] of entries.entries()) {
    if (!entry || typeof entry !== "object") {
      fail(`speaker sender map entry ${index + 1} must be an object`);
    }
    const canonicalKey = asText(entry.canonical_key ?? entry.canonicalKey);
    const senderId = asText(entry.sender_id ?? entry.senderId);
    if (!canonicalKey || !senderId) {
      fail(
        `speaker sender map entry ${index + 1} must include canonical_key and sender_id`
      );
    }
    if (map.has(canonicalKey)) {
      fail(`duplicate private speaker sender map entry for ${canonicalKey}`);
    }
    map.set(canonicalKey, senderId);
  }
  return map;
}

function validateSpeakerSenderMapCoverage(speakerSenderMap, canaries) {
  const requiredCanonicalKeys = new Set();
  for (const canary of canaries) {
    const type = canaryTypeOf(canary);
    if (type !== "speaker_self" && type !== "referenced_member") {
      continue;
    }
    const canonicalKey = asText(canary.canonical_key ?? canary.same_person_key);
    if (!canonicalKey) {
      fail(`${type} canary must include canonical_key`);
    }
    requiredCanonicalKeys.add(canonicalKey);
  }
  if (requiredCanonicalKeys.size === 0) {
    return;
  }
  for (const canonicalKey of requiredCanonicalKeys) {
    if (!speakerSenderMap.has(canonicalKey)) {
      fail(`missing private speaker sender_id for ${canonicalKey}`);
    }
  }
  for (const canonicalKey of speakerSenderMap.keys()) {
    if (!requiredCanonicalKeys.has(canonicalKey)) {
      fail(
        `private speaker sender map contains unexpected canonical_key: ${canonicalKey}`
      );
    }
  }
}

function canaryTypeOf(canary) {
  const type = asText(canary.canary_type ?? canary.canaryType);
  if (!type) {
    return "mentioned_member";
  }
  if (
    type !== "mentioned_member" &&
    type !== "speaker_self" &&
    type !== "referenced_member"
  ) {
    fail(`unsupported canary_type: ${type}`);
  }
  return type;
}

function validateCoverageCanaryTotals(
  record,
  mentionCanaries,
  speakerCanaries,
  referencedCanaries
) {
  if (mentionCanaries.length === 0) {
    fail("coverage canary spec must include mentioned-member records");
  }
  if (speakerCanaries.length === 0) {
    fail("coverage canary spec must include speaker self-canary records");
  }
  if (referencedCanaries.length === 0) {
    fail("coverage canary spec must include referenced-member records");
  }

  const mentionSpecsTotal = optionalNonNegativeInteger(
    record,
    "answer_context_canary_specs_total"
  );
  if (mentionSpecsTotal !== null && mentionCanaries.length !== mentionSpecsTotal) {
    fail(
      `answer_context_canary_specs length must match answer_context_canary_specs_total: expected ${mentionSpecsTotal}, got ${mentionCanaries.length}`
    );
  }
  const speakerSpecsTotal = optionalNonNegativeInteger(
    record,
    "answer_context_speaker_canary_specs_total"
  );
  if (speakerSpecsTotal !== null && speakerCanaries.length !== speakerSpecsTotal) {
    fail(
      `answer_context_speaker_canary_specs length must match answer_context_speaker_canary_specs_total: expected ${speakerSpecsTotal}, got ${speakerCanaries.length}`
    );
  }

  const mentionPeopleTotal = optionalNonNegativeInteger(
    record,
    "answer_context_canary_people_total"
  );
  if (
    mentionPeopleTotal !== null &&
    uniqueCanonicalKeyCount(mentionCanaries) !== mentionPeopleTotal
  ) {
    fail(
      `answer_context_canary_specs unique people must match answer_context_canary_people_total: expected ${mentionPeopleTotal}, got ${uniqueCanonicalKeyCount(
        mentionCanaries
      )}`
    );
  }
  const speakerPeopleTotal = optionalNonNegativeInteger(
    record,
    "answer_context_speaker_canary_people_total"
  );
  if (
    speakerPeopleTotal !== null &&
    uniqueCanonicalKeyCount(speakerCanaries) !== speakerPeopleTotal
  ) {
    fail(
      `answer_context_speaker_canary_specs unique people must match answer_context_speaker_canary_people_total: expected ${speakerPeopleTotal}, got ${uniqueCanonicalKeyCount(
        speakerCanaries
      )}`
    );
  }
  const referencedSpecsTotal = optionalNonNegativeInteger(
    record,
    "answer_context_referenced_canary_specs_total"
  );
  if (
    referencedSpecsTotal !== null &&
    referencedCanaries.length !== referencedSpecsTotal
  ) {
    fail(
      `answer_context_referenced_canary_specs length must match answer_context_referenced_canary_specs_total: expected ${referencedSpecsTotal}, got ${referencedCanaries.length}`
    );
  }
  const referencedPeopleTotal = optionalNonNegativeInteger(
    record,
    "answer_context_referenced_canary_people_total"
  );
  if (
    referencedPeopleTotal !== null &&
    uniqueCanonicalKeyCount(referencedCanaries) !== referencedPeopleTotal
  ) {
    fail(
      `answer_context_referenced_canary_specs unique people must match answer_context_referenced_canary_people_total: expected ${referencedPeopleTotal}, got ${uniqueCanonicalKeyCount(
        referencedCanaries
      )}`
    );
  }

  const mentionPeople = canonicalKeySet(mentionCanaries, "answer_context_canary_specs");
  const speakerPeople = canonicalKeySet(
    speakerCanaries,
    "answer_context_speaker_canary_specs"
  );
  const referencedPeople = canonicalKeySet(
    referencedCanaries,
    "answer_context_referenced_canary_specs"
  );
  if (!setsEqual(mentionPeople, speakerPeople)) {
    fail(
      "answer_context_canary_specs and answer_context_speaker_canary_specs must cover the same canonical people"
    );
  }
  if (!setsEqual(mentionPeople, referencedPeople)) {
    fail(
      "answer_context_canary_specs and answer_context_referenced_canary_specs must cover the same canonical people"
    );
  }
}

function uniqueCanonicalKeyCount(canaries) {
  return new Set(
    canaries
      .map((canary) => asText(canary?.canonical_key ?? canary?.same_person_key))
      .filter(Boolean)
  ).size;
}

function canonicalKeySet(canaries, label) {
  const keys = new Set();
  for (const [index, canary] of canaries.entries()) {
    const canonicalKey = asText(canary?.canonical_key ?? canary?.same_person_key);
    if (!canonicalKey) {
      fail(`${label}[${index}] must include canonical_key`);
    }
    keys.add(canonicalKey);
  }
  return keys;
}

function setsEqual(left, right) {
  if (left.size !== right.size) {
    return false;
  }
  for (const item of left) {
    if (!right.has(item)) {
      return false;
    }
  }
  return true;
}

function optionalNonNegativeInteger(record, fieldName) {
  const value = record?.[fieldName];
  if (value === undefined || value === null) {
    return null;
  }
  if (!Number.isInteger(value) || value < 0) {
    fail(`${fieldName} must be a non-negative integer when present`);
  }
  return value;
}

function asText(value) {
  return typeof value === "string" ? value.trim() : "";
}

function parseJson(text, label) {
  try {
    return JSON.parse(text);
  } catch (error) {
    fail(`${label} is not valid JSON: ${error.message}`);
  }
}

function fail(message) {
  console.error(message);
  process.exit(1);
}
