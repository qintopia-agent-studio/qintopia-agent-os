#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const args = parseArgs(process.argv.slice(2));
if (!args.roomSync || !args.profile || !args.coverage || !args.canary) {
  fail(
    [
      "usage: node tools/deploy/check-erhua-member-recognition-completion.mjs",
      "--room-sync <identity-backfill-room-member-sync-apply-output.json>",
      "--profile <member-profile-quiet-apply-output.json>",
      "--coverage <identity-bootstrap-dry-run-output.json>",
      "--canary <answer-context-canary-output.jsonl-with-mentioned-speaker-and-referenced-records>",
      "[--summary-output <sanitized-completion-summary.json>]",
      "[--require-active-profiles]",
    ].join(" ")
  );
}

const roomSyncText = readEvidence(args.roomSync, "room member sync evidence");
const profileText = readEvidence(args.profile, "member profile evidence");
const coverageText = readEvidence(args.coverage, "coverage evidence");
const canaryText = readEvidence(args.canary, "canary evidence");
const roomSync = parseRoomSyncEvidence(roomSyncText);
const profile = parseProfileEvidence(profileText);
const coverage = parseCoverage(coverageText);
const canaries = parseCanaryRecords(canaryText);
assertNoVisiblePhoneLike(roomSync, "room member sync evidence");
assertNoVisiblePhoneLike(profile, "member profile evidence");
assertNoVisiblePhoneLike(coverage, "coverage evidence");
assertNoVisiblePhoneLike(canaries, "canary evidence");
const MIN_PROFILE_REPAIR_MESSAGE_LIMIT = 5000;

const errors = [];
const roomSyncStats = checkRoomSync(roomSync, errors);
const profileStats = checkProfile(profile, errors);
const coverageScopeFingerprint = readScopeFingerprint(
  coverage,
  "coverage scope_fingerprint",
  errors
);
const values = {};
for (const field of [
  "qiwe_channel_identities_raw_total",
  "qiwe_room_channel_identities_raw_total",
  "qiwe_room_channel_identities_total",
  "qiwe_room_channel_identities_linked",
  "qiwe_room_channel_identities_excluded",
  "qiwe_room_potential_member_identities_total",
  "qiwe_room_potential_member_identities_linked",
  "qiwe_room_potential_member_identities_unlinked",
  "qiwe_channel_identities_total",
  "qiwe_channel_identities_linked",
  "qiwe_channel_identities_excluded",
  "total_channel_identities",
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
]) {
  values[field] = readNonNegativeInteger(coverage, field, errors);
}
if (
  profileStats.currentRoomLinkedPeople !== undefined &&
  values.linked_people_total !== undefined &&
  profileStats.currentRoomLinkedPeople !== values.linked_people_total
) {
  errors.push(
    "member profile current-room linked people must match coverage linked_people_total"
  );
}
const coverageMentionCanarySpecs = readCoverageCanarySpecs(
  coverage,
  "answer_context_canary_specs",
  "mentioned_member",
  values.answer_context_canary_specs_total,
  errors
);
const coverageSpeakerCanarySpecs = readCoverageCanarySpecs(
  coverage,
  "answer_context_speaker_canary_specs",
  "speaker_self",
  values.answer_context_speaker_canary_specs_total,
  errors
);
const coverageReferencedCanarySpecs = readCoverageCanarySpecs(
  coverage,
  "answer_context_referenced_canary_specs",
  "referenced_member",
  values.answer_context_referenced_canary_specs_total,
  errors
);
if (!canonicalKeySetsEqual(coverageMentionCanarySpecs, coverageSpeakerCanarySpecs)) {
  errors.push(
    "coverage mentioned-member and speaker self-canary specs must cover the same canonical people"
  );
}
if (!canonicalKeySetsEqual(coverageMentionCanarySpecs, coverageReferencedCanarySpecs)) {
  errors.push(
    "coverage mentioned-member and referenced-member canary specs must cover the same canonical people"
  );
}

