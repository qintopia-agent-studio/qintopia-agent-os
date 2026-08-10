#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const args = parseArgs(process.argv.slice(2));
if (!args.evidence) {
  fail(
    "usage: node tools/deploy/check-erhua-member-recognition-coverage.mjs <identity-bootstrap-dry-run-output.json> [--require-active-profiles] [--summary-output <sanitized-coverage-summary.json>]"
  );
}

const evidenceFile = path.resolve(args.evidence);
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
  /1[3-9]\d{9}/,
]) {
  if (pattern.test(evidenceText)) {
    fail(`evidence contains forbidden sensitive fragment: ${pattern}`);
  }
}

const coverage = parseCoverage(evidenceText);
const requiredNumericFields = [
  "qiwe_channel_identities_raw_total",
  "qiwe_room_channel_identities_raw_total",
  "qiwe_room_channel_identities_total",
  "qiwe_room_channel_identities_linked",
  "qiwe_room_channel_identities_excluded",
  "qiwe_room_potential_member_identities_total",
  "qiwe_room_potential_member_identities_linked",
  "qiwe_room_potential_member_identities_unlinked",
  "total_channel_identities",
  "qiwe_channel_identities_total",
  "qiwe_channel_identities_linked",
  "qiwe_channel_identities_excluded",
  "channel_identities_with_existing_person",
  "channel_identities_with_existing_name",
  "ambiguous_channel_identities_skipped",
  "linked_aliases_missing",
  "linked_messages_missing_sender_person",
  "linked_people_total",
  "linked_people_with_active_profile",
  "linked_people_without_active_profile",
  "qiwe_platform_identity_materializable_users",
  "qiwe_platform_identities_missing",
  "qiwe_platform_identity_ambiguous_users",
  "linked_people_without_qiwe_platform_identity",
  "linked_people_with_running_facts",
  "running_people_with_profile_running_hint",
  "running_people_profile_missing_running_hint",
  "answer_context_canary_specs_total",
  "answer_context_canary_people_total",
  "answer_context_speaker_canary_specs_total",
  "answer_context_speaker_canary_people_total",
  "answer_context_referenced_canary_specs_total",
  "answer_context_referenced_canary_people_total",
  "linked_people_without_answer_context_canary_spec",
];

const errors = [];
const warnings = [];
const values = {};
for (const field of requiredNumericFields) {
  values[field] = readNonNegativeInteger(coverage, field, errors);
}
const mentionCanarySpecs = readCanarySpecs(
  coverage,
  "answer_context_canary_specs",
  "mentioned_member",
  values.answer_context_canary_specs_total,
  errors
);
const speakerCanarySpecs = readCanarySpecs(
  coverage,
  "answer_context_speaker_canary_specs",
  "speaker_self",
  values.answer_context_speaker_canary_specs_total,
  errors
);
const referencedCanarySpecs = readCanarySpecs(
  coverage,
  "answer_context_referenced_canary_specs",
  "referenced_member",
  values.answer_context_referenced_canary_specs_total,
  errors
);

