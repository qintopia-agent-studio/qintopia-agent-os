#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const args = parseArgs(process.argv.slice(2));
if (!args.coverage || !args.canary || !args.completionSummary) {
  fail(
    [
      "usage: node tools/deploy/build-erhua-member-recognition-roster-audit.mjs",
      "--coverage <identity-bootstrap-dry-run-output.json>",
      "--canary <answer-context-canary-output.jsonl>",
      "--completion-summary <sanitized-completion-summary.json>",
      "[--output <sanitized-roster-audit.json>]",
      "[--require-active-profiles]",
    ].join(" ")
  );
}

const coverageText = readEvidence(args.coverage, "coverage evidence", {
  sanitizeCoverageSamples: true,
});
const canaryText = readEvidence(args.canary, "canary evidence");
const completionSummaryText = readEvidence(
  args.completionSummary,
  "completion summary evidence"
);

const coverage = parseCoverage(coverageText);
const canaries = parseCanaryRecords(canaryText);
const completionSummary = parseJson(
  completionSummaryText.trim(),
  "completion summary JSON"
);

const errors = [];
const warnings = [];
const scopeFingerprint = readScopeFingerprint(
  coverage,
  "coverage scope_fingerprint",
  errors
);
const linkedPeopleTotal = readNonNegativeInteger(
  coverage,
  "linked_people_total",
  errors
);
const mentionedSpecsTotal = readNonNegativeInteger(
  coverage,
  "answer_context_canary_specs_total",
  errors
);
const mentionedPeopleTotal = readNonNegativeInteger(
  coverage,
  "answer_context_canary_people_total",
  errors
);
const speakerSpecsTotal = readNonNegativeInteger(
  coverage,
  "answer_context_speaker_canary_specs_total",
  errors
);
const speakerPeopleTotal = readNonNegativeInteger(
  coverage,
  "answer_context_speaker_canary_people_total",
  errors
);
const referencedSpecsTotal = readNonNegativeInteger(
  coverage,
  "answer_context_referenced_canary_specs_total",
  errors
);
const referencedPeopleTotal = readNonNegativeInteger(
  coverage,
  "answer_context_referenced_canary_people_total",
  errors
);
const linkedPeopleWithoutCanarySpec = readNonNegativeInteger(
  coverage,
  "linked_people_without_answer_context_canary_spec",
  errors
);

const mentionedSpecs = readCanarySpecs(
  coverage,
  "answer_context_canary_specs",
  "mentioned_member",
  mentionedSpecsTotal,
  errors
);
const speakerSpecs = readCanarySpecs(
  coverage,
  "answer_context_speaker_canary_specs",
  "speaker_self",
  speakerSpecsTotal,
  errors
);
const referencedSpecs = readCanarySpecs(
  coverage,
  "answer_context_referenced_canary_specs",
  "referenced_member",
  referencedSpecsTotal,
  errors
);

if (!canonicalKeySetsEqual(mentionedSpecs, speakerSpecs)) {
  errors.push(
    "coverage mentioned-member and speaker self-canary specs must cover the same canonical people"
  );
}
if (!canonicalKeySetsEqual(mentionedSpecs, referencedSpecs)) {
  errors.push(
    "coverage mentioned-member and referenced-member canary specs must cover the same canonical people"
  );
}

checkCompletionSummary({
  completionSummary,
  scopeFingerprint,
  linkedPeopleTotal,
  mentionedSpecsTotal,
  mentionedPeopleTotal,
  speakerSpecsTotal,
  speakerPeopleTotal,
  referencedSpecsTotal,
  referencedPeopleTotal,
  errors,
});

const roster = buildRoster({
  mentionedSpecs,
  speakerSpecs,
  referencedSpecs,
  canaries,
  errors,
  warnings,
});
const people = [...roster.people.values()].map(finalizePerson).sort(comparePeople);
const gaps = buildGaps({
  people,
  linkedPeopleTotal,
  mentionedPeopleTotal,
  speakerPeopleTotal,
  referencedPeopleTotal,
  linkedPeopleWithoutCanarySpec,
  requireActiveProfiles: args.requireActiveProfiles,
});
for (const gap of gaps.filter((item) => item.severity === "error")) {
  errors.push(formatGap(gap));
}
for (const gap of gaps.filter((item) => item.severity === "warning")) {
  warnings.push(formatGap(gap));
}

