#!/usr/bin/env node

import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

export const CLASSIFIER_VERSION = "qintopia-low-risk-change-v3";

const MAX_CHANGED_FILES = 5;
const MAX_JSON_DEPTH = 32;
const MAX_JSON_NODES = 5_000;
const MAX_JSON_STRING_BYTES = 16 * 1024;
const MAX_JSON_KEY_BYTES = 256;
const MAX_MAPPING_SELECTOR_DEPTH = 8;
const MAX_MAPPING_PREDICATES = 64;
const MAX_MAPPING_TRANSFORMS = 8;
const MAX_EXPANDED_MAPPING_TRANSFORMS = 16;
const MAX_RESTRICTED_PRIMITIVE_OPERATIONS = 8;
const MAX_MAPPING_RECORDS = 64;
const SAFE_REF = /^[0-9A-Za-z][0-9A-Za-z._/-]{0,199}$/;
const SAFE_PATH_SEGMENT = /^[0-9A-Za-z][0-9A-Za-z._-]*$/;
const SAFE_IDENTIFIER = /^[a-z0-9][a-z0-9._:-]{0,127}$/;
const OFFICIAL_SOURCE_HOSTS = new Set(["doc.qiweapi.com"]);
const ALLOWED_LEAF_PREDICATES = new Set(["equals", "exists", "in", "type_is"]);
const ALLOWED_TRANSFORMS = new Set([
  "base64_utf8",
  "dedupe",
  "opaque_id",
  "split",
  "unix_timestamp",
  "restricted_primitive",
]);
const ALLOWED_RESTRICTED_PRIMITIVE_OPERATIONS = new Set([
  "array_flatten",
  "base64_utf8",
  "json_parse",
  "json_pointer",
  "split",
  "string_trim",
]);
const CANONICAL_EXTRACTOR_FIELDS = new Set([
  "event_id",
  "space_chat_id",
  "subject_user_ids",
  "occurred_at",
]);
const CANONICAL_EVENT_FIELDS = new Set([
  "event_type",
  "event_id",
  "space_id",
  "subject_user_ids",
  "occurred_at",
]);
const MAPPING_TOP_LEVEL_FIELDS = new Set([
  "schema_version",
  "provider",
  "definition_key",
  "selector",
  "extractor",
  "official_sources",
]);

const PATH_RULES = [
  {
    kind: "mapping",
    pattern:
      /^fixtures\/qiwe\/event-mappings\/(?:[0-9A-Za-z._-]+\/)*[0-9A-Za-z][0-9A-Za-z._-]*\.mapping\.json$/,
    maxBytes: 128 * 1024,
    allowedStatuses: new Set(["A"]),
  },
  {
    kind: "fixture",
    pattern:
      /^fixtures\/qiwe\/system\/(?:[0-9A-Za-z._-]+\/)*[0-9A-Za-z][0-9A-Za-z._-]*\.fixture\.json$/,
    maxBytes: 256 * 1024,
    allowedStatuses: new Set(["A"]),
  },
  {
    kind: "expectation",
    pattern:
      /^fixtures\/qiwe\/event-mappings\/(?:[0-9A-Za-z._-]+\/)*[0-9A-Za-z][0-9A-Za-z._-]*\.expected\.json$/,
    maxBytes: 128 * 1024,
    allowedStatuses: new Set(["A"]),
  },
  {
    kind: "primitive",
    pattern:
      /^fixtures\/qiwe\/event-mappings\/_primitives\/(?:[0-9A-Za-z][0-9A-Za-z._-]*\/)*[0-9A-Za-z][0-9A-Za-z._-]*\.primitive\.json$/,
    maxBytes: 64 * 1024,
    allowedStatuses: new Set(["A"]),
  },
  {
    kind: "documentation",
    pattern:
      /^fixtures\/qiwe\/event-mappings\/(?!_primitives\/)(?:[0-9A-Za-z._-]+\/)*[0-9A-Za-z][0-9A-Za-z._-]*\.mapping\.md$/,
    maxBytes: 24 * 1024,
    allowedStatuses: new Set(["A"]),
  },
];

const FORBIDDEN_KEY_PARTS = new Set([
  "authorization",
  "code",
  "command",
  "cookie",
  "credential",
  "credentials",
  "database",
  "deliver",
  "delivery",
  "dependency",
  "destination",
  "domain",
  "endpoint",
  "env",
  "eval",
  "exec",
  "header",
  "headers",
  "host",
  "http",
  "migration",
  "package",
  "password",
  "query",
  "script",
  "secret",
  "secrets",
  "send",
  "shell",
  "sql",
  "target",
  "token",
  "tokens",
  "webhook",
]);

