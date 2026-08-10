#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const args = process.argv.slice(2);
if (args.length !== 1) {
  fail(
    "usage: node tools/deploy/check-erhua-member-safe-identity-payload.mjs <safe-identity-payload.json>"
  );
}

const payloadPath = path.resolve(args[0]);
const payloadText = fs.readFileSync(payloadPath, "utf8");

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
  /"source_display_name"\s*:/,
  /"raw_messages"\s*:/,
  /"hidden_profile_details"\s*:/,
  /"raw"\s*:/,
  /1[3-9]\d{9}/,
]) {
  if (pattern.test(payloadText)) {
    fail(`safe identity payload contains forbidden sensitive fragment: ${pattern}`);
  }
}

const payload = parseJson(payloadText, "safe identity payload");
const errors = [];
if (!payload || typeof payload !== "object" || Array.isArray(payload)) {
  errors.push("safe identity payload must be a JSON object");
}

const allowedRootFields = new Set(["identities"]);
for (const field of Object.keys(payload || {})) {
  if (!allowedRootFields.has(field)) {
    errors.push(`unknown root field: ${field}`);
  }
}

const identities = Array.isArray(payload?.identities) ? payload.identities : null;
if (!identities || identities.length === 0) {
  errors.push("safe identity payload must include a non-empty identities array");
}

const seenIdentityKeys = new Set();
const seenSafeDisplayNames = new Set();
for (const [index, item] of (identities || []).entries()) {
  const label = `identities[${index}]`;
  if (!item || typeof item !== "object" || Array.isArray(item)) {
    errors.push(`${label} must be an object`);
    continue;
  }
  const allowedFields = new Set([
    "identity_key",
    "safe_display_name",
    "person_key",
    "reason",
  ]);
  for (const field of Object.keys(item)) {
    if (!allowedFields.has(field)) {
      errors.push(`${label} has unknown field: ${field}`);
    }
  }
  const identityKey = asText(item.identity_key).toLowerCase();
  if (!/^[0-9a-f]{12,32}$/.test(identityKey)) {
    errors.push(`${label}.identity_key must be a 12-32 character md5 hex prefix`);
  }
  if (seenIdentityKeys.has(identityKey)) {
    errors.push(`${label}.identity_key duplicates another reviewed identity`);
  }
  seenIdentityKeys.add(identityKey);

  const safeDisplayName = normalizeAlias(asText(item.safe_display_name));
  const safeDisplayNameKey = safeDisplayName.toLowerCase();
  validateAlias(safeDisplayName, `${label}.safe_display_name`, errors);
  if (seenSafeDisplayNames.has(safeDisplayNameKey)) {
    errors.push(`${label}.safe_display_name duplicates another reviewed safe name`);
  }
  seenSafeDisplayNames.add(safeDisplayNameKey);

  if (item.person_key !== undefined && item.person_key !== null) {
    const personKey = asText(item.person_key).toLowerCase();
    if (!/^[0-9a-f]{12,32}$/.test(personKey)) {
      errors.push(
        `${label}.person_key must be null or a 12-32 character md5 hex prefix`
      );
    }
  }

  const reason = asText(item.reason);
  if (reason && hasUnsafeText(reason)) {
    errors.push(`${label}.reason contains unsafe or sensitive text`);
  }
}

if (errors.length > 0) {
  console.error("Erhua member safe identity payload check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log(
  `Erhua member safe identity payload check passed: ${identities.length} identities.`
);

function validateAlias(alias, label, errors) {
  const length = [...alias].length;
  if (length < 2 || length > 40) {
    errors.push(`${label} must be 2-40 characters`);
  }
  if (hasControl(alias)) {
    errors.push(`${label} must not contain control characters`);
  }
  if (/^[0-9]+$/.test(alias)) {
    errors.push(`${label} must not be numeric-only`);
  }
  if (/\d{7,}/.test(alias)) {
    errors.push(`${label} must not contain phone-like digit runs`);
  }
  if (
    alias === "企业微信团队" ||
    alias === "秦托邦小客服" ||
    alias === "二花" ||
    alias.toLowerCase() === "sidecar smoke"
  ) {
    errors.push(`${label} must not be a system or test display name`);
  }
}

function normalizeAlias(value) {
  return value.split(/\s+/).filter(Boolean).join(" ");
}

function hasUnsafeText(value) {
  return hasControl(value) || /\d{7,}/.test(value);
}

function hasControl(value) {
  return /[\u0000-\u001f\u007f-\u009f]/.test(value);
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
