#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const args = process.argv.slice(2);
if (args.length !== 1) {
  fail(
    "usage: node tools/deploy/check-erhua-member-recognition-canary.mjs <answer-context-canary-output.jsonl>"
  );
}

const evidenceFile = path.resolve(args[0]);
const evidenceText = fs.readFileSync(evidenceFile, "utf8");

for (const pattern of [
  /postgres(?:ql)?:\/\//i,
  /tenant_access_token/i,
  /base_token/i,
  /api[_-]?key/i,
  /\btoken\b/i,
  /QIWE_TOKEN/,
  /QIWE_GUID/,
  /DATABASE_URL/,
  /"chat_id"\s*:/,
  /"sender_id"\s*:/,
  /"channel_user_id"\s*:/,
  /"person_id"\s*:/,
  /"raw_messages"\s*:/,
  /"hidden_profile_details"\s*:/,
  /"raw"\s*:/,
]) {
  if (pattern.test(evidenceText)) {
    fail(`canary evidence contains forbidden sensitive fragment: ${pattern}`);
  }
}

const records = parseCanaryRecords(evidenceText);
if (records.length === 0) {
  fail("expected at least one Erhua member recognition canary record");
}
const visibleText = records.flatMap(visibleTextFields).join("\n");
if (/1[3-9]\d{9}/.test(visibleText)) {
  fail("canary evidence contains forbidden sensitive fragment: /1[3-9]\\d{9}/");
}

const errors = [];
const canonicalPeople = new Map();
const resolvedPeople = new Set();
const mentionedResolvedPeople = new Set();
const speakerResolvedPeople = new Set();
const referencedResolvedPeople = new Set();
const mentionedProfileHintPeople = new Set();
const speakerProfileHintPeople = new Set();
const referencedProfileHintPeople = new Set();
let mentionedRecordCount = 0;
let speakerRecordCount = 0;
let referencedRecordCount = 0;

for (const [index, record] of records.entries()) {
  const canaryType = canaryTypeOf(record);
  const answerContext = answerContextFromRecord(record);
  if (!answerContext || answerContext.success !== true) {
    errors.push(`canary ${index + 1}: answer_context is missing or unsuccessful`);
    continue;
  }

  if (canaryType === "speaker_self") {
    speakerRecordCount += 1;
    const label =
      textField(record, ["expected_speaker_label", "canonical_key"]) ||
      `speaker canary ${index + 1}`;
    const speaker = answerContext.speaker;
    if (!speaker || typeof speaker !== "object") {
      errors.push(`${label}: speaker was not returned`);
      continue;
    }
    if (speaker.resolved !== true) {
      errors.push(`${label}: speaker did not resolve`);
      continue;
    }
    if (
      speaker.resolution_scope !== "exact_chat" &&
      speaker.resolution_scope !== "qiwe_platform_user"
    ) {
      errors.push(
        `${label}: speaker resolution_scope must be exact_chat or qiwe_platform_user`
      );
      continue;
    }
    const personRef = readPersonRef(speaker, label, "speaker", errors);
    if (!personRef) {
      continue;
    }
    resolvedPeople.add(personRef);
    speakerResolvedPeople.add(personRef);
    if (checkSafeProfile(record, speaker, label, errors)) {
      speakerProfileHintPeople.add(personRef);
    }
    checkCanonical(record, personRef, label, canonicalPeople, errors);
  } else if (canaryType === "referenced_member") {
    referencedRecordCount += 1;
    const label =
      textField(record, ["expected_referenced_label", "canonical_key"]) ||
      `referenced canary ${index + 1}`;
    const referencedMember = answerContext.referenced_member;
    if (!referencedMember || typeof referencedMember !== "object") {
      errors.push(`${label}: referenced_member was not returned`);
      continue;
    }
    if (referencedMember.resolved !== true) {
      errors.push(`${label}: referenced_member did not resolve`);
      continue;
    }
    if (
      referencedMember.resolution_scope !== "exact_chat" &&
      referencedMember.resolution_scope !== "qiwe_platform_user"
    ) {
      errors.push(
        `${label}: referenced_member resolution_scope must be exact_chat or qiwe_platform_user`
      );
      continue;
    }
    const personRef = readPersonRef(
      referencedMember,
      label,
      "referenced_member",
      errors
    );
    if (!personRef) {
      continue;
    }
    resolvedPeople.add(personRef);
    referencedResolvedPeople.add(personRef);
    if (checkSafeProfile(record, referencedMember, label, errors)) {
      referencedProfileHintPeople.add(personRef);
    }
    checkCanonical(record, personRef, label, canonicalPeople, errors);
  } else {
    mentionedRecordCount += 1;
    const label = textField(record, ["expected_mention", "mention_text", "name"]);
    if (!label) {
      errors.push(`canary ${index + 1} is missing expected_mention/name`);
      continue;
    }
    const members = Array.isArray(answerContext.mentioned_members)
      ? answerContext.mentioned_members
      : [];
    const member = selectMentionedMember(members, label);
    if (!member) {
      errors.push(`${label}: mentioned member was not returned`);
      continue;
    }
    if (member.resolved !== true || member.resolution_status !== "resolved") {
      errors.push(
        `${label}: member did not resolve; status=${member.resolution_status ?? "missing"}`
      );
      continue;
    }
    if (member.match_count !== 1) {
      errors.push(`${label}: resolved member must have match_count=1`);
      continue;
    }
    const personRef = readPersonRef(member, label, "member", errors);
    if (!personRef) {
      continue;
    }
    resolvedPeople.add(personRef);
    mentionedResolvedPeople.add(personRef);
    if (checkSafeProfile(record, member, label, errors)) {
      mentionedProfileHintPeople.add(personRef);
    }
    checkCanonical(record, personRef, label, canonicalPeople, errors);
  }
}