const audit = {
  schema_version: "erhua_member_recognition_roster_audit_v1",
  passed: errors.length === 0,
  strict_profile_required: args.requireActiveProfiles,
  scope_fingerprint: scopeFingerprint,
  linked_people_total: linkedPeopleTotal,
  audited_people_total: people.length,
  canary_totals: {
    mentioned_specs: mentionedSpecsTotal,
    mentioned_people: mentionedPeopleTotal,
    speaker_specs: speakerSpecsTotal,
    speaker_people: speakerPeopleTotal,
    referenced_specs: referencedSpecsTotal,
    referenced_people: referencedPeopleTotal,
    mentioned_records: roster.mentionedRecords,
    speaker_records: roster.speakerRecords,
    referenced_records: roster.referencedRecords,
  },
  profile_totals: profileTotals(people),
  people,
  gaps,
  retained_evidence_boundary: {
    sanitized_roster_only: true,
    includes_chat_id: false,
    includes_sender_id: false,
    includes_channel_user_id: false,
    includes_person_id: false,
    includes_raw_messages: false,
    includes_raw_profile_text: false,
    includes_hidden_profile_details: false,
    includes_database_url: false,
    includes_tokens: false,
  },
};

const output = `${JSON.stringify(audit, null, 2)}\n`;
assertNoForbiddenOutput(output);
if (args.output) {
  fs.writeFileSync(path.resolve(args.output), output, "utf8");
} else {
  process.stdout.write(output);
}

if (errors.length > 0) {
  console.error(
    `Erhua member recognition roster audit failed: ${people.length}/${linkedPeopleTotal ?? 0} linked people audited.`
  );
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  for (const warning of warnings) {
    console.error(`- warning: ${warning}`);
  }
  process.exit(1);
}

console.log(
  `Erhua member recognition roster audit passed: ${people.length}/${linkedPeopleTotal} linked people audited, ${roster.mentionedRecords} mentioned records, ${roster.speakerRecords} speaker records, ${roster.referencedRecords} referenced records.`
);
for (const warning of warnings) {
  console.log(`warning: ${warning}`);
}