function runGit(repoRoot, args, options = {}) {
  const result = spawnSync("git", args, {
    cwd: repoRoot,
    encoding: options.encoding === undefined ? "utf8" : options.encoding,
    maxBuffer: options.maxBuffer ?? 4 * 1024 * 1024,
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (result.status !== 0) {
    const stderr = Buffer.isBuffer(result.stderr)
      ? result.stderr.toString("utf8")
      : String(result.stderr ?? "");
    throw new Error(
      `git ${args[0]} failed: ${stderr.trim() || `exit ${result.status}`}`
    );
  }
  return result.stdout;
}

function validateRef(name, value) {
  if (
    typeof value !== "string" ||
    !SAFE_REF.test(value) ||
    value.includes("..") ||
    value.includes("//") ||
    value.endsWith(".") ||
    value.endsWith("/")
  ) {
    throw new Error(`${name} must be a bounded git ref or commit SHA`);
  }
}

function resolveCommit(repoRoot, ref) {
  return String(runGit(repoRoot, ["rev-parse", "--verify", `${ref}^{commit}`])).trim();
}

function isAncestor(repoRoot, baseSha, headSha) {
  const result = spawnSync("git", ["merge-base", "--is-ancestor", baseSha, headSha], {
    cwd: repoRoot,
    stdio: "ignore",
  });
  if (result.status === 0) {
    return true;
  }
  if (result.status === 1) {
    return false;
  }
  throw new Error("git merge-base --is-ancestor failed");
}

function validateRepositoryPath(relativePath) {
  if (
    typeof relativePath !== "string" ||
    relativePath.length === 0 ||
    relativePath.length > 512 ||
    relativePath.startsWith("/") ||
    relativePath.includes("\\") ||
    /[\u0000-\u001f\u007f]/.test(relativePath)
  ) {
    return false;
  }
  const segments = relativePath.split("/");
  return segments.every(
    (segment) =>
      segment !== "." &&
      segment !== ".." &&
      (segment === "_primitives" || SAFE_PATH_SEGMENT.test(segment))
  );
}

function changedEntries(repoRoot, baseSha, headSha) {
  const raw = String(
    runGit(repoRoot, [
      "diff",
      "--name-status",
      "-z",
      "--no-renames",
      baseSha,
      headSha,
      "--",
    ])
  );
  if (raw.length === 0) {
    return [];
  }
  const parts = raw.split("\0");
  if (parts.at(-1) === "") {
    parts.pop();
  }
  if (parts.length % 2 !== 0) {
    throw new Error("git diff returned an unexpected name-status record");
  }
  const entries = [];
  for (let index = 0; index < parts.length; index += 2) {
    entries.push({ status: parts[index], path: parts[index + 1] });
  }
  return entries;
}

function readHeadBlob(repoRoot, headSha, relativePath) {
  const raw = String(runGit(repoRoot, ["ls-tree", "-z", headSha, "--", relativePath]));
  const records = raw.split("\0").filter(Boolean);
  if (records.length !== 1) {
    throw new Error(`${relativePath}: expected exactly one git tree entry`);
  }
  const match = records[0].match(
    /^([0-7]{6}) (blob|tree|commit) ([0-9a-f]{40,64})\t([\s\S]+)$/
  );
  if (!match || match[4] !== relativePath) {
    throw new Error(`${relativePath}: malformed or mismatched git tree entry`);
  }
  const [, mode, objectType, objectId] = match;
  const size = Number(String(runGit(repoRoot, ["cat-file", "-s", objectId])).trim());
  if (!Number.isSafeInteger(size) || size < 0) {
    throw new Error(`${relativePath}: invalid git blob size`);
  }
  const content = runGit(repoRoot, ["cat-file", "blob", objectId], {
    encoding: null,
    maxBuffer: Math.max(size + 1, 1024),
  });
  return { mode, objectType, objectId, size, content };
}

function classifyPath(relativePath) {
  return PATH_RULES.find((rule) => rule.pattern.test(relativePath)) ?? null;
}

function loadExistingPrimitive(repoRoot, headSha, relativePath, errors) {
  const rule = classifyPath(relativePath);
  if (!rule || rule.kind !== "primitive") {
    errors.push(`${relativePath}:primitive_ref_not_allowlisted`);
    return null;
  }
  let blob;
  try {
    blob = readHeadBlob(repoRoot, headSha, relativePath);
  } catch {
    errors.push(`${relativePath}:primitive_ref_missing_from_head`);
    return null;
  }
  if (
    blob.objectType !== "blob" ||
    blob.mode !== "100644" ||
    blob.size > rule.maxBytes ||
    blob.content.includes(0)
  ) {
    errors.push(`${relativePath}:referenced_primitive_blob_is_invalid`);
    return null;
  }
  const text = blob.content.toString("utf8");
  if (!Buffer.from(text, "utf8").equals(blob.content)) {
    errors.push(`${relativePath}:referenced_primitive_is_not_utf8`);
    return null;
  }
  const contentErrors = [];
  const value = parseStrictJson(text, relativePath, contentErrors);
  const primitive =
    value === null ? null : validatePrimitive(value, relativePath, contentErrors);
  errors.push(...contentErrors);
  return contentErrors.length === 0 ? primitive : null;
}

function normalizeKey(key) {
  return key
    .replace(/([a-z0-9])([A-Z])/g, "$1_$2")
    .replace(/[^0-9A-Za-z]+/g, "_")
    .replace(/^_+|_+$/g, "")
    .toLowerCase();
}

function validateOfficialUrl(value, keyPath, errors) {
  let parsed;
  try {
    parsed = new URL(value);
  } catch {
    errors.push(`${keyPath}: URL is invalid`);
    return;
  }
  if (
    parsed.protocol !== "https:" ||
    !OFFICIAL_SOURCE_HOSTS.has(parsed.hostname) ||
    parsed.username ||
    parsed.password ||
    parsed.port ||
    parsed.search ||
    !/^\/doc-[0-9]+$/.test(parsed.pathname)
  ) {
    errors.push(`${keyPath}: only HTTPS Qiwe official documentation URLs are allowed`);
  }
}

function hasOnlyKeys(value, allowedKeys, keyPath, errors) {
  let valid = true;
  for (const key of Object.keys(value)) {
    if (!allowedKeys.has(key)) {
      errors.push(`${keyPath}.${key}: unknown field`);
      valid = false;
    }
  }
  return valid;
}

function isJsonScalar(value) {
  return (
    value === null ||
    typeof value === "string" ||
    typeof value === "number" ||
    typeof value === "boolean"
  );
}

function validateJsonPointer(value, keyPath, errors) {
  if (
    typeof value !== "string" ||
    value.length === 0 ||
    value.length > 256 ||
    !value.startsWith("/") ||
    value.split("/").length > 33 ||
    /~(?:[^01]|$)/.test(value)
  ) {
    errors.push(`${keyPath}: must be a bounded JSON Pointer`);
    return false;
  }
  return true;
}

function validateJsonTree(value, errors) {
  const stack = [{ value, depth: 0, keyPath: "$", parentKey: "" }];
  let nodeCount = 0;
  while (stack.length > 0) {
    const current = stack.pop();
    nodeCount += 1;
    if (nodeCount > MAX_JSON_NODES) {
      errors.push(`JSON exceeds ${MAX_JSON_NODES} nodes`);
      return;
    }
    if (current.depth > MAX_JSON_DEPTH) {
      errors.push(`JSON exceeds depth ${MAX_JSON_DEPTH}`);
      return;
    }

    if (typeof current.value === "string") {
      if (Buffer.byteLength(current.value, "utf8") > MAX_JSON_STRING_BYTES) {
        errors.push(
          `${current.keyPath}: string exceeds ${MAX_JSON_STRING_BYTES} bytes`
        );
      }
      const urls = current.value.match(/https?:\/\/[^\s)\]}>"']+/g) ?? [];
      for (const url of urls) {
        if (
          !new Set([
            "official_source",
            "official_sources",
            "official_source_url",
            "official_source_urls",
            "official_documentation_url",
            "official_documentation_urls",
          ]).has(current.parentKey)
        ) {
          errors.push(
            `${current.keyPath}: URLs are allowed only in official source fields`
          );
        } else {
          validateOfficialUrl(url, current.keyPath, errors);
        }
      }
      continue;
    }
    if (typeof current.value === "number") {
      if (!Number.isFinite(current.value)) {
        errors.push(`${current.keyPath}: number must be finite`);
      } else if (
        Number.isInteger(current.value) &&
        !Number.isSafeInteger(current.value)
      ) {
        errors.push(`${current.keyPath}: unsafe integer must be encoded as a string`);
      }
      continue;
    }
    if (current.value === null || typeof current.value === "boolean") {
      continue;
    }
    if (Array.isArray(current.value)) {
      for (let index = current.value.length - 1; index >= 0; index -= 1) {
        stack.push({
          value: current.value[index],
          depth: current.depth + 1,
          keyPath: `${current.keyPath}[${index}]`,
          parentKey: current.parentKey,
        });
      }
      continue;
    }
    if (typeof current.value !== "object") {
      errors.push(`${current.keyPath}: unsupported JSON value`);
      continue;
    }

    for (const [key, child] of Object.entries(current.value)) {
      if (Buffer.byteLength(key, "utf8") > MAX_JSON_KEY_BYTES) {
        errors.push(`${current.keyPath}: JSON key exceeds ${MAX_JSON_KEY_BYTES} bytes`);
      }
      const normalized = normalizeKey(key);
      const parts = normalized.split("_").filter(Boolean);
      if (
        key === "__proto__" ||
        normalized === "proto" ||
        normalized === "prototype" ||
        normalized === "constructor" ||
        parts.some((part) => FORBIDDEN_KEY_PARTS.has(part))
      ) {
        errors.push(
          `${current.keyPath}.${key}: forbidden executable or privileged field`
        );
      }
      stack.push({
        value: child,
        depth: current.depth + 1,
        keyPath: `${current.keyPath}.${key}`,
        parentKey: normalized,
      });
    }
  }
}

function rejectDuplicateJsonKeys(text) {
  let index = 0;
  const skipWhitespace = () => {
    while (/[\t\n\r ]/.test(text[index] ?? "")) index += 1;
  };
  const parseString = () => {
    const start = index;
    if (text[index] !== '"') throw new Error("JSON string expected");
    index += 1;
    while (index < text.length) {
      if (text[index] === "\\") {
        index += 2;
      } else if (text[index] === '"') {
        index += 1;
        return JSON.parse(text.slice(start, index));
      } else {
        index += 1;
      }
    }
    throw new Error("unterminated JSON string");
  };
  const parseValue = () => {
    skipWhitespace();
    if (text[index] === "{") {
      index += 1;
      skipWhitespace();
      const keys = new Set();
      if (text[index] === "}") {
        index += 1;
        return;
      }
      while (index < text.length) {
        const key = parseString();
        if (keys.has(key)) throw new Error("duplicate JSON object key");
        keys.add(key);
        skipWhitespace();
        if (text[index] !== ":") throw new Error("JSON object colon expected");
        index += 1;
        parseValue();
        skipWhitespace();
        if (text[index] === "}") {
          index += 1;
          return;
        }
        if (text[index] !== ",") throw new Error("JSON object comma expected");
        index += 1;
        skipWhitespace();
      }
      throw new Error("unterminated JSON object");
    }
    if (text[index] === "[") {
      index += 1;
      skipWhitespace();
      if (text[index] === "]") {
        index += 1;
        return;
      }
      while (index < text.length) {
        parseValue();
        skipWhitespace();
        if (text[index] === "]") {
          index += 1;
          return;
        }
        if (text[index] !== ",") throw new Error("JSON array comma expected");
        index += 1;
      }
      throw new Error("unterminated JSON array");
    }
    if (text[index] === '"') {
      parseString();
      return;
    }
    const start = index;
    while (index < text.length && !/[\t\n\r ,\]}]/.test(text[index])) {
      index += 1;
    }
    if (index === start) throw new Error("JSON scalar expected");
  };

  parseValue();
  skipWhitespace();
  if (index !== text.length) throw new Error("unexpected trailing JSON input");
}

