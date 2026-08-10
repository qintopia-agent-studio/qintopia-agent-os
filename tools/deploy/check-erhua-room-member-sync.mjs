#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const args = process.argv.slice(2);
if (args.length !== 1) {
  fail(
    "usage: node tools/deploy/check-erhua-room-member-sync.mjs <identity-backfill-room-member-sync-output.json>"
  );
}

const evidencePath = path.resolve(args[0]);
const evidenceText = fs.readFileSync(evidencePath, "utf8");

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
  /"raw_messages"\s*:/,
  /"hidden_profile_details"\s*:/,
  /"raw"\s*:/,
  /1[3-9]\d{9}/,
]) {
  if (pattern.test(evidenceText)) {
    fail(`room member sync evidence contains forbidden sensitive fragment: ${pattern}`);
  }
}

const evidence = parseEvidence(evidenceText);
const errors = [];
const roomMembersDiscovered = readNonNegativeInteger(
  evidence,
  "room_members_discovered",
  errors
);
const roomMemberIdentitiesUpserted = readNonNegativeInteger(
  evidence,
  "room_member_identities_upserted",
  errors
);
const staleRoomMemberIdentitiesMarked = readNonNegativeInteger(
  evidence,
  "stale_room_member_identities_marked",
  errors
);
const scopeFingerprint = readScopeFingerprint(evidence, errors);
const dryRun = evidence.dry_run === true;

if (errors.length === 0) {
  if (evidence.source !== "current_qiwe_room_member_roster") {
    errors.push("room member sync source must be current_qiwe_room_member_roster");
  }
  if (roomMembersDiscovered <= 0) {
    errors.push("room member sync must discover at least one room member");
  }
  if (!dryRun && roomMemberIdentitiesUpserted !== roomMembersDiscovered) {
    errors.push(
      "applied room member sync must upsert every discovered room member identity"
    );
  }
  if (dryRun && roomMemberIdentitiesUpserted !== 0) {
    errors.push("dry-run room member sync must not report upserted identities");
  }
  if (dryRun && staleRoomMemberIdentitiesMarked !== 0) {
    errors.push("dry-run room member sync must not report stale identities marked");
  }
}

if (errors.length > 0) {
  console.error("Erhua room member sync check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

const mode = dryRun ? "dry-run" : "applied";
console.log(
  `Erhua room member sync check passed: ${roomMembersDiscovered} room members discovered, ${roomMemberIdentitiesUpserted} identities upserted, ${staleRoomMemberIdentitiesMarked} stale identities marked, mode=${mode}, scope=${scopeFingerprint}.`
);

function parseEvidence(text) {
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
  for (const [index, line] of lines.entries()) {
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

function readNonNegativeInteger(object, field, errors) {
  const value = object?.[field];
  if (!Number.isInteger(value) || value < 0) {
    errors.push(`${field} must be a non-negative integer`);
    return undefined;
  }
  return value;
}

function readScopeFingerprint(object, errors) {
  const value = object?.scope_fingerprint;
  if (typeof value !== "string" || !/^sha256:[0-9a-f]{64}$/.test(value.trim())) {
    errors.push("scope_fingerprint must be a canonical sha256 marker");
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

function fail(message) {
  console.error(message);
  process.exit(1);
}