if (
  mentionedRecordCount > 0 &&
  speakerRecordCount > 0 &&
  !setsEqual(mentionedResolvedPeople, speakerResolvedPeople)
) {
  errors.push(
    "mentioned-member and speaker self-canary evidence must resolve the same people"
  );
}
if (
  mentionedRecordCount > 0 &&
  referencedRecordCount > 0 &&
  !setsEqual(mentionedResolvedPeople, referencedResolvedPeople)
) {
  errors.push(
    "mentioned-member and referenced-member canary evidence must resolve the same people"
  );
}
if (
  mentionedRecordCount > 0 &&
  speakerRecordCount > 0 &&
  !setsEqual(mentionedProfileHintPeople, speakerProfileHintPeople)
) {
  errors.push(
    "mentioned-member and speaker self-canary profile hint evidence must cover the same people"
  );
}
if (
  mentionedRecordCount > 0 &&
  referencedRecordCount > 0 &&
  !setsEqual(mentionedProfileHintPeople, referencedProfileHintPeople)
) {
  errors.push(
    "mentioned-member and referenced-member profile hint evidence must cover the same people"
  );
}
if (mentionedRecordCount === 0) {
  errors.push("canary evidence must include mentioned-member records");
}
if (speakerRecordCount === 0) {
  errors.push("canary evidence must include speaker self-canary records");
}
if (referencedRecordCount === 0) {
  errors.push("canary evidence must include referenced-member records");
}