if (errors.length === 0) {
  if (values.qiwe_channel_identities_total <= 0) {
    errors.push("QiWe channel identity total must be greater than zero");
  }
  if (
    values.qiwe_channel_identities_total + values.qiwe_channel_identities_excluded !==
    values.qiwe_channel_identities_raw_total
  ) {
    errors.push("safe and excluded QiWe identity counts do not add up to raw total");
  }
  if (
    values.qiwe_room_channel_identities_total +
      values.qiwe_room_channel_identities_excluded !==
    values.qiwe_room_channel_identities_raw_total
  ) {
    errors.push(
      "safe and excluded current-room QiWe identity counts do not add up to current-room raw total"
    );
  }
  if (values.qiwe_channel_identities_linked > values.qiwe_channel_identities_total) {
    errors.push("linked QiWe channel identity count exceeds total count");
  }
  if (
    values.qiwe_room_channel_identities_linked >
    values.qiwe_room_channel_identities_total
  ) {
    errors.push(
      "linked current-room QiWe channel identity count exceeds current-room total count"
    );
  }
  if (
    values.qiwe_room_channel_identities_raw_total >
    values.qiwe_channel_identities_raw_total
  ) {
    errors.push("current-room QiWe identity raw total exceeds all scoped raw total");
  }
  if (
    values.qiwe_room_potential_member_identities_linked +
      values.qiwe_room_potential_member_identities_unlinked !==
    values.qiwe_room_potential_member_identities_total
  ) {
    errors.push("current-room potential member identity counts do not add up");
  }
  if (
    values.qiwe_room_potential_member_identities_total >
    values.qiwe_room_channel_identities_raw_total
  ) {
    errors.push(
      "current-room potential member identity total exceeds current-room raw total"
    );
  }
  if (
    values.qiwe_channel_identities_raw_total !==
      values.qiwe_room_channel_identities_raw_total ||
    values.qiwe_channel_identities_total !==
      values.qiwe_room_channel_identities_total ||
    values.qiwe_channel_identities_linked !==
      values.qiwe_room_channel_identities_linked ||
    values.qiwe_channel_identities_excluded !==
      values.qiwe_room_channel_identities_excluded
  ) {
    errors.push(
      "QiWe identity coverage counts must be room-scoped and match current-room counts; platform identities are checked separately"
    );
  }
  if (
    values.linked_people_with_active_profile +
      values.linked_people_without_active_profile !==
    values.linked_people_total
  ) {
    errors.push("linked people profile counts do not add up to total");
  }
  if (
    values.running_people_with_profile_running_hint +
      values.running_people_profile_missing_running_hint !==
    values.linked_people_with_running_facts
  ) {
    errors.push("running profile hint counts do not add up to running people total");
  }
  if (
    values.qiwe_platform_identities_missing >
    values.qiwe_platform_identity_materializable_users
  ) {
    errors.push("missing QiWe platform identity count exceeds materializable users");
  }
  if (values.answer_context_canary_people_total > values.linked_people_total) {
    errors.push("answer-context canary people count exceeds linked people total");
  }
  if (
    uniqueCanonicalKeyCount(mentionCanarySpecs) !==
    values.answer_context_canary_people_total
  ) {
    errors.push(
      "answer-context canary specs unique people count must match answer_context_canary_people_total"
    );
  }
  if (values.answer_context_speaker_canary_people_total > values.linked_people_total) {
    errors.push(
      "answer-context speaker self-canary people count exceeds linked people total"
    );
  }
  if (
    uniqueCanonicalKeyCount(speakerCanarySpecs) !==
    values.answer_context_speaker_canary_people_total
  ) {
    errors.push(
      "answer-context speaker self-canary specs unique people count must match answer_context_speaker_canary_people_total"
    );
  }
  if (
    values.answer_context_referenced_canary_people_total > values.linked_people_total
  ) {
    errors.push(
      "answer-context referenced-member canary people count exceeds linked people total"
    );
  }
  if (
    uniqueCanonicalKeyCount(referencedCanarySpecs) !==
    values.answer_context_referenced_canary_people_total
  ) {
    errors.push(
      "answer-context referenced-member canary specs unique people count must match answer_context_referenced_canary_people_total"
    );
  }
  if (!canonicalKeySetsEqual(mentionCanarySpecs, speakerCanarySpecs)) {
    errors.push(
      "answer-context mentioned-member and speaker self-canary specs must cover the same canonical people"
    );
  }
  if (!canonicalKeySetsEqual(mentionCanarySpecs, referencedCanarySpecs)) {
    errors.push(
      "answer-context mentioned-member and referenced-member canary specs must cover the same canonical people"
    );
  }
  if (
    values.linked_people_without_qiwe_platform_identity > values.linked_people_total
  ) {
    errors.push(
      "linked people without QiWe platform identity exceeds linked people total"
    );
  }

  const actionableUnlinked =
    values.total_channel_identities - values.ambiguous_channel_identities_skipped;
  if (actionableUnlinked > 0) {
    errors.push(
      `identity bootstrap apply is still required for ${actionableUnlinked} non-ambiguous QiWe channel identities${sampleSuffix(
        coverage,
        ["unlinked_channel_identity_samples", "unlinked_channel_identities"]
      )}`
    );
  }
  if (values.linked_aliases_missing > 0) {
    errors.push(
      `linked QiWe display names are missing person aliases: ${values.linked_aliases_missing}${sampleSuffix(
        coverage,
        [
          "linked_aliases_missing_samples",
          "missing_aliases",
          "linked_aliases_missing_details",
        ]
      )}`
    );
  }
  if (values.linked_messages_missing_sender_person > 0) {
    errors.push(
      `linked QiWe messages still miss sender_person_id: ${values.linked_messages_missing_sender_person}${sampleSuffix(
        coverage,
        [
          "linked_messages_missing_sender_person_samples",
          "messages_missing_sender_person_samples",
        ]
      )}`
    );
  }
  if (values.qiwe_platform_identities_missing > 0) {
    errors.push(
      `linked QiWe users are missing platform identities: ${values.qiwe_platform_identities_missing}${sampleSuffix(
        coverage,
        [
          "qiwe_platform_identities_missing_samples",
          "missing_qiwe_platform_identity_samples",
        ]
      )}`
    );
  }
  if (values.linked_people_without_qiwe_platform_identity > 0) {
    errors.push(
      `linked people are missing QiWe platform identity for speaker recognition: ${values.linked_people_without_qiwe_platform_identity}`
    );
  }
  if (values.ambiguous_channel_identities_skipped > 0) {
    warnings.push(
      `manual merge needed for ${values.ambiguous_channel_identities_skipped} ambiguous QiWe channel identities${sampleSuffix(
        coverage,
        [
          "ambiguous_channel_identity_samples",
          "ambiguous_channel_identities",
          "ambiguous_channel_identities_skipped_samples",
        ]
      )}`
    );
  }
  const unsafePotentialMemberUnlinked =
    values.qiwe_room_potential_member_identities_unlinked -
    values.total_channel_identities;
  if (unsafePotentialMemberUnlinked > 0) {
    errors.push(
      `current-room unsafe-display potential member identities are still unlinked: ${unsafePotentialMemberUnlinked}${sampleSuffix(
        coverage,
        [
          "qiwe_room_potential_member_identities_unlinked_samples",
          "potential_member_identity_unlinked_samples",
        ]
      )}`
    );
  }
  if (values.qiwe_platform_identity_ambiguous_users > 0) {
    warnings.push(
      `manual merge needed for ${values.qiwe_platform_identity_ambiguous_users} QiWe users linked to multiple people`
    );
  }
  if (values.linked_people_without_answer_context_canary_spec > 0) {
    errors.push(
      `${values.linked_people_without_answer_context_canary_spec} linked people have no safe answer-context canary name; add a reviewed safe alias before claiming full member recognition coverage${sampleSuffix(
        coverage,
        [
          "linked_people_without_answer_context_canary_spec_samples",
          "missing_answer_context_canary_name_samples",
        ]
      )}`
    );
  }
  if (
    values.answer_context_speaker_canary_people_total !== values.linked_people_total
  ) {
    errors.push(
      `speaker self-canary specs must cover every linked person: ${values.answer_context_speaker_canary_people_total}/${values.linked_people_total}`
    );
  }
  if (
    values.answer_context_referenced_canary_people_total !== values.linked_people_total
  ) {
    errors.push(
      `referenced-member canary specs must cover every linked person: ${values.answer_context_referenced_canary_people_total}/${values.linked_people_total}`
    );
  }
  if (values.linked_people_without_active_profile > 0) {
    const message = `${
      values.linked_people_without_active_profile
    } linked people have no active reply_context profile${sampleSuffix(coverage, [
      "linked_people_without_active_profile_samples",
      "profile_missing_samples",
    ])}`;
    if (args.requireActiveProfiles) {
      errors.push(`full-profile coverage requires active profiles: ${message}`);
    } else {
      warnings.push(message);
    }
  }
  if (values.running_people_profile_missing_running_hint > 0) {
    errors.push(
      `linked people have running facts but no running profile hint: ${values.running_people_profile_missing_running_hint}${sampleSuffix(
        coverage,
        [
          "running_people_profile_missing_running_hint_samples",
          "running_profile_missing_samples",
        ]
      )}`
    );
  }
}