function buildRoster({
  mentionedSpecs,
  speakerSpecs,
  referencedSpecs,
  canaries,
  errors,
  warnings,
}) {
  const people = new Map();
  for (const spec of mentionedSpecs) {
    const person = ensurePerson(people, spec.canonicalKey);
    person.mentionedLabels.add(spec.label);
    person.safeNames.add(spec.label);
    addRequiredProfileTerms(person, spec.requiredProfileTerms);
  }
  for (const spec of speakerSpecs) {
    const person = ensurePerson(people, spec.canonicalKey);
    person.speakerLabels.add(spec.label);
    person.safeNames.add(spec.label);
    addRequiredProfileTerms(person, spec.requiredProfileTerms);
  }
  for (const spec of referencedSpecs) {
    const person = ensurePerson(people, spec.canonicalKey);
    person.referencedLabels.add(spec.label);
    person.safeNames.add(spec.label);
    addRequiredProfileTerms(person, spec.requiredProfileTerms);
  }

  let mentionedRecords = 0;
  let speakerRecords = 0;
  let referencedRecords = 0;
  const expectedSpecs = new Set(
    [...mentionedSpecs, ...speakerSpecs, ...referencedSpecs].map(canarySpecKey)
  );
  const seenSpecs = new Set();

  for (const [index, record] of canaries.entries()) {
    const canaryType = canaryTypeOf(record, errors);
    const canonicalKey = textField(record, ["canonical_key", "same_person_key"]);
    if (!canonicalKey) {
      errors.push(`canary ${index + 1} must include canonical_key`);
      continue;
    }
    const label = canaryLabel(record, canaryType);
    if (!label) {
      errors.push(`canary ${index + 1} is missing its expected safe label`);
      continue;
    }
    const specKey = canarySpecKey({
      canaryType,
      label,
      canonicalKey,
    });
    if (!expectedSpecs.has(specKey)) {
      errors.push(
        `${canaryType} ${JSON.stringify(label)} (${canonicalKey}) was not generated by coverage canary specs`
      );
    }
    if (seenSpecs.has(specKey)) {
      errors.push(
        `${canaryType} ${JSON.stringify(label)} (${canonicalKey}) appears more than once`
      );
    } else {
      seenSpecs.add(specKey);
    }

    const person = ensurePerson(people, canonicalKey);
    person.safeNames.add(label);
    const answerContext = answerContextFromRecord(record);
    if (!answerContext || answerContext.success !== true) {
      errors.push(`canary ${index + 1}: answer_context is missing or unsuccessful`);
      continue;
    }

    if (canaryType === "speaker_self") {
      speakerRecords += 1;
      person.speakerLabels.add(label);
      const speaker = answerContext.speaker;
      const target = validateSpeakerTarget(speaker, label, "speaker", errors);
      if (!target) {
        continue;
      }
      person.speakerResolved = true;
      applyResolvedTarget(person, record, target, label, errors);
    } else if (canaryType === "referenced_member") {
      referencedRecords += 1;
      person.referencedLabels.add(label);
      const referencedMember = answerContext.referenced_member;
      const target = validateSpeakerTarget(
        referencedMember,
        label,
        "referenced_member",
        errors
      );
      if (!target) {
        continue;
      }
      person.referencedResolved = true;
      applyResolvedTarget(person, record, target, label, errors);
    } else {
      mentionedRecords += 1;
      person.mentionedLabels.add(label);
      const members = Array.isArray(answerContext.mentioned_members)
        ? answerContext.mentioned_members
        : [];
      const member = members.find(
        (item) => item && typeof item === "object" && item.mention_text === label
      );
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
      person.mentionedResolved = true;
      applyResolvedTarget(person, record, member, label, errors);
    }
  }

  for (const spec of expectedSpecs) {
    if (!seenSpecs.has(spec)) {
      warnings.push(`missing canary evidence for ${spec.replaceAll("\0", " ")}`);
    }
  }

  return {
    people,
    mentionedRecords,
    speakerRecords,
    referencedRecords,
  };
}

function validateSpeakerTarget(target, label, fieldLabel, errors) {
  if (!target || typeof target !== "object") {
    errors.push(`${label}: ${fieldLabel} was not returned`);
    return null;
  }
  if (target.resolved !== true) {
    errors.push(`${label}: ${fieldLabel} did not resolve`);
    return null;
  }
  if (
    target.resolution_scope !== "exact_chat" &&
    target.resolution_scope !== "qiwe_platform_user"
  ) {
    errors.push(
      `${label}: ${fieldLabel} resolution_scope must be exact_chat or qiwe_platform_user`
    );
    return null;
  }
  return target;
}

function applyResolvedTarget(person, record, target, label, errors) {
  const personRef = textField(target, ["person_ref"]);
  if (!isPersonRef(personRef)) {
    errors.push(`${label}: resolved target is missing a valid person_ref`);
    return;
  }
  if (person.personRef && person.personRef !== personRef) {
    errors.push(
      `${label}: canonical key ${person.canonicalKey} resolved to multiple people`
    );
    return;
  }
  person.personRef = personRef;
  for (const value of [
    textField(target, ["display_name"]),
    textField(target, ["mention_text"]),
  ]) {
    if (value) {
      person.safeNames.add(value);
    }
  }

  const safeSummary = textField(target, ["safe_summary"]);
  if (safeSummary) {
    person.safeSummarySeen = true;
  } else {
    person.missingSafeSummary = true;
  }

  const hints =
    target.safe_reply_hints &&
    typeof target.safe_reply_hints === "object" &&
    !Array.isArray(target.safe_reply_hints)
      ? target.safe_reply_hints
      : null;
  if (!hints) {
    person.missingSafeReplyHints = true;
    return;
  }
  const profileStatus = textField(hints, ["profile_status"]);
  if (profileStatus) {
    person.profileStatuses.add(profileStatus);
  }
  if (profileStatus === "identity_only") {
    person.identityOnly = true;
    if (hints.do_not_infer_missing_profile !== true) {
      errors.push(
        `${label}: identity-only member must set do_not_infer_missing_profile=true`
      );
    }
  }
  if (hasSafeProfileHint(hints)) {
    person.hasSafeProfileHints = true;
    for (const field of [
      "topics",
      "stable_profile_notes",
      "temporary_communication_notes",
    ]) {
      const value = hints[field];
      if (
        Array.isArray(value) &&
        value.some((item) => typeof item === "string" && item.trim() !== "")
      ) {
        person.safeProfileHintFields.add(field);
      }
    }
  }

  const searchableProfile = `${safeSummary}\n${JSON.stringify(hints)}`;
  for (const term of normalizeProfileTerms(record.required_profile_terms)) {
    if (searchableProfile.includes(term)) {
      person.requiredProfileTermsMatched.add(term);
    } else {
      person.requiredProfileTermsMissing.add(term);
    }
  }
}

