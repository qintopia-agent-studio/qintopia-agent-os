#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const args = process.argv.slice(2);
if (args.length !== 1) {
  fail(
    "usage: node tools/deploy/check-erhua-member-safe-alias-payload.mjs <safe-alias-payload.json>"
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
  /"raw_messages"\s*:/,
  /"hidden_profile_details"\s*:/,
  /"raw"\s*:/,
  /1[3-9]\d{9}/,
]) {
  if (pattern.test(payloadText)) {
    fail(`safe alias payload contains forbidden sensitive fragment: ${pattern}`);
  }
}

const payload = parseJson(payloadText, "safe alias payload");
const errors = [];
if (!payload || typeof payload !== "object" || Array.isArray(payload)) {
  errors.push("safe alias payload must be a JSON object");
}

const allowedRootFields = new Set(["aliases"]);
for (const field of Object.keys(payload || {})) {
  if (!allowedRootFields.has(field)) {
    errors.push(`unknown root field: ${field}`);
  }
}

const aliases = Array.isArray(payload?.aliases) ? payload.aliases : null;
if (!aliases || aliases.length === 0) {
  errors.push("safe alias payload must include a non-empty aliases array");
}

const seen = new Set();
const seenAliases = new Set();
for (const [index, item] of (aliases || []).entries()) {
  const label = `aliases[${index}]`;
  if (!item || typeof item !== "object" || Array.isArray(item)) {
    errors.push(`${label} must be an object`);
    continue;
  }
  const allowedFields = new Set([
    "person_key",
    "alias",
    "source_display_name",
    "reason",
  ]);
  for (const field of Object.keys(item)) {
    if (!allowedFields.has(field)) {
      errors.push(`${label} has unknown field: ${field}`);
    }
  }
  const personKey = asText(item.person_key).toLowerCase();
  const alias = normalizeAlias(asText(item.alias));
  const aliasKey = alias.toLowerCase();
  if (!/^[0-9a-f]{12,32}$/.test(personKey)) {
    errors.push(`${label}.person_key must be a 12-32 character md5 hex prefix`);
  }
  validateAlias(alias, `${label}.alias`, errors);
  const sourceDisplayName = asText(item.source_display_name);
  if (sourceDisplayName && hasControl(sourceDisplayName)) {
    errors.push(`${label}.source_display_name must not contain control characters`);
  }
  const reason = asText(item.reason);
  if (reason && hasUnsafeText(reason)) {
    errors.push(`${label}.reason contains unsafe or sensitive text`);
  }
  const duplicateKey = `${personKey}\0${aliasKey}`;
  if (seen.has(duplicateKey)) {
    errors.push(`${label} duplicates another person_key and alias`);
  }
  seen.add(duplicateKey);
  if (seenAliases.has(aliasKey)) {
    errors.push(`${label}.alias duplicates another reviewed alias`);
  }
  seenAliases.add(aliasKey);
}

if (errors.length > 0) {
  console.error("Erhua member safe alias payload check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log(`Erhua member safe alias payload check passed: ${aliases.length} aliases.`);

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