const linkedRatio = ratio(
  values.qiwe_channel_identities_linked ?? 0,
  values.qiwe_channel_identities_total ?? 0
);
const roomLinkedRatio = ratio(
  values.qiwe_room_channel_identities_linked ?? 0,
  values.qiwe_room_channel_identities_total ?? 0
);
const summary =
  `QiWe identities linked ${values.qiwe_channel_identities_linked ?? 0}/${
    values.qiwe_channel_identities_total ?? 0
  } safe (${linkedRatio}), excluded ${
    values.qiwe_channel_identities_excluded ?? 0
  }, raw ${values.qiwe_channel_identities_raw_total ?? 0}; ` +
  `current-room identities linked ${
    values.qiwe_room_channel_identities_linked ?? 0
  }/${values.qiwe_room_channel_identities_total ?? 0} safe (${roomLinkedRatio}), excluded ${
    values.qiwe_room_channel_identities_excluded ?? 0
  }, raw ${values.qiwe_room_channel_identities_raw_total ?? 0}; ` +
  `current-room potential members linked ${
    values.qiwe_room_potential_member_identities_linked ?? 0
  }/${values.qiwe_room_potential_member_identities_total ?? 0}; ` +
  `bootstrap reuse user/name ${values.channel_identities_with_existing_person ?? 0}/${
    values.channel_identities_with_existing_name ?? 0
  }; ` +
  `linked people ${values.linked_people_total ?? 0}, active profiles ${
    values.linked_people_with_active_profile ?? 0
  }/${values.linked_people_total ?? 0}; ` +
  `platform identities ${
    (values.qiwe_platform_identity_materializable_users ?? 0) -
    (values.qiwe_platform_identities_missing ?? 0)
  }/${values.qiwe_platform_identity_materializable_users ?? 0}, speaker-ready people ${
    (values.linked_people_total ?? 0) -
    (values.linked_people_without_qiwe_platform_identity ?? 0)
  }/${values.linked_people_total ?? 0}; ` +
  `answer-context canary people ${values.answer_context_canary_people_total ?? 0}/${
    values.linked_people_total ?? 0
  }, speaker self-canary people ${
    values.answer_context_speaker_canary_people_total ?? 0
  }/${values.linked_people_total ?? 0}, referenced-member canary people ${
    values.answer_context_referenced_canary_people_total ?? 0
  }/${values.linked_people_total ?? 0}; ` +
  `running profile hints ${values.running_people_with_profile_running_hint ?? 0}/${
    values.linked_people_with_running_facts ?? 0
  }; ` +
  `speaker self-canary specs ${values.answer_context_speaker_canary_specs_total ?? 0}, ` +
  `referenced-member canary specs ${values.answer_context_referenced_canary_specs_total ?? 0}.`;