function ensurePerson(people, canonicalKey) {
  const key = canonicalKey || "missing-canonical-key";
  let person = people.get(key);
  if (!person) {
    person = {
      canonicalKey: key,
      personRef: "",
      safeNames: new Set(),
      mentionedLabels: new Set(),
      speakerLabels: new Set(),
      referencedLabels: new Set(),
      requiredProfileTerms: new Set(),
      requiredProfileTermsMatched: new Set(),
      requiredProfileTermsMissing: new Set(),
      profileStatuses: new Set(),
      safeProfileHintFields: new Set(),
      safeSummarySeen: false,
      hasSafeProfileHints: false,
      identityOnly: false,
      mentionedResolved: false,
      speakerResolved: false,
      referencedResolved: false,
      missingSafeSummary: false,
      missingSafeReplyHints: false,
    };
    people.set(key, person);
  }
  return person;
}

function addRequiredProfileTerms(person, terms) {
  for (const term of terms) {
    person.requiredProfileTerms.add(term);
  }
}

function finalizePerson(person) {
  const requiredTerms = sorted(person.requiredProfileTerms);
  const matchedTerms = requiredTerms.filter((term) =>
    person.requiredProfileTermsMatched.has(term)
  );
  const missingTerms = requiredTerms.filter(
    (term) => !person.requiredProfileTermsMatched.has(term)
  );
  const profileStatus = profileStatusOf(person);
  return {
    canonical_key: person.canonicalKey,
    person_ref: person.personRef || null,
    safe_names: sorted(person.safeNames),
    mentioned_labels: sorted(person.mentionedLabels),
    speaker_labels: sorted(person.speakerLabels),
    referenced_labels: sorted(person.referencedLabels),
    profile_status: profileStatus,
    has_safe_profile_hints: person.hasSafeProfileHints,
    safe_profile_hint_fields: sorted(person.safeProfileHintFields),
    required_profile_terms: requiredTerms,
    required_profile_terms_matched: matchedTerms,
    required_profile_terms_missing: missingTerms,
    mentioned_resolved: person.mentionedResolved,
    speaker_resolved: person.speakerResolved,
    referenced_resolved: person.referencedResolved,
    safe_summary_present: person.safeSummarySeen,
  };
}

function profileStatusOf(person) {
  if (person.identityOnly || person.profileStatuses.has("identity_only")) {
    return "identity_only";
  }
  if (person.profileStatuses.has("no_stable_profile")) {
    return "no_stable_profile";
  }
  if (person.hasSafeProfileHints) {
    return "stable_profile";
  }
  return "missing_profile_hints";
}