function parseStrictJson(text, relativePath, errors) {
  let value;
  try {
    value = JSON.parse(text);
  } catch (error) {
    errors.push(`${relativePath}: invalid JSON: ${error.message}`);
    return null;
  }
  try {
    rejectDuplicateJsonKeys(text);
  } catch {
    errors.push(`${relativePath}: duplicate or invalid JSON keys are forbidden`);
    return null;
  }
  validateJsonTree(value, errors);
  return value;
}

function validatePredicate(value, keyPath, errors, depth, counter) {
  if (!value || Array.isArray(value) || typeof value !== "object") {
    errors.push(`${keyPath}: selector must be an object`);
    return;
  }
  counter.count += 1;
  if (depth > MAX_MAPPING_SELECTOR_DEPTH) {
    errors.push(`${keyPath}: selector nesting exceeds the bounded DSL`);
    return;
  }
  if (counter.count > MAX_MAPPING_PREDICATES) {
    errors.push(`${keyPath}: selector count exceeds the bounded DSL`);
    return;
  }

  if (value.op === "all" || value.op === "any") {
    hasOnlyKeys(value, new Set(["op", "rules"]), keyPath, errors);
    if (
      !Array.isArray(value.rules) ||
      value.rules.length === 0 ||
      value.rules.length > MAX_MAPPING_PREDICATES
    ) {
      errors.push(`${keyPath}.rules: boolean selectors require bounded rules`);
      return;
    }
    value.rules.forEach((rule, index) =>
      validatePredicate(rule, `${keyPath}.rules[${index}]`, errors, depth + 1, counter)
    );
    return;
  }
  if (typeof value.op !== "string" || !ALLOWED_LEAF_PREDICATES.has(value.op)) {
    errors.push(`${keyPath}.op: predicate is not in the bounded DSL`);
    return;
  }

  const allowedKeys =
    value.op === "in"
      ? new Set(["op", "pointer", "values"])
      : new Set(["op", "pointer", "value"]);
  hasOnlyKeys(value, allowedKeys, keyPath, errors);
  validateJsonPointer(value.pointer, `${keyPath}.pointer`, errors);
  const hasValue = Object.hasOwn(value, "value");
  const hasValues = Object.hasOwn(value, "values");
  if (value.op === "equals") {
    if (!hasValue || hasValues || !isJsonScalar(value.value)) {
      errors.push(`${keyPath}: equals requires one scalar value`);
    }
  } else if (value.op === "in") {
    if (
      hasValue ||
      !Array.isArray(value.values) ||
      value.values.length === 0 ||
      value.values.length > MAX_MAPPING_PREDICATES ||
      value.values.some((candidate) => !isJsonScalar(candidate))
    ) {
      errors.push(`${keyPath}: in requires bounded scalar values`);
    }
  } else if (value.op === "exists") {
    if (hasValues || (hasValue && typeof value.value !== "boolean")) {
      errors.push(`${keyPath}: exists accepts only an optional boolean value`);
    }
  } else if (
    value.op === "type_is" &&
    (!hasValue ||
      hasValues ||
      !new Set(["array", "boolean", "null", "number", "object", "string"]).has(
        value.value
      ))
  ) {
    errors.push(`${keyPath}: type_is requires one supported JSON type`);
  }
}

