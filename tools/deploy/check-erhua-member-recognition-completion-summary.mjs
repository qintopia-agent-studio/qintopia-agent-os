#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const args = parseArgs(process.argv.slice(2));
if (!args.summaryPath) {
  fail(
    "usage: node tools/deploy/check-erhua-member-recognition-completion-summary.mjs <sanitized-completion-summary.json> [--require-active-profiles]"
  );
}

const summaryPath = path.resolve(args.summaryPath);
const summaryText = fs.readFileSync(summaryPath, "utf8");

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
  /"canonical_key"\s*:/,
  /"raw_messages"\s*:/,
  /"hidden_profile_details"\s*:/,
  /"raw"\s*:/,
  /\b[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\b/i,
  /1[3-9]\d{9}/,
]) {
  if (pattern.test(summaryText)) {
    fail(`completion summary contains forbidden sensitive fragment: ${pattern}`);
  }
}

const summary = parseJson(summaryText, "completion summary");
const errors = [];
checkAllowedKeys(
  summary,
  "completion summary",
  [
    "schema_version",
    "passed",
    "scope_fingerprint",
    "room_sync",
    "current_room_qiwe_identities",
    "linked_people",
    "profile_repair",
    "running_profile_hints",
    "answer_context_canaries",
    "retained_evidence_boundary",
  ],
  errors
);

if (summary.schema_version !== "erhua_member_recognition_completion_v1") {
  errors.push("schema_version must be erhua_member_recognition_completion_v1");
}
if (summary.passed !== true) {
  errors.push("passed must be true");
}
const scopeFingerprint = readScopeFingerprint(summary, "scope_fingerprint", errors);

const roomSync = objectField(summary, "room_sync", errors);
checkAllowedKeys(
  roomSync,
  "room_sync",
  [
    "source",
    "dry_run",
    "room_members_discovered",
    "room_member_identities_upserted",
    "stale_room_member_identities_marked",
  ],
  errors
);
const roomMembersDiscovered = readNonNegativeInteger(
  roomSync,
  "room_members_discovered",
  errors
);
const roomMemberIdentitiesUpserted = readNonNegativeInteger(
  roomSync,
  "room_member_identities_upserted",
  errors
);
readNonNegativeInteger(roomSync, "stale_room_member_identities_marked", errors);
if (roomSync?.source !== "current_qiwe_room_member_roster") {
  errors.push("room_sync.source must be current_qiwe_room_member_roster");
}
if (roomSync?.dry_run !== false) {
  errors.push("room_sync.dry_run must be false");
}
if (roomMembersDiscovered !== undefined && roomMembersDiscovered <= 0) {
  errors.push("room_sync.room_members_discovered must be greater than zero");
}
if (
  roomMembersDiscovered !== undefined &&
  roomMemberIdentitiesUpserted !== undefined &&
  roomMemberIdentitiesUpserted !== roomMembersDiscovered
) {
  errors.push("room_sync must upsert every discovered room member identity");
}

const identities = objectField(summary, "current_room_qiwe_identities", errors);
checkAllowedKeys(
  identities,
  "current_room_qiwe_identities",
  [
    "raw_total",
    "safe_total",
    "linked",
    "excluded",
    "potential_member_total",
    "potential_member_linked",
    "potential_member_unlinked",
  ],
  errors
);
const rawTotal = readNonNegativeInteger(identities, "raw_total", errors);
const safeTotal = readNonNegativeInteger(identities, "safe_total", errors);
const linkedIdentities = readNonNegativeInteger(identities, "linked", errors);
const excludedIdentities = readNonNegativeInteger(identities, "excluded", errors);
const potentialMemberTotal = readNonNegativeInteger(
  identities,
  "potential_member_total",
  errors
);
const potentialMemberLinked = readNonNegativeInteger(
  identities,
  "potential_member_linked",
  errors
);
const potentialMemberUnlinked = readNonNegativeInteger(
  identities,
  "potential_member_unlinked",
  errors
);
if (
  rawTotal !== undefined &&
  safeTotal !== undefined &&
  excludedIdentities !== undefined &&
  safeTotal + excludedIdentities !== rawTotal
) {
  errors.push(
    "current-room safe and excluded identity counts must add up to raw_total"
  );
}
if (
  safeTotal !== undefined &&
  linkedIdentities !== undefined &&
  linkedIdentities !== safeTotal
) {
  errors.push("all current-room safe identities must be linked");
}
if (
  potentialMemberTotal !== undefined &&
  safeTotal !== undefined &&
  potentialMemberTotal < safeTotal
) {
  errors.push(
    "current-room potential member identity total must be at least safe_total"
  );
}
if (
  potentialMemberTotal !== undefined &&
  rawTotal !== undefined &&
  potentialMemberTotal > rawTotal
) {
  errors.push("current-room potential member identity total exceeds raw_total");
}
if (
  potentialMemberTotal !== undefined &&
  potentialMemberLinked !== undefined &&
  potentialMemberUnlinked !== undefined &&
  potentialMemberLinked + potentialMemberUnlinked !== potentialMemberTotal
) {
  errors.push("current-room potential member identity counts must add up");
}
if (potentialMemberUnlinked !== undefined && potentialMemberUnlinked !== 0) {
  errors.push("current-room potential member identities must all be linked");
}
if (
  potentialMemberLinked !== undefined &&
  linkedIdentities !== undefined &&
  potentialMemberLinked < linkedIdentities
) {
  errors.push(
    "current-room potential member linked identities must include all linked safe identities"
  );
}
if (
  roomMembersDiscovered !== undefined &&
  rawTotal !== undefined &&
  rawTotal !== roomMembersDiscovered
) {
  errors.push("current-room raw identity count must match synced room roster");
}