const coverageSummary = buildCoverageSummary({
  values,
  errors,
  warnings,
  requireActiveProfiles: args.requireActiveProfiles,
});
if (args.summaryOutput) {
  fs.writeFileSync(
    path.resolve(args.summaryOutput),
    `${JSON.stringify(coverageSummary, null, 2)}\n`,
    "utf8"
  );
}

if (errors.length > 0) {
  console.error(`Erhua member recognition coverage check failed: ${summary}`);
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  for (const warning of warnings) {
    console.error(`- warning: ${warning}`);
  }
  process.exit(1);
}

console.log(`Erhua member recognition coverage check passed: ${summary}`);
for (const warning of warnings) {
  console.log(`warning: ${warning}`);
}

function parseArgs(argv) {
  const parsed = { requireActiveProfiles: false };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--require-active-profiles") {
      parsed.requireActiveProfiles = true;
    } else if (arg === "--summary-output") {
      parsed.summaryOutput = argv[++index];
    } else if (!parsed.evidence) {
      parsed.evidence = arg;
    } else {
      fail(`unknown argument: ${arg}`);
    }
  }
  return parsed;
}

function buildCoverageSummary({ values, errors, warnings, requireActiveProfiles }) {
  const linkedPeopleTotal = asCount(values.linked_people_total);
  const linkedPeopleWithActiveProfile = asCount(
    values.linked_people_with_active_profile
  );
  const linkedPeopleWithoutActiveProfile = asCount(
    values.linked_people_without_active_profile
  );
  const answerContextPeople = asCount(values.answer_context_canary_people_total);
  const speakerPeople = asCount(values.answer_context_speaker_canary_people_total);
  const referencedPeople = asCount(
    values.answer_context_referenced_canary_people_total
  );
  return {
    schema_version: "erhua_member_recognition_coverage_v1",
    passed: errors.length === 0,
    strict_profile_required: requireActiveProfiles,
    error_count: errors.length,
    warning_count: warnings.length,
    current_room_qiwe_identities: {
      raw_total: asCount(values.qiwe_room_channel_identities_raw_total),
      safe_total: asCount(values.qiwe_room_channel_identities_total),
      linked: asCount(values.qiwe_room_channel_identities_linked),
      excluded: asCount(values.qiwe_room_channel_identities_excluded),
    },
    current_room_potential_member_identities: {
      total: asCount(values.qiwe_room_potential_member_identities_total),
      linked: asCount(values.qiwe_room_potential_member_identities_linked),
      unlinked: asCount(values.qiwe_room_potential_member_identities_unlinked),
      unsafe_display_unlinked: nonNegativeDifference(
        values.qiwe_room_potential_member_identities_unlinked,
        values.total_channel_identities
      ),
    },
    identity_bootstrap: {
      non_ambiguous_unlinked_identities: nonNegativeDifference(
        values.total_channel_identities,
        values.ambiguous_channel_identities_skipped
      ),
      ambiguous_identities: asCount(values.ambiguous_channel_identities_skipped),
      reused_existing_people: asCount(values.channel_identities_with_existing_person),
      reused_existing_names_or_aliases: asCount(
        values.channel_identities_with_existing_name
      ),
    },
    linked_people: {
      total: linkedPeopleTotal,
      with_active_profile: linkedPeopleWithActiveProfile,
      without_active_profile: linkedPeopleWithoutActiveProfile,
      without_qiwe_platform_identity: asCount(
        values.linked_people_without_qiwe_platform_identity
      ),
      without_answer_context_canary_spec: asCount(
        values.linked_people_without_answer_context_canary_spec
      ),
    },
    repair_gaps: {
      linked_aliases_missing: asCount(values.linked_aliases_missing),
      linked_messages_missing_sender_person: asCount(
        values.linked_messages_missing_sender_person
      ),
      qiwe_platform_identities_missing: asCount(
        values.qiwe_platform_identities_missing
      ),
      qiwe_platform_identity_ambiguous_users: asCount(
        values.qiwe_platform_identity_ambiguous_users
      ),
      running_people_profile_missing_running_hint: asCount(
        values.running_people_profile_missing_running_hint
      ),
    },
    answer_context_canary_specs: {
      mentioned_records: asCount(values.answer_context_canary_specs_total),
      mentioned_people: answerContextPeople,
      speaker_records: asCount(values.answer_context_speaker_canary_specs_total),
      speaker_people: speakerPeople,
      referenced_records: asCount(values.answer_context_referenced_canary_specs_total),
      referenced_people: referencedPeople,
    },
    readiness: {
      all_safe_current_room_identities_linked: countsEqual(
        values.qiwe_room_channel_identities_linked,
        values.qiwe_room_channel_identities_total
      ),
      all_current_room_potential_members_linked: countIsZero(
        values.qiwe_room_potential_member_identities_unlinked
      ),
      all_linked_people_have_active_profiles:
        linkedPeopleTotal !== null &&
        linkedPeopleWithActiveProfile !== null &&
        linkedPeopleWithActiveProfile === linkedPeopleTotal &&
        linkedPeopleWithoutActiveProfile === 0,
      all_linked_people_have_qiwe_platform_identity: countIsZero(
        values.linked_people_without_qiwe_platform_identity
      ),
      all_linked_people_have_canary_names: countIsZero(
        values.linked_people_without_answer_context_canary_spec
      ),
      mentioned_speaker_referenced_canaries_cover_linked_people:
        linkedPeopleTotal !== null &&
        answerContextPeople === linkedPeopleTotal &&
        speakerPeople === linkedPeopleTotal &&
        referencedPeople === linkedPeopleTotal,
      running_profile_hints_cover_running_people: countsEqual(
        values.running_people_with_profile_running_hint,
        values.linked_people_with_running_facts
      ),
    },
    retained_evidence_boundary: {
      sanitized_summary_only: true,
      includes_chat_id: false,
      includes_sender_id: false,
      includes_channel_user_id: false,
      includes_person_id: false,
      includes_raw_messages: false,
      includes_profile_text: false,
      includes_database_url: false,
      includes_tokens: false,
    },
  };
}

