#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const args = parseArgs(process.argv.slice(2));
if (!args.summaryPath) {
  fail(
    "usage: node tools/deploy/check-erhua-member-recognition-coverage-summary.mjs <sanitized-coverage-summary.json> [--expect-pass] [--require-active-profiles]"
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
  /"display_name"\s*:/,
  /"safe_summary"\s*:/,
  /"safe_reply_hints"\s*:/,
  /"raw_messages"\s*:/,
  /"hidden_profile_details"\s*:/,
  /"profile_text"\s*:/,
  /"raw"\s*:/,
  /\b[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\b/i,
  /1[3-9]\d{9}/,
]) {
  if (pattern.test(summaryText)) {
    fail(`coverage summary contains forbidden sensitive fragment: ${pattern}`);
  }
}

const summary = parseJson(summaryText, "coverage summary");
const errors = [];
checkAllowedKeys(
  summary,
  "coverage summary",
  [
    "schema_version",
    "passed",
    "strict_profile_required",
    "error_count",
    "warning_count",
    "current_room_qiwe_identities",
    "current_room_potential_member_identities",
    "identity_bootstrap",
    "linked_people",
    "repair_gaps",
    "answer_context_canary_specs",
    "readiness",
    "retained_evidence_boundary",
  ],
  errors
);

if (summary.schema_version !== "erhua_member_recognition_coverage_v1") {
  errors.push("schema_version must be erhua_member_recognition_coverage_v1");
}
if (typeof summary.passed !== "boolean") {
  errors.push("passed must be a boolean");
}
if (typeof summary.strict_profile_required !== "boolean") {
  errors.push("strict_profile_required must be a boolean");
}
const errorCount = readNonNegativeInteger(summary, "error_count", errors);
readNonNegativeInteger(summary, "warning_count", errors);
if (summary.passed === true && errorCount !== undefined && errorCount !== 0) {
  errors.push("passed summaries must have error_count = 0");
}
if (args.expectPass && summary.passed !== true) {
  errors.push("--expect-pass requires passed=true");
}
if (args.requireActiveProfiles && summary.strict_profile_required !== true) {
  errors.push("--require-active-profiles requires strict_profile_required=true");
}

const identities = objectField(summary, "current_room_qiwe_identities", errors);
checkAllowedKeys(
  identities,
  "current_room_qiwe_identities",
  ["raw_total", "safe_total", "linked", "excluded"],
  errors
);
const rawTotal = readNonNegativeInteger(identities, "raw_total", errors);
const safeTotal = readNonNegativeInteger(identities, "safe_total", errors);
const linkedIdentities = readNonNegativeInteger(identities, "linked", errors);
const excludedIdentities = readNonNegativeInteger(identities, "excluded", errors);
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
  linkedIdentities > safeTotal
) {
  errors.push("current-room linked identity count exceeds safe_total");
}

const potentialMembers = objectField(
  summary,
  "current_room_potential_member_identities",
  errors
);
checkAllowedKeys(
  potentialMembers,
  "current_room_potential_member_identities",
  ["total", "linked", "unlinked", "unsafe_display_unlinked"],
  errors
);
const potentialTotal = readNonNegativeInteger(potentialMembers, "total", errors);
const potentialLinked = readNonNegativeInteger(potentialMembers, "linked", errors);
const potentialUnlinked = readNonNegativeInteger(potentialMembers, "unlinked", errors);
const unsafeDisplayUnlinked = readNonNegativeInteger(
  potentialMembers,
  "unsafe_display_unlinked",
  errors
);
if (
  potentialTotal !== undefined &&
  potentialLinked !== undefined &&
  potentialUnlinked !== undefined &&
  potentialLinked + potentialUnlinked !== potentialTotal
) {
  errors.push("current-room potential member identity counts must add up");
}
if (
  unsafeDisplayUnlinked !== undefined &&
  potentialUnlinked !== undefined &&
  unsafeDisplayUnlinked > potentialUnlinked
) {
  errors.push("unsafe_display_unlinked exceeds potential member unlinked count");
}