if (errors.length === 0) {
  requireEqual(
    values.qiwe_channel_identities_total + values.qiwe_channel_identities_excluded,
    values.qiwe_channel_identities_raw_total,
    "safe and excluded QiWe identity counts must add up to raw total",
    errors
  );
  requireEqual(
    values.qiwe_room_channel_identities_total +
      values.qiwe_room_channel_identities_excluded,
    values.qiwe_room_channel_identities_raw_total,
    "safe and excluded current-room QiWe identity counts must add up to current-room raw total",
    errors
  );
  requireEqual(
    values.qiwe_channel_identities_raw_total,
    values.qiwe_room_channel_identities_raw_total,
    "QiWe raw identity coverage must be room-scoped",
    errors
  );
  requireEqual(
    values.qiwe_channel_identities_total,
    values.qiwe_room_channel_identities_total,
    "QiWe safe identity coverage must be room-scoped",
    errors
  );
  requireEqual(
    values.qiwe_channel_identities_linked,
    values.qiwe_room_channel_identities_linked,
    "QiWe linked identity coverage must be room-scoped",
    errors
  );
  requireEqual(
    values.qiwe_channel_identities_excluded,
    values.qiwe_room_channel_identities_excluded,
    "QiWe excluded identity coverage must be room-scoped",
    errors
  );
  requireEqual(
    values.qiwe_channel_identities_linked,
    values.qiwe_channel_identities_total,
    "all safe QiWe channel identities must be linked",
    errors
  );
  requireEqual(
    values.qiwe_room_channel_identities_linked,
    values.qiwe_room_channel_identities_total,
    "all current-room safe QiWe channel identities must be linked",
    errors
  );
  requireEqual(
    values.qiwe_room_potential_member_identities_linked +
      values.qiwe_room_potential_member_identities_unlinked,
    values.qiwe_room_potential_member_identities_total,
    "current-room potential member identity counts must add up",
    errors
  );
  requireZero(
    values.qiwe_room_potential_member_identities_unlinked,
    "unlinked current-room potential member identities",
    errors
  );
  requireZero(
    values.total_channel_identities,
    "non-ambiguous unlinked identities",
    errors
  );
  requireZero(
    values.ambiguous_channel_identities_skipped,
    "ambiguous identities",
    errors
  );
  requireZero(values.linked_aliases_missing, "missing linked aliases", errors);
  requireZero(
    values.linked_messages_missing_sender_person,
    "messages missing sender_person_id",
    errors
  );
  requireZero(
    values.qiwe_platform_identities_missing,
    "missing QiWe platform identities",
    errors
  );
  requireZero(
    values.qiwe_platform_identity_ambiguous_users,
    "QiWe users linked to multiple people",
    errors
  );
  requireZero(
    values.linked_people_without_qiwe_platform_identity,
    "linked people without QiWe platform identity for speaker recognition",
    errors
  );
  requireZero(
    values.running_people_profile_missing_running_hint,
    "running facts without running profile hints",
    errors
  );
  requireZero(
    values.linked_people_without_answer_context_canary_spec,
    "linked people without safe canary names",
    errors
  );
  requireEqual(
    values.linked_people_with_active_profile +
      values.linked_people_without_active_profile,
    values.linked_people_total,
    "linked people profile counts must add up to linked people total",
    errors
  );
  if (args.requireActiveProfiles) {
    requireZero(
      values.linked_people_without_active_profile,
      "linked people without active reply_context profiles",
      errors
    );
    requireEqual(
      values.linked_people_with_active_profile,
      values.linked_people_total,
      "active reply_context profiles must cover every linked person",
      errors
    );
  }
  requireEqual(
    values.answer_context_canary_people_total,
    values.linked_people_total,
    "answer-context canary people must cover every linked person",
    errors
  );
  requireEqual(
    values.answer_context_speaker_canary_people_total,
    values.linked_people_total,
    "answer-context speaker self-canary people must cover every linked person",
    errors
  );
  requireEqual(
    values.answer_context_referenced_canary_people_total,
    values.linked_people_total,
    "answer-context referenced-member canary people must cover every linked person",
    errors
  );
  requireEqual(
    values.running_people_with_profile_running_hint,
    values.linked_people_with_running_facts,
    "running profile hints must cover every linked person with running facts",
    errors
  );
  if (
    roomSyncStats.roomMembersDiscovered !== undefined &&
    values.qiwe_room_channel_identities_raw_total !==
      roomSyncStats.roomMembersDiscovered
  ) {
    errors.push(
      `coverage current-room raw QiWe identity count must match synced room roster: expected ${roomSyncStats.roomMembersDiscovered}, got ${values.qiwe_room_channel_identities_raw_total}`
    );
  }
  if (
    roomSyncStats.scopeFingerprint &&
    coverageScopeFingerprint &&
    roomSyncStats.scopeFingerprint !== coverageScopeFingerprint
  ) {
    errors.push("room member sync and coverage scope_fingerprint must match");
  }
  if (
    roomSyncStats.scopeFingerprint &&
    profileStats.scopeFingerprint &&
    profileStats.scopeFingerprint !== roomSyncStats.scopeFingerprint
  ) {
    errors.push("member profile scope_fingerprint must match room member sync scope");
  }
  if (
    values.linked_people_with_active_profile > 0 &&
    profileStats.valuableMessages <= 0 &&
    profileStats.baselineProfileTargets <= 0
  ) {
    errors.push(
      "member profile evidence must include valuable messages or baseline profile targets when active profiles are claimed"
    );
  }
}