const linkedPeople = objectField(summary, "linked_people", errors);
checkAllowedKeys(
  linkedPeople,
  "linked_people",
  [
    "total",
    "with_active_profile",
    "without_active_profile",
    "without_qiwe_platform_identity",
    "without_answer_context_canary_spec",
  ],
  errors
);
const linkedPeopleTotal = readNonNegativeInteger(linkedPeople, "total", errors);
const peopleWithProfile = readNonNegativeInteger(
  linkedPeople,
  "with_active_profile",
  errors
);
const peopleWithoutProfile = readNonNegativeInteger(
  linkedPeople,
  "without_active_profile",
  errors
);
const peopleWithoutPlatformIdentity = readNonNegativeInteger(
  linkedPeople,
  "without_qiwe_platform_identity",
  errors
);
const peopleWithoutCanarySpec = readNonNegativeInteger(
  linkedPeople,
  "without_answer_context_canary_spec",
  errors
);
if (linkedPeopleTotal !== undefined && linkedPeopleTotal <= 0) {
  errors.push("linked_people.total must be greater than zero");
}
if (
  peopleWithProfile !== undefined &&
  peopleWithoutProfile !== undefined &&
  linkedPeopleTotal !== undefined &&
  peopleWithProfile + peopleWithoutProfile !== linkedPeopleTotal
) {
  errors.push("linked_people profile counts must add up to total");
}
if (
  peopleWithoutPlatformIdentity !== undefined &&
  peopleWithoutPlatformIdentity !== 0
) {
  errors.push("linked people must all have QiWe platform identities");
}
if (peopleWithoutCanarySpec !== undefined && peopleWithoutCanarySpec !== 0) {
  errors.push("linked people must all have answer-context canary specs");
}
if (
  linkedPeopleTotal !== undefined &&
  potentialMemberLinked !== undefined &&
  linkedPeopleTotal > potentialMemberLinked
) {
  errors.push("linked_people.total exceeds linked potential member identities");
}
if (args.requireActiveProfiles) {
  if (peopleWithoutProfile !== undefined && peopleWithoutProfile !== 0) {
    errors.push(
      "linked people must all have active reply_context profiles for full-profile completion"
    );
  }
  if (
    peopleWithProfile !== undefined &&
    linkedPeopleTotal !== undefined &&
    peopleWithProfile !== linkedPeopleTotal
  ) {
    errors.push(
      "linked_people.with_active_profile must match linked_people.total for full-profile completion"
    );
  }
}

const profileRepair = objectField(summary, "profile_repair", errors);
checkAllowedKeys(
  profileRepair,
  "profile_repair",
  ["dry_run", "requested_message_limit", "messages_scanned", "valuable_messages"],
  errors
);
const requestedMessageLimit = readNonNegativeInteger(
  profileRepair,
  "requested_message_limit",
  errors
);
const messagesScanned = readNonNegativeInteger(
  profileRepair,
  "messages_scanned",
  errors
);
readNonNegativeInteger(profileRepair, "valuable_messages", errors);
if (profileRepair?.dry_run !== false) {
  errors.push("profile_repair.dry_run must be false");
}
if (requestedMessageLimit !== undefined && requestedMessageLimit < 5000) {
  errors.push("profile_repair.requested_message_limit must be at least 5000");
}
if (messagesScanned !== undefined && messagesScanned <= 0) {
  errors.push("profile_repair.messages_scanned must be greater than zero");
}

