#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const args = parseArgs(process.argv.slice(2));
if (!args.coverage) {
  fail(
    "usage: node tools/deploy/build-erhua-member-safe-alias-payload-template.mjs --coverage <identity-bootstrap-dry-run-output.json> [--output <safe-alias-template.json>]"
  );
}

const coveragePath = path.resolve(args.coverage);
const coverageText = fs.readFileSync(coveragePath, "utf8");
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
  if (pattern.test(coverageText)) {
    fail(`coverage evidence contains forbidden sensitive fragment: ${pattern}`);
  }
}

const coverage = parseCoverage(coverageText);
const samples = sampleArray(coverage, [
  "linked_people_without_answer_context_canary_spec_samples",
  "missing_answer_context_canary_name_samples",
]);
if (samples.length === 0) {
  fail("coverage evidence has no missing safe alias samples");
}

const seen = new Set();
const aliases = [];
for (const [index, sample] of samples.entries()) {
  if (!sample || typeof sample !== "object" || Array.isArray(sample)) {
    fail(`sample ${index + 1} must be an object`);
  }
  const personKey = asText(sample.person_key).toLowerCase();
  if (!/^[0-9a-f]{12,32}$/.test(personKey)) {
    fail(`sample ${index + 1} is missing a valid person_key`);
  }
  if (seen.has(personKey)) {
    continue;
  }
  seen.add(personKey);
  aliases.push({
    person_key: personKey,
    alias: "",
    source_display_name: asText(sample.display_name),
    reason: "owner reviewed safe member name for answer-context canary coverage",
  });
}
const expectedCount = readOptionalNonNegativeInteger(
  coverage,
  "linked_people_without_answer_context_canary_spec"
);
if (expectedCount !== null && aliases.length !== expectedCount) {
  fail(
    `safe alias template would cover ${aliases.length}/${expectedCount} linked people without safe canary names; rerun coverage with complete candidates before review`
  );
}

const payload = {
  aliases,
};
const output = JSON.stringify(payload, null, 2).concat("\n");

if (args.output) {
  fs.writeFileSync(path.resolve(args.output), output, "utf8");
} else {
  process.stdout.write(output);
}

function parseArgs(argv) {
  const parsed = {};
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--coverage") {
      parsed.coverage = argv[++index];
    } else if (arg === "--output") {
      parsed.output = argv[++index];
    } else {
      fail(`unknown argument: ${arg}`);
    }
  }
  return parsed;
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

function sampleArray(record, fieldNames) {
  for (const fieldName of fieldNames) {
    const value = record?.[fieldName];
    if (Array.isArray(value)) {
      return value;
    }
  }
  return [];
}

function readOptionalNonNegativeInteger(record, fieldName) {
  const value = record?.[fieldName];
  if (value === undefined || value === null) {
    return null;
  }
  if (!Number.isInteger(value) || value < 0) {
    fail(`${fieldName} must be a non-negative integer when present`);
  }
  return value;
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