const canaryStats = checkCanaries(canaries, errors);
checkCanaryEvidenceMatchesCoverageSpecs(
  canaries,
  [
    ...coverageMentionCanarySpecs,
    ...coverageSpeakerCanarySpecs,
    ...coverageReferencedCanarySpecs,
  ],
  errors
);
if (values.answer_context_canary_specs_total !== undefined) {
  requireEqual(
    canaryStats.mentionedRecordCount,
    values.answer_context_canary_specs_total,
    "mentioned-member canary record count must match coverage answer_context_canary_specs_total",
    errors
  );
}
if (values.answer_context_speaker_canary_specs_total !== undefined) {
  requireEqual(
    canaryStats.speakerRecordCount,
    values.answer_context_speaker_canary_specs_total,
    "speaker self-canary record count must match coverage answer_context_speaker_canary_specs_total",
    errors
  );
}
if (values.answer_context_referenced_canary_specs_total !== undefined) {
  requireEqual(
    canaryStats.referencedRecordCount,
    values.answer_context_referenced_canary_specs_total,
    "referenced-member canary record count must match coverage answer_context_referenced_canary_specs_total",
    errors
  );
}
if (values.answer_context_canary_people_total !== undefined) {
  requireEqual(
    canaryStats.resolvedPeople.size,
    values.answer_context_canary_people_total,
    "canary resolved people must match coverage answer_context_canary_people_total",
    errors
  );
}
if (values.answer_context_speaker_canary_people_total !== undefined) {
  requireEqual(
    canaryStats.speakerResolvedPeople.size,
    values.answer_context_speaker_canary_people_total,
    "speaker self-canary resolved people must match coverage answer_context_speaker_canary_people_total",
    errors
  );
}
if (values.answer_context_referenced_canary_people_total !== undefined) {
  requireEqual(
    canaryStats.referencedResolvedPeople.size,
    values.answer_context_referenced_canary_people_total,
    "referenced-member resolved people must match coverage answer_context_referenced_canary_people_total",
    errors
  );
}
if (
  !setsEqual(canaryStats.mentionedResolvedPeople, canaryStats.speakerResolvedPeople)
) {
  errors.push(
    "mentioned-member and speaker self-canary evidence must resolve the same people"
  );
}
if (
  !setsEqual(canaryStats.mentionedResolvedPeople, canaryStats.referencedResolvedPeople)
) {
  errors.push(
    "mentioned-member and referenced-member canary evidence must resolve the same people"
  );
}
if (
  !setsEqual(
    canaryStats.mentionedProfileHintPeople,
    canaryStats.speakerProfileHintPeople
  )
) {
  errors.push(
    "mentioned-member and speaker self-canary profile hint evidence must cover the same people"
  );
}
if (
  !setsEqual(
    canaryStats.mentionedProfileHintPeople,
    canaryStats.referencedProfileHintPeople
  )
) {
  errors.push(
    "mentioned-member and referenced-member profile hint evidence must cover the same people"
  );
}
if (values.linked_people_without_active_profile !== undefined) {
  requireEqual(
    canaryStats.identityOnlyPeople.size,
    values.linked_people_without_active_profile,
    "identity-only canary people must match linked people without active profiles",
    errors
  );
}
if (args.requireActiveProfiles) {
  requireEqual(
    canaryStats.identityOnlyPeople.size,
    0,
    "identity-only canary people must be zero for full-profile completion",
    errors
  );
}