function validateTransform(value, keyPath, errors, primitiveRefs) {
  if (!value || Array.isArray(value) || typeof value !== "object") {
    errors.push(`${keyPath}: transform must be an object`);
    return;
  }
  if (typeof value.op !== "string" || !ALLOWED_TRANSFORMS.has(value.op)) {
    errors.push(`${keyPath}.op: transform is not in the bounded DSL`);
    return;
  }
  if (value.op === "split") {
    hasOnlyKeys(value, new Set(["op", "delimiter", "max_parts"]), keyPath, errors);
    if (
      typeof value.delimiter !== "string" ||
      value.delimiter.length === 0 ||
      value.delimiter.length > 8 ||
      !/^[\x20-\x7e]+$/.test(value.delimiter)
    ) {
      errors.push(`${keyPath}.delimiter: split delimiter is outside bounded limits`);
    }
    if (
      !Number.isInteger(value.max_parts) ||
      value.max_parts < 1 ||
      value.max_parts > MAX_MAPPING_RECORDS
    ) {
      errors.push(`${keyPath}.max_parts: split limit is outside bounded limits`);
    }
  } else if (value.op === "unix_timestamp") {
    hasOnlyKeys(value, new Set(["op", "milliseconds"]), keyPath, errors);
    if (
      Object.hasOwn(value, "milliseconds") &&
      typeof value.milliseconds !== "boolean"
    ) {
      errors.push(`${keyPath}.milliseconds: must be boolean`);
    }
  } else if (value.op === "restricted_primitive") {
    hasOnlyKeys(value, new Set(["op", "primitive_ref"]), keyPath, errors);
    if (!isPathOfKind(value.primitive_ref, "primitive")) {
      errors.push(
        `${keyPath}.primitive_ref: must name an immutable restricted primitive`
      );
    } else {
      primitiveRefs.add(value.primitive_ref);
    }
  } else {
    hasOnlyKeys(value, new Set(["op"]), keyPath, errors);
  }
}

function validateExtractField(
  value,
  outputField,
  keyPath,
  errors,
  primitiveRefs,
  primitiveUses
) {
  if (!value || Array.isArray(value) || typeof value !== "object") {
    errors.push(`${keyPath}: extractor must be an object`);
    return;
  }
  hasOnlyKeys(value, new Set(["pointer", "transforms"]), keyPath, errors);
  validateJsonPointer(value.pointer, `${keyPath}.pointer`, errors);
  if (
    !Array.isArray(value.transforms) ||
    value.transforms.length > MAX_MAPPING_TRANSFORMS
  ) {
    errors.push(`${keyPath}.transforms: transforms must use only the bounded DSL`);
  } else {
    value.transforms.forEach((transform, index) =>
      validateTransform(
        transform,
        `${keyPath}.transforms[${index}]`,
        errors,
        primitiveRefs
      )
    );
    const restricted = value.transforms.filter(
      (transform) => transform?.op === "restricted_primitive"
    );
    if (restricted.length > 1) {
      errors.push(`${keyPath}: may invoke at most one restricted primitive`);
    } else if (restricted.length === 1) {
      primitiveUses.push({
        primitiveRef: restricted[0].primitive_ref,
        transformCount: value.transforms.length,
      });
    }
  }
  if (
    ["event_id", "space_chat_id", "subject_user_ids"].includes(outputField) &&
    !value.transforms?.some((transform) => transform?.op === "opaque_id")
  ) {
    errors.push(`${keyPath}: opaque identifiers must use opaque_id`);
  }
}