function buildGaps({
  people,
  linkedPeopleTotal,
  mentionedPeopleTotal,
  speakerPeopleTotal,
  referencedPeopleTotal,
  linkedPeopleWithoutCanarySpec,
  requireActiveProfiles,
}) {
  const gaps = [];
  if (people.length !== linkedPeopleTotal) {
    gaps.push({
      severity: "error",
      issue: "audited_people_total_mismatch",
      expected: linkedPeopleTotal,
      actual: people.length,
    });
  }
  if (mentionedPeopleTotal !== linkedPeopleTotal) {
    gaps.push({
      severity: "error",
      issue: "mentioned_canary_people_mismatch",
      expected: linkedPeopleTotal,
      actual: mentionedPeopleTotal,
    });
  }
  if (speakerPeopleTotal !== linkedPeopleTotal) {
    gaps.push({
      severity: "error",
      issue: "speaker_canary_people_mismatch",
      expected: linkedPeopleTotal,
      actual: speakerPeopleTotal,
    });
  }
  if (referencedPeopleTotal !== linkedPeopleTotal) {
    gaps.push({
      severity: "error",
      issue: "referenced_canary_people_mismatch",
      expected: linkedPeopleTotal,
      actual: referencedPeopleTotal,
    });
  }
  if (linkedPeopleWithoutCanarySpec !== 0) {
    gaps.push({
      severity: "error",
      issue: "linked_people_without_canary_spec",
      actual: linkedPeopleWithoutCanarySpec,
    });
  }

  for (const person of people) {
    const base = {
      canonical_key: person.canonical_key,
      person_ref: person.person_ref,
    };
    if (!person.person_ref) {
      gaps.push({ ...base, severity: "error", issue: "missing_person_ref" });
    }
    if (person.safe_names.length === 0) {
      gaps.push({ ...base, severity: "error", issue: "missing_safe_name" });
    }
    if (!person.mentioned_resolved) {
      gaps.push({ ...base, severity: "error", issue: "missing_mentioned_canary" });
    }
    if (!person.speaker_resolved) {
      gaps.push({ ...base, severity: "error", issue: "missing_speaker_canary" });
    }
    if (!person.referenced_resolved) {
      gaps.push({ ...base, severity: "error", issue: "missing_referenced_canary" });
    }
    if (!person.safe_summary_present) {
      gaps.push({ ...base, severity: "error", issue: "missing_safe_summary" });
    }
    if (person.required_profile_terms_missing.length > 0) {
      gaps.push({
        ...base,
        severity: "error",
        issue: "missing_required_profile_terms",
        required_profile_terms_missing: person.required_profile_terms_missing,
      });
    }
    if (!person.has_safe_profile_hints) {
      gaps.push({
        ...base,
        severity: requireActiveProfiles ? "error" : "warning",
        issue: "missing_safe_profile_hints",
      });
    }
    if (person.profile_status === "identity_only") {
      gaps.push({
        ...base,
        severity: requireActiveProfiles ? "error" : "warning",
        issue: "identity_only_profile",
      });
    }
  }
  return gaps;
}

function profileTotals(people) {
  const totals = {
    stable_profile: 0,
    no_stable_profile: 0,
    identity_only: 0,
    missing_profile_hints: 0,
    with_safe_profile_hints: 0,
  };
  for (const person of people) {
    totals[person.profile_status] += 1;
    if (person.has_safe_profile_hints) {
      totals.with_safe_profile_hints += 1;
    }
  }
  return totals;
}

function checkCompletionSummary({
  completionSummary,
  scopeFingerprint,
  linkedPeopleTotal,
  mentionedSpecsTotal,
  mentionedPeopleTotal,
  speakerSpecsTotal,
  speakerPeopleTotal,
  referencedSpecsTotal,
  referencedPeopleTotal,
  errors,
}) {
  if (
    !completionSummary ||
    typeof completionSummary !== "object" ||
    Array.isArray(completionSummary)
  ) {
    errors.push("completion summary must be a JSON object");
    return;
  }
  if (completionSummary.schema_version !== "erhua_member_recognition_completion_v1") {
    errors.push(
      "completion summary schema_version must be erhua_member_recognition_completion_v1"
    );
  }
  if (completionSummary.passed !== true) {
    errors.push("completion summary must be passed=true");
  }
  if (
    scopeFingerprint &&
    completionSummary.scope_fingerprint &&
    completionSummary.scope_fingerprint !== scopeFingerprint
  ) {
    errors.push("completion summary scope_fingerprint must match coverage");
  }
  requireSummaryCount(
    completionSummary.linked_people?.total,
    linkedPeopleTotal,
    "completion linked_people.total",
    errors
  );
  requireSummaryCount(
    completionSummary.answer_context_canaries?.mentioned_records,
    mentionedSpecsTotal,
    "completion mentioned canary records",
    errors
  );
  requireSummaryCount(
    completionSummary.answer_context_canaries?.mentioned_people_resolved,
    mentionedPeopleTotal,
    "completion mentioned people resolved",
    errors
  );
  requireSummaryCount(
    completionSummary.answer_context_canaries?.speaker_records,
    speakerSpecsTotal,
    "completion speaker canary records",
    errors
  );
  requireSummaryCount(
    completionSummary.answer_context_canaries?.speaker_people_resolved,
    speakerPeopleTotal,
    "completion speaker people resolved",
    errors
  );
  requireSummaryCount(
    completionSummary.answer_context_canaries?.referenced_records,
    referencedSpecsTotal,
    "completion referenced canary records",
    errors
  );
  requireSummaryCount(
    completionSummary.answer_context_canaries?.referenced_people_resolved,
    referencedPeopleTotal,
    "completion referenced people resolved",
    errors
  );
  requireSummaryCount(
    completionSummary.answer_context_canaries?.linked_people_resolved,
    linkedPeopleTotal,
    "completion linked people resolved",
    errors
  );
  if (
    completionSummary.current_room_qiwe_identities?.unsafe_display_unlinked !==
    undefined
  ) {
    requireSummaryCount(
      completionSummary.current_room_qiwe_identities.unsafe_display_unlinked,
      0,
      "completion unsafe display unlinked",
      errors
    );
  }
}