const identityBootstrap = objectField(summary, "identity_bootstrap", errors);
checkAllowedKeys(
  identityBootstrap,
  "identity_bootstrap",
  [
    "non_ambiguous_unlinked_identities",
    "ambiguous_identities",
    "reused_existing_people",
    "reused_existing_names_or_aliases",
  ],
  errors
);
const nonAmbiguousUnlinked = readNonNegativeInteger(
  identityBootstrap,
  "non_ambiguous_unlinked_identities",
  errors
);
const ambiguousIdentities = readNonNegativeInteger(
  identityBootstrap,
  "ambiguous_identities",
  errors
);
readNonNegativeInteger(identityBootstrap, "reused_existing_people", errors);
readNonNegativeInteger(identityBootstrap, "reused_existing_names_or_aliases", errors);

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
if (
  peopleWithProfile !== undefined &&
  peopleWithoutProfile !== undefined &&
  linkedPeopleTotal !== undefined &&
  peopleWithProfile + peopleWithoutProfile !== linkedPeopleTotal
) {
  errors.push("linked_people profile counts must add up to total");
}
for (const [label, value] of [
  ["without_qiwe_platform_identity", peopleWithoutPlatformIdentity],
  ["without_answer_context_canary_spec", peopleWithoutCanarySpec],
]) {
  if (
    value !== undefined &&
    linkedPeopleTotal !== undefined &&
    value > linkedPeopleTotal
  ) {
    errors.push(`linked_people.${label} exceeds linked_people.total`);
  }
}

const repairGaps = objectField(summary, "repair_gaps", errors);
checkAllowedKeys(
  repairGaps,
  "repair_gaps",
  [
    "linked_aliases_missing",
    "linked_messages_missing_sender_person",
    "qiwe_platform_identities_missing",
    "qiwe_platform_identity_ambiguous_users",
    "running_people_profile_missing_running_hint",
  ],
  errors
);
const linkedAliasesMissing = readNonNegativeInteger(
  repairGaps,
  "linked_aliases_missing",
  errors
);
const linkedMessagesMissingSenderPerson = readNonNegativeInteger(
  repairGaps,
  "linked_messages_missing_sender_person",
  errors
);
const platformIdentitiesMissing = readNonNegativeInteger(
  repairGaps,
  "qiwe_platform_identities_missing",
  errors
);
readNonNegativeInteger(repairGaps, "qiwe_platform_identity_ambiguous_users", errors);
const runningProfileMissing = readNonNegativeInteger(
  repairGaps,
  "running_people_profile_missing_running_hint",
  errors
);

const canaries = objectField(summary, "answer_context_canary_specs", errors);
checkAllowedKeys(
  canaries,
  "answer_context_canary_specs",
  [
    "mentioned_records",
    "mentioned_people",
    "speaker_records",
    "speaker_people",
    "referenced_records",
    "referenced_people",
  ],
  errors
);
readNonNegativeInteger(canaries, "mentioned_records", errors);
const mentionedPeople = readNonNegativeInteger(canaries, "mentioned_people", errors);
readNonNegativeInteger(canaries, "speaker_records", errors);
const speakerPeople = readNonNegativeInteger(canaries, "speaker_people", errors);
readNonNegativeInteger(canaries, "referenced_records", errors);
const referencedPeople = readNonNegativeInteger(canaries, "referenced_people", errors);
for (const [label, value] of [
  ["mentioned_people", mentionedPeople],
  ["speaker_people", speakerPeople],
  ["referenced_people", referencedPeople],
]) {
  if (
    value !== undefined &&
    linkedPeopleTotal !== undefined &&
    value > linkedPeopleTotal
  ) {
    errors.push(`answer_context_canary_specs.${label} exceeds linked_people.total`);
  }
}