function asCount(value) {
  return Number.isInteger(value) && value >= 0 ? value : null;
}

function countIsZero(value) {
  return asCount(value) === 0;
}

function countsEqual(left, right) {
  const leftCount = asCount(left);
  const rightCount = asCount(right);
  return leftCount !== null && rightCount !== null && leftCount === rightCount;
}

function nonNegativeDifference(left, right) {
  const leftCount = asCount(left);
  const rightCount = asCount(right);
  if (leftCount === null || rightCount === null) {
    return null;
  }
  return Math.max(0, leftCount - rightCount);
}

function parseCoverage(text) {
  const trimmed = text.trim();
  if (trimmed.startsWith("{")) {
    return parseJson(trimmed, "coverage JSON");
  }

  const lines = text.split(/\r?\n/);
  for (const [index, line] of lines.entries()) {
    if (!line.trimStart().startsWith("{")) {
      continue;
    }
    const candidate = lines.slice(index).join("\n").trim();
    try {
      return JSON.parse(candidate);
    } catch {
      continue;
    }
  }

  const prefixes = [
    "erhua_member_recognition_coverage=",
    "identity_bootstrap_persons=",
  ];
  const records = [];
  for (const [index, line] of text.split(/\r?\n/).entries()) {
    const trimmedLine = line.trim();
    if (trimmedLine === "") {
      continue;
    }
    const prefix = prefixes.find((candidate) => trimmedLine.startsWith(candidate));
    if (!prefix) {
      continue;
    }
    records.push(
      parseJson(trimmedLine.slice(prefix.length), `coverage line ${index + 1}`)
    );
  }

  if (records.length !== 1) {
    fail("expected exactly one coverage JSON object or one prefixed coverage record");
  }
  return records[0];
}