function requireSummaryCount(actual, expected, label, errors) {
  if (actual !== expected) {
    errors.push(`${label}: expected ${expected}, got ${actual}`);
  }
}

function readEvidence(file, label, options = {}) {
  let text = fs.readFileSync(path.resolve(file), "utf8");
  if (options.sanitizeCoverageSamples) {
    text = sanitizeCoverageGapSamples(text, label);
  }
  for (const pattern of forbiddenInputPatterns()) {
    if (pattern.test(text)) {
      fail(`${label} contains forbidden sensitive fragment: ${pattern}`);
    }
  }
  return text;
}

function sanitizeCoverageGapSamples(text, label) {
  let changed = false;
  const sanitized = text.replace(
    /("(?:linked_people_with_active_profile_samples|linked_people_without_active_profile_samples|linked_messages_missing_sender_person_samples|messages_missing_sender_person_samples|running_people_profile_missing_running_hint_samples|running_profile_missing_samples)"\s*:\s*\[)([\s\S]*?)(\n\s*\])/g,
    (match, prefix, body, suffix) => {
      if (!body.includes('"person_id"')) {
        return match;
      }
      changed = true;
      const sanitizedBody = body.replace(
        /^[ \t]*"person_id"\s*:\s*"(?:\\.|[^"\\])*"(?:[ \t]*,[ \t]*\r?\n[ \t]*|[ \t]*,?[ \t]*(?=\r?\n))/gm,
        ""
      );
      return `${prefix}${sanitizedBody}${suffix}`;
    }
  );
  if (!changed) {
    return text;
  }
  parseJson(sanitized, `${label} sanitized coverage JSON`);
  return sanitized;
}

function forbiddenInputPatterns() {
  return [
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
    /"target_chat_ids"\s*:/,
    /"raw_messages"\s*:/,
    /"candidate_facts"\s*:/,
    /"source_message_id"\s*:/,
    /"hidden_profile_details"\s*:/,
    /"raw"\s*:/,
    /[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}/i,
    /1[3-9]\d{9}/,
  ];
}