const runningProfileHints = objectField(summary, "running_profile_hints", errors);
checkAllowedKeys(
  runningProfileHints,
  "running_profile_hints",
  [
    "linked_people_with_running_facts",
    "running_people_with_profile_running_hint",
    "running_people_profile_missing_running_hint",
  ],
  errors
);
const runningFacts = readNonNegativeInteger(
  runningProfileHints,
  "linked_people_with_running_facts",
  errors
);
const runningHints = readNonNegativeInteger(
  runningProfileHints,
  "running_people_with_profile_running_hint",
  errors
);
const runningMissingHints = readNonNegativeInteger(
  runningProfileHints,
  "running_people_profile_missing_running_hint",
  errors
);
if (
  runningFacts !== undefined &&
  runningHints !== undefined &&
  runningMissingHints !== undefined &&
  runningHints + runningMissingHints !== runningFacts
) {
  errors.push("running profile hint counts must add up to running facts");
}
if (runningMissingHints !== undefined && runningMissingHints !== 0) {
  errors.push("running facts must all have running profile hints");
}

const canaries = objectField(summary, "answer_context_canaries", errors);
checkAllowedKeys(
  canaries,
  "answer_context_canaries",
  [
    "mentioned_records",
    "speaker_records",
    "referenced_records",
    "mentioned_people_resolved",
    "speaker_people_resolved",
    "referenced_people_resolved",
    "linked_people_resolved",
    "mentioned_profile_hint_people",
    "speaker_profile_hint_people",
    "referenced_profile_hint_people",
    "linked_profile_hint_people",
    "identity_only_people",
  ],
  errors
);
const mentionedRecords = readNonNegativeInteger(canaries, "mentioned_records", errors);
const speakerRecords = readNonNegativeInteger(canaries, "speaker_records", errors);
const referencedRecords = readNonNegativeInteger(
  canaries,
  "referenced_records",
  errors
);
const mentionedPeopleResolved = readNonNegativeInteger(
  canaries,
  "mentioned_people_resolved",
  errors
);
const speakerPeopleResolved = readNonNegativeInteger(
  canaries,
  "speaker_people_resolved",
  errors
);
const referencedPeopleResolved = readNonNegativeInteger(
  canaries,
  "referenced_people_resolved",
  errors
);
const linkedPeopleResolved = readNonNegativeInteger(
  canaries,
  "linked_people_resolved",
  errors
);
const mentionedProfileHintPeople = readNonNegativeInteger(
  canaries,
  "mentioned_profile_hint_people",
  errors
);
const speakerProfileHintPeople = readNonNegativeInteger(
  canaries,
  "speaker_profile_hint_people",
  errors
);
const referencedProfileHintPeople = readNonNegativeInteger(
  canaries,
  "referenced_profile_hint_people",
  errors
);
const linkedProfileHintPeople = readNonNegativeInteger(
  canaries,
  "linked_profile_hint_people",
  errors
);
const identityOnlyPeople = readNonNegativeInteger(
  canaries,
  "identity_only_people",
  errors
);
if (mentionedRecords !== undefined && mentionedRecords <= 0) {
  errors.push("answer_context_canaries.mentioned_records must be greater than zero");
}
if (speakerRecords !== undefined && speakerRecords <= 0) {
  errors.push("answer_context_canaries.speaker_records must be greater than zero");
}
if (referencedRecords !== undefined && referencedRecords <= 0) {
  errors.push("answer_context_canaries.referenced_records must be greater than zero");
}
for (const [label, value] of [
  ["mentioned_people_resolved", mentionedPeopleResolved],
  ["speaker_people_resolved", speakerPeopleResolved],
  ["referenced_people_resolved", referencedPeopleResolved],
  ["linked_people_resolved", linkedPeopleResolved],
]) {
  if (
    value !== undefined &&
    linkedPeopleTotal !== undefined &&
    value !== linkedPeopleTotal
  ) {
    errors.push(`answer_context_canaries.${label} must match linked_people.total`);
  }
}
for (const [label, value] of [
  ["mentioned_profile_hint_people", mentionedProfileHintPeople],
  ["speaker_profile_hint_people", speakerProfileHintPeople],
  ["referenced_profile_hint_people", referencedProfileHintPeople],
  ["linked_profile_hint_people", linkedProfileHintPeople],
]) {
  if (
    value !== undefined &&
    linkedPeopleTotal !== undefined &&
    value > linkedPeopleTotal
  ) {
    errors.push(`answer_context_canaries.${label} exceeds linked_people.total`);
  }
}
for (const [label, value] of [
  ["speaker_profile_hint_people", speakerProfileHintPeople],
  ["referenced_profile_hint_people", referencedProfileHintPeople],
  ["linked_profile_hint_people", linkedProfileHintPeople],
]) {
  if (
    value !== undefined &&
    mentionedProfileHintPeople !== undefined &&
    value !== mentionedProfileHintPeople
  ) {
    errors.push(
      `answer_context_canaries.${label} must match answer_context_canaries.mentioned_profile_hint_people`
    );
  }
}
if (
  identityOnlyPeople !== undefined &&
  peopleWithoutProfile !== undefined &&
  identityOnlyPeople !== peopleWithoutProfile
) {
  errors.push("identity-only canary people must match linked people without profiles");
}
if (args.requireActiveProfiles && identityOnlyPeople !== undefined) {
  if (identityOnlyPeople !== 0) {
    errors.push("identity-only canary people must be zero for full-profile completion");
  }
}
if (args.requireActiveProfiles) {
  for (const [label, value] of [
    ["mentioned_profile_hint_people", mentionedProfileHintPeople],
    ["speaker_profile_hint_people", speakerProfileHintPeople],
    ["referenced_profile_hint_people", referencedProfileHintPeople],
    ["linked_profile_hint_people", linkedProfileHintPeople],
  ]) {
    if (
      value !== undefined &&
      linkedPeopleTotal !== undefined &&
      value !== linkedPeopleTotal
    ) {
      errors.push(
        `answer_context_canaries.${label} must match linked_people.total for full-profile completion`
      );
    }
  }
}

