#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { createHash } from "node:crypto";

const args = parseArgs(process.argv.slice(2));
if (!args.spec || !args.mcpOutput) {
  fail(
    "usage: node tools/deploy/build-erhua-member-recognition-canary-evidence.mjs --spec <canary-spec.json> --mcp-output <context-mcp-output.jsonl> [--output <canary.jsonl>]"
  );
}

const specPath = path.resolve(args.spec);
const mcpOutputPath = path.resolve(args.mcpOutput);
const spec = parseSpec(fs.readFileSync(specPath, "utf8"));
const responses = parseMcpResponses(fs.readFileSync(mcpOutputPath, "utf8"));
const records = [];
const ids = new Set();

for (const canary of spec) {
  const canaryType = canaryTypeOf(canary);
  const id = canary.id;
  if (id === undefined || id === null || `${id}`.trim() === "") {
    fail("each canary spec item must include id");
  }
  const idKey = `${id}`;
  if (ids.has(idKey)) {
    fail(`duplicate canary spec id: ${idKey}`);
  }
  ids.add(idKey);
  const expectedMention = asText(canary.expected_mention ?? canary.name);
  const expectedSpeakerLabel = asText(canary.expected_speaker_label);
  const expectedReferencedLabel = asText(canary.expected_referenced_label);
  if (canaryType === "mentioned_member" && !expectedMention) {
    fail(`mentioned-member canary id ${id} must include expected_mention or name`);
  }
  const canonicalKey = asText(canary.canonical_key);
  if (canaryType === "speaker_self" && !canonicalKey) {
    fail(`speaker self canary id ${id} must include canonical_key`);
  }
  if (canaryType === "referenced_member" && !canonicalKey) {
    fail(`referenced member canary id ${id} must include canonical_key`);
  }
  const answerContext = responses.get(idKey);
  if (!answerContext) {
    fail(`missing MCP answer_context response for canary id ${id}`);
  }
  records.push({
    canary_type: canaryType,
    ...(expectedMention ? { expected_mention: expectedMention } : {}),
    ...(expectedSpeakerLabel ? { expected_speaker_label: expectedSpeakerLabel } : {}),
    ...(expectedReferencedLabel
      ? { expected_referenced_label: expectedReferencedLabel }
      : {}),
    ...(canonicalKey ? { canonical_key: canonicalKey } : {}),
    ...(Array.isArray(canary.required_profile_terms)
      ? {
          required_profile_terms: canary.required_profile_terms
            .map(asText)
            .filter(Boolean),
        }
      : {}),
    answer_context: sanitizeAnswerContext(answerContext),
  });
}

const output = records
  .map((record) => `erhua_member_recognition_canary=${JSON.stringify(record)}`)
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
    } else if (arg === "--mcp-output") {
      parsed.mcpOutput = argv[++index];
    } else if (arg === "--output") {
      parsed.output = argv[++index];
    } else {
      fail(`unknown argument: ${arg}`);
    }
  }
  return parsed;
}