function readNonNegativeInteger(record, field, errorSink) {
  const value = record[field];
  if (!Number.isInteger(value) || value < 0) {
    errorSink.push(`${field} must be a non-negative integer`);
    return undefined;
  }
  return value;
}

function readCanarySpecs(record, field, expectedType, expectedCount, errorSink) {
  const specs = record[field];
  if (!Array.isArray(specs)) {
    errorSink.push(`${field} must be an array`);
    return [];
  }
  if (expectedCount !== undefined && specs.length !== expectedCount) {
    errorSink.push(
      `${field} length must match ${field}_total: expected ${expectedCount}, got ${specs.length}`
    );
  }
  const seen = new Set();
  const normalized = [];
  for (const [index, spec] of specs.entries()) {
    const label = `${field}[${index}]`;
    if (!spec || typeof spec !== "object" || Array.isArray(spec)) {
      errorSink.push(`${label} must be an object`);
      continue;
    }
    const canaryType = textField(spec, ["canary_type", "canaryType"]) || expectedType;
    if (canaryType !== expectedType) {
      errorSink.push(`${label} canary_type must be ${expectedType}`);
    }
    const expectedLabel =
      expectedType === "speaker_self"
        ? textField(spec, ["expected_speaker_label"])
        : expectedType === "referenced_member"
          ? textField(spec, ["expected_referenced_label"])
          : textField(spec, ["expected_mention", "name"]);
    if (!expectedLabel) {
      errorSink.push(
        `${label} must include ${
          expectedType === "speaker_self"
            ? "expected_speaker_label"
            : expectedType === "referenced_member"
              ? "expected_referenced_label"
              : "expected_mention"
        }`
      );
    }
    const canonicalKey = textField(spec, ["canonical_key", "same_person_key"]);
    if (!canonicalKey) {
      errorSink.push(`${label} must include canonical_key`);
    }
    const specKey = `${expectedType}\0${expectedLabel}\0${canonicalKey}`;
    if (seen.has(specKey)) {
      errorSink.push(`${label} duplicates another canary spec`);
    } else {
      seen.add(specKey);
    }
    if (
      spec.required_profile_terms !== undefined &&
      !Array.isArray(spec.required_profile_terms)
    ) {
      errorSink.push(`${label} required_profile_terms must be an array when present`);
    }
    normalized.push({ canonicalKey });
  }
  return normalized;
}