function validateMapping(value, relativePath, errors) {
  if (!value || Array.isArray(value) || typeof value !== "object") {
    errors.push(`${relativePath}: mapping must be a JSON object`);
    return null;
  }
  hasOnlyKeys(value, MAPPING_TOP_LEVEL_FIELDS, relativePath, errors);
  if (value.schema_version !== 1) {
    errors.push(`${relativePath}: mapping schema_version must be 1`);
  }
  if (value.provider !== "qiwe") {
    errors.push(`${relativePath}: mapping provider must be qiwe`);
  }
  const primitiveRefs = new Set();
  const primitiveUses = [];
  if (
    typeof value.definition_key !== "string" ||
    !SAFE_IDENTIFIER.test(value.definition_key)
  ) {
    errors.push(
      `${relativePath}: definition_key must be a bounded lowercase identifier`
    );
  }
  if (
    !Array.isArray(value.official_sources) ||
    value.official_sources.length === 0 ||
    value.official_sources.length > 8 ||
    value.official_sources.some((source) => typeof source !== "string")
  ) {
    errors.push(`${relativePath}: official_sources must contain one to eight URLs`);
  } else {
    if (new Set(value.official_sources).size !== value.official_sources.length) {
      errors.push(`${relativePath}: official_sources must be unique`);
    }
    value.official_sources.forEach((source, index) =>
      validateOfficialUrl(source, `${relativePath}.official_sources[${index}]`, errors)
    );
  }
  if (
    !value.selector ||
    Array.isArray(value.selector) ||
    typeof value.selector !== "object"
  ) {
    errors.push(`${relativePath}: selector must be an object`);
  } else {
    validatePredicate(value.selector, `${relativePath}.selector`, errors, 0, {
      count: 0,
    });
  }
  if (
    !value.extractor ||
    Array.isArray(value.extractor) ||
    typeof value.extractor !== "object"
  ) {
    errors.push(`${relativePath}: extractor must be an object`);
  } else {
    hasOnlyKeys(
      value.extractor,
      new Set(["event_type", ...CANONICAL_EXTRACTOR_FIELDS]),
      `${relativePath}.extractor`,
      errors
    );
    if (
      typeof value.extractor.event_type !== "string" ||
      !SAFE_IDENTIFIER.test(value.extractor.event_type)
    ) {
      errors.push(
        `${relativePath}.extractor.event_type: must be a bounded lowercase identifier`
      );
    }
    for (const outputField of CANONICAL_EXTRACTOR_FIELDS) {
      if (!Object.hasOwn(value.extractor, outputField)) {
        errors.push(
          `${relativePath}.extractor: missing canonical field ${outputField}`
        );
      }
    }
    for (const [outputField, extractor] of Object.entries(value.extractor)) {
      if (CANONICAL_EXTRACTOR_FIELDS.has(outputField)) {
        validateExtractField(
          extractor,
          outputField,
          `${relativePath}.extractor.${outputField}`,
          errors,
          primitiveRefs,
          primitiveUses
        );
      }
    }
    const spaceExtractor = value.extractor.space_chat_id;
    if (
      value.provider === "qiwe" &&
      (spaceExtractor?.pointer !== "/fromRoomId" ||
        !Array.isArray(spaceExtractor?.transforms) ||
        spaceExtractor.transforms.length !== 1 ||
        spaceExtractor.transforms[0]?.op !== "opaque_id")
    ) {
      errors.push(
        `${relativePath}.extractor.space_chat_id: QiWe Space routing must bind exactly to /fromRoomId`
      );
    }
  }
  return {
    definitionKey: value.definition_key,
    eventType: value.extractor?.event_type,
    primitiveRefs: [...primitiveRefs].sort(),
    primitiveUses,
  };
}

function validateRestrictedPrimitiveOperation(value, keyPath, errors) {
  if (!value || Array.isArray(value) || typeof value !== "object") {
    errors.push(`${keyPath}: restricted primitive operation must be an object`);
    return;
  }
  if (
    typeof value.op !== "string" ||
    !ALLOWED_RESTRICTED_PRIMITIVE_OPERATIONS.has(value.op)
  ) {
    errors.push(`${keyPath}.op: operation is outside the fixed parser kernel`);
    return;
  }
  if (
    ["array_flatten", "base64_utf8", "json_parse", "string_trim"].includes(value.op)
  ) {
    hasOnlyKeys(value, new Set(["op"]), keyPath, errors);
    return;
  }
  if (value.op === "json_pointer") {
    hasOnlyKeys(value, new Set(["op", "pointer"]), keyPath, errors);
    validateJsonPointer(value.pointer, `${keyPath}.pointer`, errors);
    return;
  }
  hasOnlyKeys(value, new Set(["op", "delimiter", "max_parts"]), keyPath, errors);
  if (
    typeof value.delimiter !== "string" ||
    value.delimiter.length === 0 ||
    value.delimiter.length > 8 ||
    !/^[\x20-\x7e]+$/.test(value.delimiter)
  ) {
    errors.push(`${keyPath}.delimiter: split delimiter is outside bounded limits`);
  }
  if (
    !Number.isInteger(value.max_parts) ||
    value.max_parts < 1 ||
    value.max_parts > MAX_MAPPING_RECORDS
  ) {
    errors.push(`${keyPath}.max_parts: split limit is outside bounded limits`);
  }
}

function validatePrimitive(value, relativePath, errors) {
  if (!value || Array.isArray(value) || typeof value !== "object") {
    errors.push(`${relativePath}: restricted primitive must be a JSON object`);
    return null;
  }
  hasOnlyKeys(
    value,
    new Set([
      "schema_version",
      "provider",
      "definition_key",
      "operations",
      "official_sources",
    ]),
    relativePath,
    errors
  );
  if (value.schema_version !== 1) {
    errors.push(`${relativePath}: primitive schema_version must be 1`);
  }
  if (value.provider !== "qiwe") {
    errors.push(`${relativePath}: primitive provider must be qiwe`);
  }
  if (
    typeof value.definition_key !== "string" ||
    !SAFE_IDENTIFIER.test(value.definition_key)
  ) {
    errors.push(
      `${relativePath}: primitive definition_key must be a bounded lowercase identifier`
    );
  }
  if (
    !Array.isArray(value.operations) ||
    value.operations.length === 0 ||
    value.operations.length > MAX_RESTRICTED_PRIMITIVE_OPERATIONS
  ) {
    errors.push(`${relativePath}: primitive operations are outside bounded limits`);
  } else {
    value.operations.forEach((operation, index) =>
      validateRestrictedPrimitiveOperation(
        operation,
        `${relativePath}.operations[${index}]`,
        errors
      )
    );
  }
  if (
    !Array.isArray(value.official_sources) ||
    value.official_sources.length === 0 ||
    value.official_sources.length > 8 ||
    value.official_sources.some((source) => typeof source !== "string")
  ) {
    errors.push(
      `${relativePath}: primitive official_sources must contain one to eight URLs`
    );
  } else {
    if (new Set(value.official_sources).size !== value.official_sources.length) {
      errors.push(`${relativePath}: primitive official_sources must be unique`);
    }
    value.official_sources.forEach((source, index) =>
      validateOfficialUrl(source, `${relativePath}.official_sources[${index}]`, errors)
    );
  }
  return {
    definitionKey: value.definition_key,
    operationCount: Array.isArray(value.operations) ? value.operations.length : 0,
  };
}

function validateOpaqueIdFields(value, keyPath, errors) {
  const stack = [{ value, keyPath }];
  while (stack.length > 0) {
    const current = stack.pop();
    if (Array.isArray(current.value)) {
      current.value.forEach((child, index) =>
        stack.push({ value: child, keyPath: `${current.keyPath}[${index}]` })
      );
      continue;
    }
    if (!current.value || typeof current.value !== "object") {
      continue;
    }
    for (const [key, child] of Object.entries(current.value)) {
      const normalized = normalizeKey(key);
      if (/(?:^|_)(?:id|identifier)$/.test(normalized) && typeof child !== "string") {
        errors.push(`${current.keyPath}.${key}: opaque identifier must be a string`);
      }
      stack.push({ value: child, keyPath: `${current.keyPath}.${key}` });
    }
  }
}