function parseSpec(text) {
  const parsed = parseJson(text, "canary spec");
  const canaries = Array.isArray(parsed) ? parsed : parsed.canaries;
  if (Array.isArray(canaries)) {
    return requireCanaryArray(canaries);
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
  return requireCanaryArray([
    ...mentionCanaries,
    ...speakerCanaries,
    ...referencedCanaries,
  ]);
}

function requireCanaryArray(canaries) {
  if (!Array.isArray(canaries) || canaries.length === 0) {
    fail(
      "canary spec must be a non-empty array, {canaries:[...]}, or {answer_context_canary_specs:[...]}"
    );
  }
  return canaries;
}

function parseMcpResponses(text) {
  const responses = new Map();
  for (const [index, line] of text.split(/\r?\n/).entries()) {
    const trimmed = line.trim();
    if (!trimmed) {
      continue;
    }
    const message = parseJson(trimmed, `MCP output line ${index + 1}`);
    if (message.id === undefined || message.id === null) {
      continue;
    }
    const answerContext = answerContextFromMcpMessage(message);
    if (answerContext) {
      const idKey = `${message.id}`;
      if (responses.has(idKey)) {
        fail(`duplicate MCP answer_context response for canary id ${idKey}`);
      }
      responses.set(idKey, answerContext);
    }
  }
  return responses;
}

function answerContextFromMcpMessage(message) {
  const content = message.result?.content;
  if (!Array.isArray(content) || !content[0] || typeof content[0] !== "object") {
    return null;
  }
  const text = asText(content[0].text);
  if (!text) {
    return null;
  }
  const parsed = parseJson(text, `MCP response ${message.id} text`);
  return parsed && typeof parsed === "object" ? parsed : null;
}

function sanitizeAnswerContext(answerContext) {
  const mentionedMembers = Array.isArray(answerContext.mentioned_members)
    ? answerContext.mentioned_members.map(sanitizeMentionedMember)
    : [];
  return keepDefined({
    success: answerContext.success === true,
    speaker: sanitizeSpeaker(answerContext.speaker),
    referenced_member: sanitizeSpeaker(answerContext.referenced_member),
    mentioned_members: mentionedMembers,
  });
}

function sanitizeSpeaker(speaker) {
  if (!speaker || typeof speaker !== "object") {
    return undefined;
  }
  return keepDefined({
    resolved: speaker.resolved === true,
    resolution_scope: redactSensitiveText(asText(speaker.resolution_scope)),
    display_name: redactSensitiveText(asText(speaker.display_name)),
    person_ref: personRef(asText(speaker.person_id)),
    safe_summary: redactSensitiveText(asText(speaker.safe_summary)),
    safe_reply_hints:
      speaker.safe_reply_hints &&
      typeof speaker.safe_reply_hints === "object" &&
      !Array.isArray(speaker.safe_reply_hints)
        ? redactSensitiveJson(speaker.safe_reply_hints)
        : undefined,
  });
}

function sanitizeMentionedMember(member) {
  if (!member || typeof member !== "object") {
    return {};
  }
  return keepDefined({
    mention_text: redactSensitiveText(asText(member.mention_text)),
    resolved: member.resolved === true,
    resolution_status: redactSensitiveText(asText(member.resolution_status)),
    match_count: Number.isInteger(member.match_count) ? member.match_count : undefined,
    display_name: redactSensitiveText(asText(member.display_name)),
    person_ref: personRef(asText(member.person_id)),
    safe_summary: redactSensitiveText(asText(member.safe_summary)),
    safe_reply_hints:
      member.safe_reply_hints &&
      typeof member.safe_reply_hints === "object" &&
      !Array.isArray(member.safe_reply_hints)
        ? redactSensitiveJson(member.safe_reply_hints)
        : undefined,
  });
}

function keepDefined(record) {
  return Object.fromEntries(
    Object.entries(record).filter(([, value]) => value !== undefined && value !== "")
  );
}

function asText(value) {
  return typeof value === "string" ? value.trim() : "";
}

function redactSensitiveJson(value) {
  if (typeof value === "string") {
    return redactSensitiveText(value);
  }
  if (Array.isArray(value)) {
    return value.map(redactSensitiveJson);
  }
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value).map(([key, item]) => [key, redactSensitiveJson(item)])
    );
  }
  return value;
}

function redactSensitiveText(text) {
  return text
    .replace(/\d{7,}/g, "[敏感数字]")
    .replace(/[\u0000-\u001f\u007f-\u009f]/g, "");
}

function personRef(personId) {
  if (!isUuid(personId)) {
    return undefined;
  }
  return `sha256:${createHash("sha256")
    .update(`erhua-member-recognition-person-ref-v1:${personId.toLowerCase()}`)
    .digest("hex")}`;
}

function isUuid(value) {
  return (
    typeof value === "string" &&
    /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(
      value
    )
  );
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