function uniqueCanonicalKeyCount(specs) {
  return new Set(specs.map((spec) => spec.canonicalKey).filter(Boolean)).size;
}

function canonicalKeySetsEqual(left, right) {
  const leftKeys = new Set(left.map((spec) => spec.canonicalKey).filter(Boolean));
  const rightKeys = new Set(right.map((spec) => spec.canonicalKey).filter(Boolean));
  if (leftKeys.size !== rightKeys.size) {
    return false;
  }
  for (const key of leftKeys) {
    if (!rightKeys.has(key)) {
      return false;
    }
  }
  return true;
}

function sampleSuffix(record, fieldNames) {
  for (const fieldName of fieldNames) {
    const samples = record[fieldName];
    if (Array.isArray(samples) && samples.length > 0) {
      return `; samples: ${samples.slice(0, 5).map(formatSample).join("; ")}`;
    }
  }
  return "";
}

function formatSample(sample) {
  if (typeof sample === "string") {
    return sample;
  }
  if (!sample || typeof sample !== "object" || Array.isArray(sample)) {
    return JSON.stringify(sample);
  }
  const display = textField(sample, [
    "display_name",
    "alias",
    "sender_name",
    "name",
    "mention_text",
  ]);
  const personId = textField(sample, ["person_id"]);
  const identityKey = textField(sample, ["identity_key"]);
  const personKey = textField(sample, ["person_key"]);
  const reason = textField(sample, ["reason", "status"]);
  return [display, identityKey, personKey, personId, reason]
    .filter(Boolean)
    .join(" / ");
}

function textField(record, fields) {
  for (const field of fields) {
    const value = record[field];
    if (typeof value === "string" && value.trim() !== "") {
      return value.trim();
    }
  }
  return "";
}

function ratio(numerator, denominator) {
  if (!denominator) {
    return "0.00%";
  }
  return `${((numerator / denominator) * 100).toFixed(2)}%`;
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