if (errors.length > 0) {
  console.error(
    `Erhua member recognition canary check failed: ${records.length} records, ${resolvedPeople.size} resolved people.`
  );
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log(
  `Erhua member recognition canary check passed: ${records.length} records (${mentionedRecordCount} mentioned, ${speakerRecordCount} speaker, ${referencedRecordCount} referenced), ${resolvedPeople.size} resolved people, ${canonicalPeople.size} canonical groups.`
);

function parseCanaryRecords(text) {
  const trimmed = text.trim();
  if (!trimmed) {
    return [];
  }
  if (trimmed.startsWith("[")) {
    const parsed = parseJson(trimmed, "canary array");
    return Array.isArray(parsed) ? parsed : [];
  }
  if (trimmed.startsWith("{")) {
    const parsed = parseJson(trimmed, "canary JSON");
    if (Array.isArray(parsed.canaries)) {
      return parsed.canaries;
    }
    return [parsed];
  }

  const records = [];
  for (const [index, line] of text.split(/\r?\n/).entries()) {
    const trimmedLine = line.trim();
    if (!trimmedLine) {
      continue;
    }
    const prefix = "erhua_member_recognition_canary=";
    if (!trimmedLine.startsWith(prefix)) {
      continue;
    }
    records.push(
      parseJson(trimmedLine.slice(prefix.length), `canary line ${index + 1}`)
    );
  }
  return records;
}

function answerContextFromRecord(record) {
  if (record.answer_context && typeof record.answer_context === "object") {
    return record.answer_context;
  }
  if (record.success === true && Array.isArray(record.mentioned_members)) {
    return record;
  }
  const response = record.mcp_response;
  if (!response || typeof response !== "object") {
    return null;
  }
  const content = response.result?.content;
  if (!Array.isArray(content) || !content[0] || typeof content[0] !== "object") {
    return null;
  }
  const text = asText(content[0].text);
  if (!text) {
    return null;
  }
  return parseJson(text, "MCP answer_context text");
}

function selectMentionedMember(members, label) {
  return members.find((member) => {
    if (!member || typeof member !== "object") {
      return false;
    }
    return member.mention_text === label;
  });
}

function checkSafeProfile(record, target, label, errors) {
  const safeSummary = asText(target.safe_summary);
  if (!safeSummary) {
    errors.push(`${label}: resolved target is missing safe_summary`);
  }
  const hints =
    target.safe_reply_hints &&
    typeof target.safe_reply_hints === "object" &&
    !Array.isArray(target.safe_reply_hints)
      ? target.safe_reply_hints
      : null;
  if (!hints) {
    errors.push(`${label}: resolved target is missing safe_reply_hints`);
    return false;
  } else if (hints.profile_status === "identity_only") {
    if (hints.do_not_infer_missing_profile !== true) {
      errors.push(
        `${label}: identity-only member must set do_not_infer_missing_profile=true`
      );
    }
    if (!safeSummary.includes("暂无") || !safeSummary.includes("画像")) {
      errors.push(
        `${label}: identity-only member safe_summary must state that no stable profile is available`
      );
    }
    return false;
  } else if (!hasSafeProfileHint(hints)) {
    errors.push(`${label}: resolved target is missing non-empty safe profile hints`);
  }
  const requiredTerms = Array.isArray(record.required_profile_terms)
    ? record.required_profile_terms.map(asText).filter(Boolean)
    : [];
  const searchableProfile = `${safeSummary}\n${JSON.stringify(hints ?? {}, null, 2)}`;
  for (const term of requiredTerms) {
    if (!searchableProfile.includes(term)) {
      errors.push(`${label}: profile is missing required term "${term}"`);
    }
  }
  return hints ? hasSafeProfileHint(hints) : false;
}

function hasSafeProfileHint(hints) {
  return ["topics", "stable_profile_notes", "temporary_communication_notes"].some(
    (field) => {
      const value = hints[field];
      return (
        Array.isArray(value) &&
        value.some((item) => typeof item === "string" && item.trim() !== "")
      );
    }
  );
}

function checkCanonical(record, personRef, label, canonicalPeople, errors) {
  const canonicalKey = textField(record, ["canonical_key", "same_person_key"]);
  if (!canonicalKey) {
    return;
  }
  const existingPersonRef = canonicalPeople.get(canonicalKey);
  if (existingPersonRef && existingPersonRef !== personRef) {
    errors.push(`${label}: canonical key ${canonicalKey} resolved to multiple people`);
  } else {
    canonicalPeople.set(canonicalKey, personRef);
  }
}

function readPersonRef(target, label, fieldLabel, errors) {
  const value = asText(target.person_ref);
  if (!isPersonRef(value)) {
    errors.push(`${label}: resolved ${fieldLabel} is missing a valid person_ref`);
    return "";
  }
  return value;
}

function textField(record, fields) {
  for (const field of fields) {
    const value = record[field];
    if (typeof value === "string" && value.trim()) {
      return value.trim();
    }
  }
  return "";
}

function visibleTextFields(value, key = "") {
  if (key === "person_ref" || key === "canonical_key") {
    return [];
  }
  if (typeof value === "string") {
    return [value];
  }
  if (Array.isArray(value)) {
    return value.flatMap((item) => visibleTextFields(item));
  }
  if (value && typeof value === "object") {
    return Object.entries(value).flatMap(([entryKey, entryValue]) =>
      visibleTextFields(entryValue, entryKey)
    );
  }
  return [];
}

function asText(value) {
  return typeof value === "string" ? value.trim() : "";
}

function isPersonRef(value) {
  return typeof value === "string" && /^sha256:[0-9a-f]{64}$/.test(value.trim());
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

function canaryTypeOf(record) {
  const type = textField(record, ["canary_type", "canaryType"]);
  if (!type) {
    return "mentioned_member";
  }
  if (
    type !== "mentioned_member" &&
    type !== "speaker_self" &&
    type !== "referenced_member"
  ) {
    errors.push(`unsupported canary_type: ${type}`);
    return "mentioned_member";
  }
  return type;
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