if (errors.length > 0) {
  console.error("Erhua member recognition completion check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

const completionSummary = buildCompletionSummary({
  coverageScopeFingerprint,
  roomSyncStats,
  profileStats,
  values,
  canaryStats,
});
if (args.summaryOutput) {
  fs.writeFileSync(
    path.resolve(args.summaryOutput),
    `${JSON.stringify(completionSummary, null, 2)}\n`,
    "utf8"
  );
}

console.log(
  `Erhua member recognition completion check passed: ${roomSyncStats.roomMembersDiscovered} synced room members, ${values.qiwe_channel_identities_linked}/${values.qiwe_channel_identities_total} safe QiWe identities linked, ${values.qiwe_room_potential_member_identities_linked}/${values.qiwe_room_potential_member_identities_total} potential member identities linked, ${canaryStats.mentionedRecordCount} mentioned canaries, ${canaryStats.speakerRecordCount} speaker canaries, ${canaryStats.referencedRecordCount} referenced canaries, ${canaryStats.resolvedPeople.size}/${values.linked_people_total} linked people resolved.`
);

function buildCompletionSummary({
  coverageScopeFingerprint,
  roomSyncStats,
  profileStats,
  values,
  canaryStats,
}) {
  return {
    schema_version: "erhua_member_recognition_completion_v1",
    passed: true,
    scope_fingerprint: coverageScopeFingerprint,
    room_sync: {
      source: "current_qiwe_room_member_roster",
      dry_run: false,
      room_members_discovered: roomSyncStats.roomMembersDiscovered,
      room_member_identities_upserted: roomSyncStats.roomMemberIdentitiesUpserted,
      stale_room_member_identities_marked:
        roomSyncStats.staleRoomMemberIdentitiesMarked,
    },
    current_room_qiwe_identities: {
      raw_total: values.qiwe_room_channel_identities_raw_total,
      safe_total: values.qiwe_room_channel_identities_total,
      linked: values.qiwe_room_channel_identities_linked,
      excluded: values.qiwe_room_channel_identities_excluded,
      potential_member_total: values.qiwe_room_potential_member_identities_total,
      potential_member_linked: values.qiwe_room_potential_member_identities_linked,
      potential_member_unlinked: values.qiwe_room_potential_member_identities_unlinked,
      unsafe_display_unlinked: nonNegativeDifference(
        values.qiwe_room_potential_member_identities_unlinked,
        values.total_channel_identities
      ),
    },
    linked_people: {
      total: values.linked_people_total,
      with_active_profile: values.linked_people_with_active_profile,
      without_active_profile: values.linked_people_without_active_profile,
      without_qiwe_platform_identity:
        values.linked_people_without_qiwe_platform_identity,
      without_answer_context_canary_spec:
        values.linked_people_without_answer_context_canary_spec,
    },
    qiwe_speaker_identities: {
      materializable_users: values.qiwe_platform_identity_materializable_users,
      platform_identities_missing: values.qiwe_platform_identities_missing,
      ambiguous_users: values.qiwe_platform_identity_ambiguous_users,
      linked_people_without_platform_identity:
        values.linked_people_without_qiwe_platform_identity,
    },
    profile_repair: {
      dry_run: false,
      requested_message_limit: profileStats.requestedMessageLimit,
      current_room_linked_people: profileStats.currentRoomLinkedPeople,
      baseline_profile_targets: profileStats.baselineProfileTargets,
      messages_scanned: profileStats.messagesScanned,
      valuable_messages: profileStats.valuableMessages,
      baseline_profiles_inserted: profileStats.baselineProfilesInserted,
    },
    running_profile_hints: {
      linked_people_with_running_facts: values.linked_people_with_running_facts,
      running_people_with_profile_running_hint:
        values.running_people_with_profile_running_hint,
      running_people_profile_missing_running_hint:
        values.running_people_profile_missing_running_hint,
    },
    answer_context_canaries: {
      mentioned_records: canaryStats.mentionedRecordCount,
      speaker_records: canaryStats.speakerRecordCount,
      referenced_records: canaryStats.referencedRecordCount,
      mentioned_people_resolved: canaryStats.mentionedResolvedPeople.size,
      speaker_people_resolved: canaryStats.speakerResolvedPeople.size,
      referenced_people_resolved: canaryStats.referencedResolvedPeople.size,
      linked_people_resolved: canaryStats.resolvedPeople.size,
      mentioned_profile_hint_people: canaryStats.mentionedProfileHintPeople.size,
      speaker_profile_hint_people: canaryStats.speakerProfileHintPeople.size,
      referenced_profile_hint_people: canaryStats.referencedProfileHintPeople.size,
      linked_profile_hint_people: canaryStats.profileHintPeople.size,
      identity_only_people: canaryStats.identityOnlyPeople.size,
    },
    retained_evidence_boundary: {
      sanitized_summary_only: true,
      includes_chat_id: false,
      includes_sender_id: false,
      includes_channel_user_id: false,
      includes_person_id: false,
      includes_raw_messages: false,
      includes_hidden_profile_details: false,
      includes_database_url: false,
      includes_tokens: false,
    },
  };
}

function checkRoomSync(record, errors) {
  const scopeFingerprint = readScopeFingerprint(
    record,
    "room member sync scope_fingerprint",
    errors
  );
  const roomMembersDiscovered = readNonNegativeInteger(
    record,
    "room_members_discovered",
    errors
  );
  const roomMemberIdentitiesUpserted = readNonNegativeInteger(
    record,
    "room_member_identities_upserted",
    errors
  );
  const staleRoomMemberIdentitiesMarked = readNonNegativeInteger(
    record,
    "stale_room_member_identities_marked",
    errors
  );
  if (errors.length > 0) {
    return {
      roomMembersDiscovered,
      roomMemberIdentitiesUpserted,
      staleRoomMemberIdentitiesMarked,
      scopeFingerprint,
    };
  }
  if (record.source !== "current_qiwe_room_member_roster") {
    errors.push("room member sync source must be current_qiwe_room_member_roster");
  }
  if (record.dry_run === true) {
    errors.push("room member sync evidence for completion must be an applied run");
  }
  if (roomMembersDiscovered <= 0) {
    errors.push("room member sync must discover at least one room member");
  }
  if (roomMemberIdentitiesUpserted !== roomMembersDiscovered) {
    errors.push(
      "applied room member sync must upsert every discovered room member identity"
    );
  }
  return {
    roomMembersDiscovered,
    roomMemberIdentitiesUpserted,
    staleRoomMemberIdentitiesMarked,
    scopeFingerprint,
  };
}

function checkProfile(record, errors) {
  const requestedMessageLimit = readNonNegativeInteger(
    record,
    "requested_message_limit",
    errors
  );
  const currentRoomLinkedPeople = readNonNegativeInteger(
    record,
    "current_room_linked_people",
    errors
  );
  const baselineProfileTargets = readNonNegativeInteger(
    record,
    "baseline_profile_targets",
    errors
  );
  const messagesScanned = readNonNegativeInteger(record, "messages_scanned", errors);
  readNonNegativeInteger(record, "messages_skipped_without_person", errors);
  readNonNegativeInteger(record, "messages_skipped_excluded_identity", errors);
  const valuableMessages = readNonNegativeInteger(record, "valuable_messages", errors);
  readNonNegativeInteger(record, "candidate_fact_count", errors);
  readNonNegativeInteger(record, "facts_inserted", errors);
  readNonNegativeInteger(record, "summaries_inserted", errors);
  readNonNegativeInteger(record, "snapshots_inserted", errors);
  const baselineProfilesInserted = readNonNegativeInteger(
    record,
    "baseline_profiles_inserted",
    errors
  );
  const scopeFingerprints = readScopeFingerprintArray(
    record,
    "member profile scope_fingerprints",
    errors
  );
  if (scopeFingerprints && scopeFingerprints.length !== 1) {
    errors.push("member profile evidence must contain exactly one scope_fingerprint");
  }
  if (record.dry_run === true) {
    errors.push("member profile evidence for completion must be an applied run");
  }
  if (messagesScanned !== undefined && messagesScanned <= 0) {
    errors.push("member profile evidence must scan at least one message");
  }
  if (
    requestedMessageLimit !== undefined &&
    requestedMessageLimit < MIN_PROFILE_REPAIR_MESSAGE_LIMIT
  ) {
    errors.push(
      `member profile evidence must be generated with --limit ${MIN_PROFILE_REPAIR_MESSAGE_LIMIT} or higher for one-shot recognition repair`
    );
  }
  if (
    baselineProfileTargets !== undefined &&
    currentRoomLinkedPeople !== undefined &&
    baselineProfileTargets > currentRoomLinkedPeople
  ) {
    errors.push("baseline profile targets exceed current-room linked people");
  }
  if (
    baselineProfilesInserted !== undefined &&
    baselineProfileTargets !== undefined &&
    baselineProfilesInserted > baselineProfileTargets
  ) {
    errors.push("baseline profiles inserted exceed baseline profile targets");
  }
  return {
    scopeFingerprint: scopeFingerprints?.[0],
    requestedMessageLimit,
    messagesScanned,
    valuableMessages,
    currentRoomLinkedPeople,
    baselineProfileTargets,
    baselineProfilesInserted,
  };
}

function checkCanaries(records, errors) {
  if (!Array.isArray(records) || records.length === 0) {
    errors.push("canary evidence must include at least one record");
    return {
      resolvedPeople: new Set(),
      mentionedResolvedPeople: new Set(),
      speakerResolvedPeople: new Set(),
      referencedResolvedPeople: new Set(),
      profileHintPeople: new Set(),
      mentionedProfileHintPeople: new Set(),
      speakerProfileHintPeople: new Set(),
      referencedProfileHintPeople: new Set(),
      canonicalPeople: new Map(),
      identityOnlyPeople: new Set(),
      mentionedRecordCount: 0,
      speakerRecordCount: 0,
      referencedRecordCount: 0,
    };
  }
  const resolvedPeople = new Set();
  const mentionedResolvedPeople = new Set();
  const speakerResolvedPeople = new Set();
  const referencedResolvedPeople = new Set();
  const profileHintPeople = new Set();
  const mentionedProfileHintPeople = new Set();
  const speakerProfileHintPeople = new Set();
  const referencedProfileHintPeople = new Set();
  const canonicalPeople = new Map();
  const identityOnlyPeople = new Set();
  let mentionedRecordCount = 0;
  let speakerRecordCount = 0;
  let referencedRecordCount = 0;
  for (const [index, record] of records.entries()) {
    const canaryType = canaryTypeOf(record, errors);
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
      if (
        checkSafeProfile(record, speaker, label, identityOnlyPeople, personRef, errors)
      ) {
        profileHintPeople.add(personRef);
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
      if (
        checkSafeProfile(
          record,
          referencedMember,
          label,
          identityOnlyPeople,
          personRef,
          errors
        )
      ) {
        profileHintPeople.add(personRef);
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
      if (
        checkSafeProfile(record, member, label, identityOnlyPeople, personRef, errors)
      ) {
        profileHintPeople.add(personRef);
        mentionedProfileHintPeople.add(personRef);
      }
      checkCanonical(record, personRef, label, canonicalPeople, errors);
    }
  }
  return {
    resolvedPeople,
    mentionedResolvedPeople,
    speakerResolvedPeople,
    referencedResolvedPeople,
    profileHintPeople,
    mentionedProfileHintPeople,
    speakerProfileHintPeople,
    referencedProfileHintPeople,
    canonicalPeople,
    identityOnlyPeople,
    mentionedRecordCount,
    speakerRecordCount,
    referencedRecordCount,
  };
}

function readEvidence(file, label) {
  const text = fs.readFileSync(path.resolve(file), "utf8");
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
    /"target_chat_ids"\s*:/,
    /"raw_messages"\s*:/,
    /"candidate_facts"\s*:/,
    /"source_message_id"\s*:/,
    /"hidden_profile_details"\s*:/,
    /"raw"\s*:/,
  ]) {
    if (pattern.test(text)) {
      fail(`${label} contains forbidden sensitive fragment: ${pattern}`);
    }
  }
  return text;
}

function assertNoVisiblePhoneLike(value, label) {
  const visibleText = visibleTextFields(value).join("\n");
  if (/1[3-9]\d{9}/.test(visibleText)) {
    fail(`${label} contains forbidden sensitive fragment: /1[3-9]\\d{9}/`);
  }
}

function visibleTextFields(value, key = "") {
  if (
    key === "person_ref" ||
    key === "canonical_key" ||
    key === "same_person_key" ||
    key === "scope_fingerprint"
  ) {
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

function parseRoomSyncEvidence(text) {
  const trimmed = text.trim();
  if (trimmed.startsWith("{")) {
    return parseJson(trimmed, "room member sync JSON");
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
  const prefix = "erhua_room_member_sync=";
  for (const [index, line] of text.split(/\r?\n/).entries()) {
    const trimmedLine = line.trim();
    if (!trimmedLine.startsWith(prefix)) {
      continue;
    }
    return parseJson(
      trimmedLine.slice(prefix.length),
      `room member sync line ${index + 1}`
    );
  }
  fail("room member sync evidence does not contain JSON output");
}

function parseProfileEvidence(text) {
  const trimmed = text.trim();
  if (trimmed.startsWith("{")) {
    return parseJson(trimmed, "member profile JSON");
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
  fail("member profile evidence does not contain JSON output");
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

function selectMentionedMember(members, label) {
  return members.find((member) => {
    if (!member || typeof member !== "object") {
      return false;
    }
    return member.mention_text === label;
  });
}

function checkSafeProfile(
  record,
  target,
  label,
  identityOnlyPeople,
  personRef,
  errors
) {
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
    identityOnlyPeople.add(personRef);
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
  } else if (args.requireActiveProfiles && !hasSafeProfileHint(hints)) {
    errors.push(
      `${label}: full-profile completion requires non-empty safe profile hints`
    );
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

function readCoverageCanarySpecs(record, field, expectedType, expectedCount, errors) {
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
      sourceLabel: label,
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
    errors.push(
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
    errors.push(`${label} must include canonical_key`);
  }
  return {
    canaryType: expectedType,
    label: expectedLabel,
    canonicalKey,
    requiredProfileTerms: normalizeProfileTerms(spec.required_profile_terms),
    sourceLabel: label,
  };
}

function checkCanaryEvidenceMatchesCoverageSpecs(records, expectedSpecs, errors) {
  const expectedByKey = new Map();
  for (const spec of expectedSpecs) {
    if (!spec.label || !spec.canonicalKey) {
      continue;
    }
    const key = canarySpecKey(spec);
    if (expectedByKey.has(key)) {
      errors.push(`coverage canary spec is duplicated: ${formatCanarySpec(spec)}`);
    } else {
      expectedByKey.set(key, spec);
    }
  }

  const seen = new Map();
  for (const [index, record] of records.entries()) {
    const canaryType = canaryTypeOf(record, errors);
    const label =
      canaryType === "speaker_self"
        ? textField(record, ["expected_speaker_label"])
        : canaryType === "referenced_member"
          ? textField(record, ["expected_referenced_label"])
          : textField(record, ["expected_mention", "name"]);
    const canonicalKey = textField(record, ["canonical_key", "same_person_key"]);
    if (!label) {
      errors.push(
        `canary ${index + 1} must include ${
          canaryType === "speaker_self"
            ? "expected_speaker_label"
            : canaryType === "referenced_member"
              ? "expected_referenced_label"
              : "expected_mention"
        } from coverage spec`
      );
      continue;
    }
    if (!canonicalKey) {
      errors.push(`canary ${index + 1} must include canonical_key from coverage spec`);
      continue;
    }
    const evidenceSpec = {
      canaryType,
      label,
      canonicalKey,
      requiredProfileTerms: normalizeProfileTerms(record.required_profile_terms),
      sourceLabel: `canary ${index + 1}`,
    };
    const key = canarySpecKey(evidenceSpec);
    const expected = expectedByKey.get(key);
    if (!expected) {
      errors.push(
        `${formatCanarySpec(evidenceSpec)} was not generated by coverage canary specs`
      );
      continue;
    }
    if (seen.has(key)) {
      errors.push(`${formatCanarySpec(evidenceSpec)} appears more than once`);
    } else {
      seen.set(key, evidenceSpec);
    }
    const evidenceTerms = termsFingerprint(evidenceSpec.requiredProfileTerms);
    const expectedTerms = termsFingerprint(expected.requiredProfileTerms);
    if (evidenceTerms !== expectedTerms) {
      errors.push(
        `${formatCanarySpec(evidenceSpec)} required_profile_terms must match coverage spec: expected [${expected.requiredProfileTerms.join(
          ", "
        )}], got [${evidenceSpec.requiredProfileTerms.join(", ")}]`
      );
    }
  }

  for (const [key, spec] of expectedByKey.entries()) {
    if (!seen.has(key)) {
      errors.push(`missing canary evidence for ${formatCanarySpec(spec)}`);
    }
  }
}

function canarySpecKey(spec) {
  return `${spec.canaryType}\0${spec.label}\0${spec.canonicalKey}`;
}

function formatCanarySpec(spec) {
  return `${spec.canaryType} ${JSON.stringify(spec.label)} (${spec.canonicalKey})`;
}

function normalizeProfileTerms(value) {
  if (!Array.isArray(value)) {
    return [];
  }
  return [...new Set(value.map(asText).filter(Boolean))].sort();
}

function termsFingerprint(terms) {
  return terms.join("\0");
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

function readNonNegativeInteger(record, field, errors) {
  const value = record[field];
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

function readScopeFingerprintArray(record, label, errors) {
  const value = record?.scope_fingerprints;
  if (!Array.isArray(value) || value.length === 0) {
    errors.push(`${label} must be a non-empty array`);
    return undefined;
  }
  const fingerprints = [];
  for (const [index, item] of value.entries()) {
    if (typeof item !== "string" || !/^sha256:[0-9a-f]{64}$/.test(item.trim())) {
      errors.push(`${label}[${index}] must be a canonical sha256 marker`);
      continue;
    }
    fingerprints.push(item.trim());
  }
  return fingerprints;
}

function requireZero(value, label, errors) {
  if (value !== 0) {
    errors.push(`${label} must be 0, got ${value}`);
  }
}

function requireEqual(actual, expected, label, errors) {
  if (actual !== expected) {
    errors.push(`${label}: expected ${expected}, got ${actual}`);
  }
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

function asText(value) {
  return typeof value === "string" ? value.trim() : "";
}

function isPersonRef(value) {
  return typeof value === "string" && /^sha256:[0-9a-f]{64}$/.test(value.trim());
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

function nonNegativeDifference(left, right) {
  if (!Number.isInteger(left) || !Number.isInteger(right)) {
    return null;
  }
  return Math.max(0, left - right);
}

function parseArgs(argv) {
  const parsed = {};
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--coverage") {
      parsed.coverage = argv[++index];
    } else if (arg === "--room-sync") {
      parsed.roomSync = argv[++index];
    } else if (arg === "--profile") {
      parsed.profile = argv[++index];
    } else if (arg === "--canary") {
      parsed.canary = argv[++index];
    } else if (arg === "--summary-output") {
      parsed.summaryOutput = argv[++index];
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