function isPathOfKind(relativePath, expectedKind) {
  return (
    validateRepositoryPath(relativePath) &&
    classifyPath(relativePath)?.kind === expectedKind
  );
}

function validateFixture(value, relativePath, errors) {
  if (!value || Array.isArray(value) || typeof value !== "object") {
    errors.push(`${relativePath}: fixture must be a JSON object`);
    return null;
  }
  hasOnlyKeys(value, new Set(["fixture_metadata", "event"]), relativePath, errors);
  const metadata = value.fixture_metadata;
  if (!metadata || Array.isArray(metadata) || typeof metadata !== "object") {
    errors.push(`${relativePath}: fixture_metadata must be an object`);
    return null;
  }
  hasOnlyKeys(
    metadata,
    new Set(["sanitized", "synthetic", "mapping_ref"]),
    `${relativePath}.fixture_metadata`,
    errors
  );
  if (metadata.sanitized !== true || metadata.synthetic !== true) {
    errors.push(
      `${relativePath}: fixture_metadata must assert sanitized=true and synthetic=true`
    );
  }
  if (!isPathOfKind(metadata.mapping_ref, "mapping")) {
    errors.push(
      `${relativePath}: fixture_metadata.mapping_ref must name an allowlisted mapping`
    );
  }
  if (!value.event || Array.isArray(value.event) || typeof value.event !== "object") {
    errors.push(`${relativePath}: event must be a JSON object`);
  } else {
    if (
      !Array.isArray(value.event.data) ||
      value.event.data.length === 0 ||
      value.event.data.length > MAX_MAPPING_RECORDS
    ) {
      errors.push(
        `${relativePath}: event.data must contain one to ${MAX_MAPPING_RECORDS} records`
      );
    }
    validateOpaqueIdFields(value.event, `${relativePath}.event`, errors);
  }
  return {
    mappingRef: metadata.mapping_ref,
    recordCount: Array.isArray(value.event?.data) ? value.event.data.length : 0,
  };
}

function validateCanonicalEvent(value, keyPath, errors) {
  if (!value || Array.isArray(value) || typeof value !== "object") {
    errors.push(`${keyPath}: canonical event must be an object`);
    return;
  }
  hasOnlyKeys(value, CANONICAL_EVENT_FIELDS, keyPath, errors);
  for (const requiredField of CANONICAL_EVENT_FIELDS) {
    if (!Object.hasOwn(value, requiredField)) {
      errors.push(`${keyPath}: missing canonical field ${requiredField}`);
    }
  }
  if (typeof value.event_type !== "string" || !SAFE_IDENTIFIER.test(value.event_type)) {
    errors.push(`${keyPath}.event_type: must be a bounded lowercase identifier`);
  }
  for (const idField of ["event_id", "space_id"]) {
    if (
      typeof value[idField] !== "string" ||
      value[idField].length === 0 ||
      value[idField].length > 256 ||
      /[\u0000-\u001f\u007f]/.test(value[idField])
    ) {
      errors.push(`${keyPath}.${idField}: must be a bounded opaque string`);
    }
  }
  if (
    !Array.isArray(value.subject_user_ids) ||
    value.subject_user_ids.length === 0 ||
    value.subject_user_ids.length > 64 ||
    value.subject_user_ids.some(
      (subject) =>
        typeof subject !== "string" ||
        subject.length === 0 ||
        subject.length > 256 ||
        /[\u0000-\u001f\u007f]/.test(subject)
    )
  ) {
    errors.push(`${keyPath}.subject_user_ids: must contain bounded opaque strings`);
  } else if (new Set(value.subject_user_ids).size !== value.subject_user_ids.length) {
    errors.push(`${keyPath}.subject_user_ids: duplicate identifiers are forbidden`);
  }
  if (
    typeof value.occurred_at !== "string" ||
    value.occurred_at.length === 0 ||
    value.occurred_at.length > 128 ||
    /[\u0000-\u001f\u007f]/.test(value.occurred_at)
  ) {
    errors.push(`${keyPath}.occurred_at: must be a bounded string`);
  }
}

function validateExpectation(value, relativePath, errors) {
  if (!value || Array.isArray(value) || typeof value !== "object") {
    errors.push(`${relativePath}: expectation must be a JSON object`);
    return null;
  }
  hasOnlyKeys(value, new Set(["expectation_metadata", "events"]), relativePath, errors);
  const metadata = value.expectation_metadata;
  if (!metadata || Array.isArray(metadata) || typeof metadata !== "object") {
    errors.push(`${relativePath}: expectation_metadata must be an object`);
    return null;
  }
  hasOnlyKeys(
    metadata,
    new Set(["sanitized", "synthetic", "mapping_ref", "fixture_ref"]),
    `${relativePath}.expectation_metadata`,
    errors
  );
  if (metadata.sanitized !== true || metadata.synthetic !== true) {
    errors.push(
      `${relativePath}: expectation_metadata must assert sanitized=true and synthetic=true`
    );
  }
  if (!isPathOfKind(metadata.mapping_ref, "mapping")) {
    errors.push(
      `${relativePath}: expectation_metadata.mapping_ref must name an allowlisted mapping`
    );
  }
  if (!isPathOfKind(metadata.fixture_ref, "fixture")) {
    errors.push(
      `${relativePath}: expectation_metadata.fixture_ref must name an allowlisted fixture`
    );
  }
  if (
    !Array.isArray(value.events) ||
    value.events.length === 0 ||
    value.events.length > MAX_MAPPING_RECORDS
  ) {
    errors.push(
      `${relativePath}: events must contain one to ${MAX_MAPPING_RECORDS} canonical events`
    );
  } else {
    value.events.forEach((event, index) =>
      validateCanonicalEvent(event, `${relativePath}.events[${index}]`, errors)
    );
  }
  return {
    mappingRef: metadata.mapping_ref,
    fixtureRef: metadata.fixture_ref,
    eventCount: Array.isArray(value.events) ? value.events.length : 0,
    eventTypes: Array.isArray(value.events)
      ? value.events.map((event) => event?.event_type)
      : [],
  };
}