const boundary = objectField(summary, "retained_evidence_boundary", errors);
checkAllowedKeys(
  boundary,
  "retained_evidence_boundary",
  [
    "sanitized_summary_only",
    "includes_chat_id",
    "includes_sender_id",
    "includes_channel_user_id",
    "includes_person_id",
    "includes_raw_messages",
    "includes_hidden_profile_details",
    "includes_database_url",
    "includes_tokens",
  ],
  errors
);
for (const [field, expected] of [
  ["sanitized_summary_only", true],
  ["includes_chat_id", false],
  ["includes_sender_id", false],
  ["includes_channel_user_id", false],
  ["includes_person_id", false],
  ["includes_raw_messages", false],
  ["includes_hidden_profile_details", false],
  ["includes_database_url", false],
  ["includes_tokens", false],
]) {
  if (boundary?.[field] !== expected) {
    errors.push(`retained_evidence_boundary.${field} must be ${expected}`);
  }
}

if (errors.length > 0) {
  console.error("Erhua member recognition completion summary check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log(
  `Erhua member recognition completion summary check passed: ${roomMembersDiscovered} synced room members, ${linkedPeopleTotal} linked people, ${mentionedRecords} mentioned canaries, ${speakerRecords} speaker canaries, ${referencedRecords} referenced canaries, scope=${scopeFingerprint}.`
);

function objectField(object, field, errors) {
  const value = object?.[field];
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    errors.push(`${field} must be an object`);
    return undefined;
  }
  return value;
}

function checkAllowedKeys(object, label, allowedKeys, errors) {
  if (!object || typeof object !== "object" || Array.isArray(object)) {
    return;
  }
  const allowed = new Set(allowedKeys);
  for (const key of Object.keys(object)) {
    if (!allowed.has(key)) {
      errors.push(`${label} contains unsupported field: ${key}`);
    }
  }
}

function readNonNegativeInteger(object, field, errors) {
  const value = object?.[field];
  if (!Number.isInteger(value) || value < 0) {
    errors.push(`${field} must be a non-negative integer`);
    return undefined;
  }
  return value;
}

function readScopeFingerprint(object, field, errors) {
  const value = object?.[field];
  if (typeof value !== "string" || !/^sha256:[0-9a-f]{64}$/.test(value.trim())) {
    errors.push(`${field} must be a canonical sha256 marker`);
    return undefined;
  }
  return value.trim();
}

function parseJson(text, label) {
  try {
    return JSON.parse(text);
  } catch (error) {
    fail(`${label} is not valid JSON: ${error.message}`);
  }
}

function parseArgs(argv) {
  const parsed = {};
  for (const arg of argv) {
    if (arg === "--require-active-profiles") {
      parsed.requireActiveProfiles = true;
    } else if (!parsed.summaryPath) {
      parsed.summaryPath = arg;
    } else {
      fail(`unknown argument: ${arg}`);
    }
  }
  return parsed;
}

function fail(message) {
  console.error(message);
  process.exit(1);
}