function assertNoForbiddenOutput(text) {
  for (const pattern of [
    ...forbiddenInputPatterns(),
    /"includes_person_id"\s*:\s*true/,
    /"includes_raw_profile_text"\s*:\s*true/,
  ]) {
    if (pattern.test(text)) {
      fail(`roster audit output contains forbidden sensitive fragment: ${pattern}`);
    }
  }
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
    if (!trimmedLine.startsWith("erhua_member_recognition_canary=")) {
      continue;
    }
    records.push(
      parseJson(
        trimmedLine.slice("erhua_member_recognition_canary=".length),
        `canary line ${index + 1}`
      )
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

function readCanarySpecs(record, field, expectedType, expectedCount, errors) {
  const rawSpecs = record[field];
  if (!Array.isArray(rawSpecs)) {
    if (expectedCount === 0) {
      return [];
    }
    errors.push(`${field} must be an array when coverage canary count is non-zero`);
    return [];
  }
  if (expectedCount !== undefined && rawSpecs.length !== expectedCount) {
    errors.push(
      `${field} length must match ${field}_total: expected ${expectedCount}, got ${rawSpecs.length}`
    );
  }
  return rawSpecs.map((spec, index) =>
    normalizeCanarySpec(spec, expectedType, `${field}[${index}]`, errors)
  );
}

function normalizeCanarySpec(spec, expectedType, label, errors) {
  if (!spec || typeof spec !== "object" || Array.isArray(spec)) {
    errors.push(`${label} must be an object`);
    return {
      canaryType: expectedType,
      label: "",
      canonicalKey: "",
      requiredProfileTerms: [],
    };
  }
  const canaryType = textField(spec, ["canary_type", "canaryType"]) || expectedType;
  if (canaryType !== expectedType) {
    errors.push(`${label} canary_type must be ${expectedType}`);
  }
  const expectedLabel =
    expectedType === "speaker_self"
      ? textField(spec, ["expected_speaker_label"])
      : expectedType === "referenced_member"
        ? textField(spec, ["expected_referenced_label"])
        : textField(spec, ["expected_mention", "name"]);
  if (!expectedLabel) {
    errors.push(`${label} must include an expected safe label`);
  }
  const canonicalKey = textField(spec, ["canonical_key", "same_person_key"]);
  if (!canonicalKey) {
    errors.push(`${label} must include canonical_key`);
  }
  return {
    canaryType: expectedType,
    label: expectedLabel,
    canonicalKey,
    requiredProfileTerms: normalizeProfileTerms(spec.required_profile_terms),
  };
}

function canaryTypeOf(record, errors) {
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

function canaryLabel(record, canaryType) {
  if (canaryType === "speaker_self") {
    return textField(record, ["expected_speaker_label"]);
  }
  if (canaryType === "referenced_member") {
    return textField(record, ["expected_referenced_label"]);
  }
  return textField(record, ["expected_mention", "name"]);
}

function canarySpecKey(spec) {
  return `${spec.canaryType}\0${spec.label}\0${spec.canonicalKey}`;
}

function canonicalKeySetsEqual(left, right) {
  const leftKeys = new Set(left.map((spec) => spec.canonicalKey).filter(Boolean));
  const rightKeys = new Set(right.map((spec) => spec.canonicalKey).filter(Boolean));
  return setsEqual(leftKeys, rightKeys);
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

function readNonNegativeInteger(record, field, errors) {
  const value = record?.[field];
  if (!Number.isInteger(value) || value < 0) {
    errors.push(`${field} must be a non-negative integer`);
    return undefined;
  }
  return value;
}

function readScopeFingerprint(record, label, errors) {
  const value = record?.scope_fingerprint;
  if (typeof value !== "string" || !/^sha256:[0-9a-f]{64}$/.test(value.trim())) {
    errors.push(`${label} must be a canonical sha256 marker`);
    return undefined;
  }
  return value.trim();
}

function textField(record, fields) {
  for (const field of fields) {
    const value = record?.[field];
    if (typeof value === "string" && value.trim()) {
      return value.trim();
    }
  }
  return "";
}

function asText(value) {
  return typeof value === "string" ? value.trim() : "";
}

function isPersonRef(value) {
  return typeof value === "string" && /^sha256:[0-9a-f]{64}$/.test(value.trim());
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

function normalizeProfileTerms(value) {
  if (!Array.isArray(value)) {
    return [];
  }
  return [...new Set(value.map(asText).filter(Boolean))].sort();
}

function sorted(set) {
  return [...set].sort((left, right) => left.localeCompare(right));
}

function comparePeople(left, right) {
  return left.canonical_key.localeCompare(right.canonical_key);
}

function formatGap(gap) {
  const person = gap.canonical_key ? ` ${gap.canonical_key}` : "";
  return `${gap.issue}${person}`;
}

function parseArgs(argv) {
  const parsed = { requireActiveProfiles: false };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--coverage") {
      parsed.coverage = argv[++index];
    } else if (arg === "--canary") {
      parsed.canary = argv[++index];
    } else if (arg === "--completion-summary") {
      parsed.completionSummary = argv[++index];
    } else if (arg === "--output") {
      parsed.output = argv[++index];
    } else if (arg === "--require-active-profiles") {
      parsed.requireActiveProfiles = true;
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