function validateDocumentation(text, relativePath, errors) {
  if (
    !text.endsWith("\n") ||
    text.includes("\r") ||
    /[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/.test(text)
  ) {
    errors.push(`${relativePath}: documentation must be bounded UTF-8 text`);
    return null;
  }
  const match = text.match(
    /^# QiWe event mapping `([a-z0-9][a-z0-9._:-]{0,127})`\n\n- Mapping: `([^`]+\.mapping\.json)`\n- Fixture: `([^`]+\.fixture\.json)`\n- Expectation: `([^`]+\.expected\.json)`\n- Scope: declarative event interpretation only\n$/
  );
  if (!match) {
    errors.push(`${relativePath}: documentation must use the fixed mapping summary`);
    return null;
  }
  const [, definitionKey, mappingRef, fixtureRef, expectationRef] = match;
  if (
    !validateRepositoryPath(mappingRef) ||
    !validateRepositoryPath(fixtureRef) ||
    !validateRepositoryPath(expectationRef) ||
    !isPathOfKind(mappingRef, "mapping") ||
    !isPathOfKind(fixtureRef, "fixture") ||
    !isPathOfKind(expectationRef, "expectation")
  ) {
    errors.push(`${relativePath}: documentation references must stay in the bundle`);
    return null;
  }
  return { definitionKey, mappingRef, fixtureRef, expectationRef };
}

export function classifyLowRiskChange({ repoRoot, baseRef, headRef }) {
  validateRef("baseRef", baseRef);
  validateRef("headRef", headRef);
  const resolvedRoot = path.resolve(repoRoot);
  const baseSha = resolveCommit(resolvedRoot, baseRef);
  const headSha = resolveCommit(resolvedRoot, headRef);
  const reasons = [];
  const files = [];
  const bundleReferences = new Map();

  if (!isAncestor(resolvedRoot, baseSha, headSha)) {
    reasons.push("base_not_ancestor_of_head");
  }

  const commitCount = Number(
    String(
      runGit(resolvedRoot, ["rev-list", "--count", `${baseSha}..${headSha}`])
    ).trim()
  );
  if (commitCount !== 1) {
    reasons.push(`commit_count_must_be_one:${commitCount}`);
  }

  const entries = changedEntries(resolvedRoot, baseSha, headSha);
  if (entries.length === 0) {
    reasons.push("no_changed_files");
  }
  if (entries.length > MAX_CHANGED_FILES) {
    reasons.push(`changed_file_limit_exceeded:${entries.length}`);
  }

  for (const entry of entries.slice(0, MAX_CHANGED_FILES + 1)) {
    if (!validateRepositoryPath(entry.path)) {
      reasons.push(`${entry.path}:unsafe_path`);
      continue;
    }
    const rule = classifyPath(entry.path);
    if (!rule) {
      reasons.push(`${entry.path}:path_not_allowlisted`);
      continue;
    }
    if (!rule.allowedStatuses.has(entry.status)) {
      reasons.push(`${entry.path}:status_${entry.status}_not_append_only`);
      continue;
    }

    let blob;
    try {
      blob = readHeadBlob(resolvedRoot, headSha, entry.path);
    } catch (error) {
      reasons.push(`${entry.path}:blob_unavailable:${error.message}`);
      continue;
    }
    if (blob.objectType !== "blob" || blob.mode !== "100644") {
      reasons.push(`${entry.path}:must_be_non_executable_regular_file`);
      continue;
    }
    if (blob.size > rule.maxBytes) {
      reasons.push(`${entry.path}:size_limit_exceeded:${blob.size}`);
      continue;
    }
    if (blob.content.includes(0)) {
      reasons.push(`${entry.path}:nul_byte_forbidden`);
      continue;
    }
    const text = blob.content.toString("utf8");
    if (!Buffer.from(text, "utf8").equals(blob.content)) {
      reasons.push(`${entry.path}:invalid_utf8`);
      continue;
    }

    const contentErrors = [];
    let references = null;
    if (rule.kind === "documentation") {
      references = validateDocumentation(text, entry.path, contentErrors);
    } else {
      const value = parseStrictJson(text, entry.path, contentErrors);
      if (value !== null) {
        if (rule.kind === "mapping") {
          references = validateMapping(value, entry.path, contentErrors);
        } else if (rule.kind === "fixture") {
          references = validateFixture(value, entry.path, contentErrors);
        } else if (rule.kind === "expectation") {
          references = validateExpectation(value, entry.path, contentErrors);
        } else if (rule.kind === "primitive") {
          references = validatePrimitive(value, entry.path, contentErrors);
        }
      }
    }
    reasons.push(...contentErrors);
    if (references) {
      bundleReferences.set(entry.path, references);
    }
    files.push({
      path: entry.path,
      status: entry.status,
      kind: rule.kind,
      mode: blob.mode,
      bytes: blob.size,
      git_object: blob.objectId,
      sha256: createHash("sha256").update(blob.content).digest("hex"),
    });
  }

  if (files.length > 0) {
    for (const requiredKind of ["mapping", "fixture", "expectation"]) {
      if (!files.some((file) => file.kind === requiredKind)) {
        reasons.push(`missing_required_${requiredKind}`);
      }
    }

    const mappingPaths = new Set(
      files.filter((file) => file.kind === "mapping").map((file) => file.path)
    );
    const fixturePaths = new Set(
      files.filter((file) => file.kind === "fixture").map((file) => file.path)
    );
    const expectationPaths = new Set(
      files.filter((file) => file.kind === "expectation").map((file) => file.path)
    );
    const primitivePaths = new Set(
      files.filter((file) => file.kind === "primitive").map((file) => file.path)
    );
    const documentationPaths = new Set(
      files.filter((file) => file.kind === "documentation").map((file) => file.path)
    );
    if (mappingPaths.size !== 1) {
      reasons.push(`mapping_file_count_must_be_one:${mappingPaths.size}`);
    }
    if (fixturePaths.size !== 1) {
      reasons.push(`fixture_file_count_must_be_one:${fixturePaths.size}`);
    }
    if (expectationPaths.size !== 1) {
      reasons.push(`expectation_file_count_must_be_one:${expectationPaths.size}`);
    }
    if (primitivePaths.size > 1) {
      reasons.push("restricted_primitive_file_limit_exceeded");
    }
    if (documentationPaths.size > 1) {
      reasons.push("documentation_file_limit_exceeded");
    }
    const fixtureCountByMapping = new Map();
    const expectationCountByFixture = new Map();
    const primitiveUseCount = new Map();

    for (const mappingPath of mappingPaths) {
      const mapping = bundleReferences.get(mappingPath);
      for (const use of mapping?.primitiveUses ?? []) {
        if (!isPathOfKind(use.primitiveRef, "primitive")) {
          continue;
        }
        let primitive = bundleReferences.get(use.primitiveRef);
        if (!primitivePaths.has(use.primitiveRef)) {
          primitive = loadExistingPrimitive(
            resolvedRoot,
            headSha,
            use.primitiveRef,
            reasons
          );
        }
        if (!primitive) {
          reasons.push(`${mappingPath}:restricted_primitive_ref_not_registered`);
          continue;
        }
        if (
          use.transformCount - 1 + primitive.operationCount >
          MAX_EXPANDED_MAPPING_TRANSFORMS
        ) {
          reasons.push(`${mappingPath}:expanded_transform_limit_exceeded`);
        }
        primitiveUseCount.set(
          use.primitiveRef,
          (primitiveUseCount.get(use.primitiveRef) ?? 0) + 1
        );
      }
    }
    for (const primitivePath of primitivePaths) {
      if (!primitiveUseCount.has(primitivePath)) {
        reasons.push(`${primitivePath}:primitive_not_referenced_by_added_mapping`);
      }
    }

    for (const fixturePath of fixturePaths) {
      const mappingRef = bundleReferences.get(fixturePath)?.mappingRef;
      if (!mappingPaths.has(mappingRef)) {
        reasons.push(`${fixturePath}:mapping_ref_not_added_in_change`);
        continue;
      }
      fixtureCountByMapping.set(
        mappingRef,
        (fixtureCountByMapping.get(mappingRef) ?? 0) + 1
      );
    }

    for (const expectationPath of expectationPaths) {
      const expectation = bundleReferences.get(expectationPath);
      if (!expectation) {
        continue;
      }
      if (!mappingPaths.has(expectation.mappingRef)) {
        reasons.push(`${expectationPath}:mapping_ref_not_added_in_change`);
      }
      if (!fixturePaths.has(expectation.fixtureRef)) {
        reasons.push(`${expectationPath}:fixture_ref_not_added_in_change`);
        continue;
      }
      const fixture = bundleReferences.get(expectation.fixtureRef);
      if (fixture?.mappingRef !== expectation.mappingRef) {
        reasons.push(`${expectationPath}:mapping_ref_does_not_match_fixture`);
      }
      if (fixture && fixture.recordCount <= expectation.eventCount) {
        reasons.push(`${expectation.fixtureRef}:requires_selector_non_match_record`);
      }
      const mapping = bundleReferences.get(expectation.mappingRef);
      if (
        mapping?.eventType &&
        expectation.eventTypes.some((eventType) => eventType !== mapping.eventType)
      ) {
        reasons.push(`${expectationPath}:event_type_does_not_match_mapping`);
      }
      expectationCountByFixture.set(
        expectation.fixtureRef,
        (expectationCountByFixture.get(expectation.fixtureRef) ?? 0) + 1
      );
    }

    for (const documentationPath of documentationPaths) {
      const documentation = bundleReferences.get(documentationPath);
      if (!documentation) {
        continue;
      }
      if (!mappingPaths.has(documentation.mappingRef)) {
        reasons.push(`${documentationPath}:mapping_ref_not_added_in_change`);
      }
      if (!fixturePaths.has(documentation.fixtureRef)) {
        reasons.push(`${documentationPath}:fixture_ref_not_added_in_change`);
      }
      if (!expectationPaths.has(documentation.expectationRef)) {
        reasons.push(`${documentationPath}:expectation_ref_not_added_in_change`);
      }
      const mapping = bundleReferences.get(documentation.mappingRef);
      if (mapping?.definitionKey !== documentation.definitionKey) {
        reasons.push(`${documentationPath}:definition_key_does_not_match_mapping`);
      }
    }

    for (const mappingPath of mappingPaths) {
      if (!fixtureCountByMapping.has(mappingPath)) {
        reasons.push(`${mappingPath}:missing_corresponding_fixture`);
      }
    }
    for (const fixturePath of fixturePaths) {
      const count = expectationCountByFixture.get(fixturePath) ?? 0;
      if (count !== 1) {
        reasons.push(`${fixturePath}:requires_exactly_one_corresponding_expectation`);
      }
    }
  }

  files.sort((left, right) => left.path.localeCompare(right.path));
  reasons.sort();
  return {
    schema_version: 1,
    classifier_version: CLASSIFIER_VERSION,
    eligible: reasons.length === 0,
    base_sha: baseSha,
    head_sha: headSha,
    commit_count: commitCount,
    files,
    reasons,
  };
}

function parseArguments(argv) {
  const parsed = { repoRoot: process.cwd(), baseRef: "", headRef: "" };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--repo-root") {
      parsed.repoRoot = argv[++index] ?? "";
    } else if (argument === "--base-ref") {
      parsed.baseRef = argv[++index] ?? "";
    } else if (argument === "--head-ref") {
      parsed.headRef = argv[++index] ?? "";
    } else if (argument === "--help" || argument === "-h") {
      process.stdout.write(
        "Usage: node tools/ci/classify-low-risk-change.mjs --base-ref <ref> --head-ref <ref> [--repo-root <path>]\n"
      );
      process.exit(0);
    } else {
      throw new Error(`unknown argument: ${argument}`);
    }
  }
  if (!parsed.baseRef || !parsed.headRef) {
    throw new Error("--base-ref and --head-ref are required");
  }
  return parsed;
}

const isMain =
  process.argv[1] &&
  path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url));

if (isMain) {
  try {
    const result = classifyLowRiskChange(parseArguments(process.argv.slice(2)));
    process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
    if (!result.eligible) {
      process.exitCode = 1;
    }
  } catch (error) {
    process.stderr.write(`Low-risk classification failed: ${error.message}\n`);
    process.exitCode = 2;
  }
}