const readiness = objectField(summary, "readiness", errors);
checkAllowedKeys(
  readiness,
  "readiness",
  [
    "all_safe_current_room_identities_linked",
    "all_current_room_potential_members_linked",
    "all_linked_people_have_active_profiles",
    "all_linked_people_have_qiwe_platform_identity",
    "all_linked_people_have_canary_names",
    "mentioned_speaker_referenced_canaries_cover_linked_people",
    "running_profile_hints_cover_running_people",
  ],
  errors
);
for (const field of [
  "all_safe_current_room_identities_linked",
  "all_current_room_potential_members_linked",
  "all_linked_people_have_active_profiles",
  "all_linked_people_have_qiwe_platform_identity",
  "all_linked_people_have_canary_names",
  "mentioned_speaker_referenced_canaries_cover_linked_people",
  "running_profile_hints_cover_running_people",
]) {
  readBoolean(readiness, field, errors);
}
checkReadiness(
  readiness,
  "all_safe_current_room_identities_linked",
  linkedIdentities !== undefined &&
    safeTotal !== undefined &&
    linkedIdentities === safeTotal,
  errors
);
checkReadiness(
  readiness,
  "all_current_room_potential_members_linked",
  potentialUnlinked === 0,
  errors
);
checkReadiness(
  readiness,
  "all_linked_people_have_active_profiles",
  linkedPeopleTotal !== undefined &&
    peopleWithProfile !== undefined &&
    peopleWithoutProfile !== undefined &&
    peopleWithProfile === linkedPeopleTotal &&
    peopleWithoutProfile === 0,
  errors
);
checkReadiness(
  readiness,
  "all_linked_people_have_qiwe_platform_identity",
  peopleWithoutPlatformIdentity === 0,
  errors
);
checkReadiness(
  readiness,
  "all_linked_people_have_canary_names",
  peopleWithoutCanarySpec === 0,
  errors
);
checkReadiness(
  readiness,
  "mentioned_speaker_referenced_canaries_cover_linked_people",
  linkedPeopleTotal !== undefined &&
    mentionedPeople === linkedPeopleTotal &&
    speakerPeople === linkedPeopleTotal &&
    referencedPeople === linkedPeopleTotal,
  errors
);
checkReadiness(
  readiness,
  "running_profile_hints_cover_running_people",
  runningProfileMissing === 0,
  errors
);

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
    "includes_profile_text",
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
  ["includes_profile_text", false],
  ["includes_database_url", false],
  ["includes_tokens", false],
]) {
  if (boundary?.[field] !== expected) {
    errors.push(`retained_evidence_boundary.${field} must be ${expected}`);
  }
}

if (args.expectPass) {
  for (const [label, value] of [
    ["non_ambiguous_unlinked_identities", nonAmbiguousUnlinked],
    ["ambiguous_identities", ambiguousIdentities],
    ["linked_aliases_missing", linkedAliasesMissing],
    ["linked_messages_missing_sender_person", linkedMessagesMissingSenderPerson],
    ["qiwe_platform_identities_missing", platformIdentitiesMissing],
    ["running_people_profile_missing_running_hint", runningProfileMissing],
    ["potential_member_unlinked", potentialUnlinked],
    ["linked_people_without_qiwe_platform_identity", peopleWithoutPlatformIdentity],
    ["linked_people_without_answer_context_canary_spec", peopleWithoutCanarySpec],
  ]) {
    if (value !== undefined && value !== 0) {
      errors.push(`--expect-pass requires ${label} = 0`);
    }
  }
  for (const [label, value] of Object.entries(readiness ?? {})) {
    if (value !== true) {
      errors.push(`--expect-pass requires readiness.${label} = true`);
    }
  }
}

if (args.requireActiveProfiles) {
  if (peopleWithoutProfile !== undefined && peopleWithoutProfile !== 0) {
    errors.push(
      "--require-active-profiles requires linked_people.without_active_profile = 0"
    );
  }
  if (readiness?.all_linked_people_have_active_profiles !== true) {
    errors.push(
      "--require-active-profiles requires readiness.all_linked_people_have_active_profiles = true"
    );
  }
}

if (errors.length > 0) {
  console.error("Erhua member recognition coverage summary check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log(
  `Erhua member recognition coverage summary check passed: passed=${summary.passed}, current-room safe identities ${linkedIdentities}/${safeTotal}, potential members ${potentialLinked}/${potentialTotal}, linked people ${linkedPeopleTotal}.`
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

function readBoolean(object, field, errors) {
  const value = object?.[field];
  if (typeof value !== "boolean") {
    errors.push(`${field} must be a boolean`);
    return undefined;
  }
  return value;
}

function checkReadiness(object, field, expected, errors) {
  if (object?.[field] !== expected) {
    errors.push(`readiness.${field} must match count-derived readiness`);
  }
}

function parseArgs(argv) {
  const parsed = { expectPass: false, requireActiveProfiles: false };
  for (const arg of argv) {
    if (arg === "--expect-pass") {
      parsed.expectPass = true;
    } else if (arg === "--require-active-profiles") {
      parsed.requireActiveProfiles = true;
    } else if (!parsed.summaryPath) {
      parsed.summaryPath = arg;
    } else {
      fail(`unknown argument: ${arg}`);
    }
  }
  return parsed;
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
