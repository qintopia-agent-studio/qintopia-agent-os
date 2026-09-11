#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const OFFICIAL_REPOSITORY = "https://github.com/NousResearch/hermes-agent.git";
const MAX_METADATA_BYTES = 8 * 1024 * 1024;
const MAX_FILE_BYTES = 512 * 1024 * 1024;
const MAX_TOTAL_BYTES = 2 * 1024 * 1024 * 1024;
const MAX_ENTRIES = 200000;
const SHA256_PATTERN = /^[0-9a-f]{64}$/;
const COMMIT_PATTERN = /^[0-9a-f]{40}$/;
const TAG_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/;
const PYTHON_ABI_PATTERN = /^cp[0-9]{2,3}$/;
const VERSION_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._+-]{0,127}$/;
const TIMESTAMP_PATTERN =
  /^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(?:\.[0-9]{3})?Z$/;
const ARTIFACT_PATH_PATTERN =
  /^(?:core|runtime)\/[A-Za-z0-9._+@-]+(?:\/[A-Za-z0-9._+@-]+)*$/;
const METADATA_FILES = Object.freeze([
  "artifact-manifest.json",
  "build-receipt.json",
  "update-receipt.json",
  "validation-summary.json",
]);
const TOP_LEVEL_ENTRIES = Object.freeze([
  ...METADATA_FILES,
  "SHA256SUMS",
  "core",
  "runtime",
]);

class VerificationError extends Error {
  constructor(code) {
    super(code);
    this.name = "VerificationError";
  }
}

const fail = (code) => {
  throw new VerificationError(code);
};

const sha256Bytes = (value) => crypto.createHash("sha256").update(value).digest("hex");

const sha256File = (filePath) => sha256Bytes(fs.readFileSync(filePath));

const fileMode = (metadata) => (metadata.mode & 0o7777).toString(8).padStart(4, "0");

const sameOwner = (metadata, owner) =>
  metadata.uid === owner.uid && metadata.gid === owner.gid;

const identityFields = (value) => [
  value.repository,
  value.tag,
  value.commit_sha,
  value.source_archive_sha256,
  value.python_abi,
  value.dependency_lock_sha256,
  value.runtime_binding_sha256,
  value.hermes_cli_version,
];

export const computeHermesCoreArtifactIdentity = (value) =>
  sha256Bytes(`${identityFields(value).join("\n")}\n`);

const isRecord = (value) =>
  value !== null && typeof value === "object" && !Array.isArray(value);

const hasExactKeys = (value, keys) =>
  isRecord(value) &&
  Object.keys(value).length === keys.length &&
  keys.every((key) => Object.hasOwn(value, key));

const matches = (value, pattern) => typeof value === "string" && pattern.test(value);

const validCommonIdentity = (value) =>
  value.repository === OFFICIAL_REPOSITORY &&
  matches(value.tag, TAG_PATTERN) &&
  matches(value.commit_sha, COMMIT_PATTERN) &&
  matches(value.source_archive_sha256, SHA256_PATTERN) &&
  matches(value.artifact_identity_sha256, SHA256_PATTERN);

function validManifestEntry(entry) {
  if (!isRecord(entry) || !matches(entry.path, ARTIFACT_PATH_PATTERN)) {
    return false;
  }
  if (entry.type === "directory") {
    return (
      hasExactKeys(entry, ["path", "type", "mode", "owner"]) &&
      entry.mode === "0555" &&
      entry.owner === "release-owner"
    );
  }
  return (
    entry.type === "file" &&
    hasExactKeys(entry, ["path", "type", "mode", "owner", "size_bytes", "sha256"]) &&
    (entry.mode === "0444" || entry.mode === "0555") &&
    entry.owner === "release-owner" &&
    Number.isInteger(entry.size_bytes) &&
    entry.size_bytes >= 0 &&
    entry.size_bytes <= MAX_FILE_BYTES &&
    matches(entry.sha256, SHA256_PATTERN)
  );
}

function validReceiptPointer(value, expectedPath) {
  return (
    hasExactKeys(value, ["path", "sha256"]) &&
    value.path === expectedPath &&
    matches(value.sha256, SHA256_PATTERN)
  );
}

function validateManifest(value) {
  return (
    hasExactKeys(value, [
      "schema_version",
      "artifact_type",
      "artifact_name",
      "repository",
      "tag",
      "commit_sha",
      "source_archive_sha256",
      "artifact_identity_sha256",
      "python_abi",
      "dependency_lock_sha256",
      "runtime_binding_sha256",
      "hermes_cli_version",
      "runtime",
      "files",
      "receipts",
    ]) &&
    value.schema_version === 1 &&
    value.artifact_type === "hermes-core" &&
    value.artifact_name === `hermes-core-${value.commit_sha}` &&
    validCommonIdentity(value) &&
    matches(value.python_abi, PYTHON_ABI_PATTERN) &&
    matches(value.dependency_lock_sha256, SHA256_PATTERN) &&
    matches(value.runtime_binding_sha256, SHA256_PATTERN) &&
    matches(value.hermes_cli_version, VERSION_PATTERN) &&
    isRecord(value.runtime) &&
    hasExactKeys(value.runtime, [
      "kind",
      "platform",
      "interpreter_path",
      "site_packages_path",
      "launcher_path",
    ]) &&
    value.runtime.kind === "release-local-venv" &&
    value.runtime.platform === "linux-x86_64" &&
    value.runtime.interpreter_path === "runtime/venv/bin/python" &&
    matches(
      value.runtime.site_packages_path,
      /^runtime\/venv\/lib\/python[0-9]+\.[0-9]+\/site-packages$/
    ) &&
    value.runtime.launcher_path === "runtime/hermes-core-launcher.py" &&
    Array.isArray(value.files) &&
    value.files.length >= 1 &&
    value.files.length <= MAX_ENTRIES &&
    value.files.every(validManifestEntry) &&
    hasExactKeys(value.receipts, ["build", "update", "validation"]) &&
    validReceiptPointer(value.receipts.build, "build-receipt.json") &&
    validReceiptPointer(value.receipts.update, "update-receipt.json") &&
    validReceiptPointer(value.receipts.validation, "validation-summary.json")
  );
}

function validateBuildReceipt(value) {
  return (
    hasExactKeys(value, [
      "schema_version",
      "receipt_type",
      "outcome",
      "repository",
      "tag",
      "commit_sha",
      "source_archive_sha256",
      "artifact_identity_sha256",
      "python_abi",
      "dependency_lock_sha256",
      "runtime_binding_sha256",
      "hermes_cli_version",
      "started_at",
      "completed_at",
      "builder",
      "source_checkout",
      "dependency_install",
    ]) &&
    value.schema_version === 1 &&
    value.receipt_type === "hermes-core-build" &&
    value.outcome === "success" &&
    validCommonIdentity(value) &&
    matches(value.python_abi, PYTHON_ABI_PATTERN) &&
    matches(value.dependency_lock_sha256, SHA256_PATTERN) &&
    matches(value.runtime_binding_sha256, SHA256_PATTERN) &&
    matches(value.hermes_cli_version, VERSION_PATTERN) &&
    matches(value.started_at, TIMESTAMP_PATTERN) &&
    matches(value.completed_at, TIMESTAMP_PATTERN) &&
    hasExactKeys(value.builder, ["os", "arch", "image_digest"]) &&
    value.builder.os === "linux" &&
    value.builder.arch === "x86_64" &&
    matches(value.builder.image_digest, /^sha256:[0-9a-f]{64}$/) &&
    hasExactKeys(value.source_checkout, ["clean", "tag_commit_verified"]) &&
    value.source_checkout.clean === true &&
    value.source_checkout.tag_commit_verified === true &&
    hasExactKeys(value.dependency_install, ["locked", "hashes_verified"]) &&
    value.dependency_install.locked === true &&
    value.dependency_install.hashes_verified === true
  );
}

function validateUpdateReceipt(value) {
  return (
    hasExactKeys(value, [
      "schema_version",
      "receipt_type",
      "outcome",
      "strategy",
      "repository",
      "tag",
      "commit_sha",
      "source_archive_sha256",
      "artifact_identity_sha256",
      "previous_version",
      "previous_commit_sha",
      "new_version",
      "started_at",
      "completed_at",
      "upstream_receipt_sha256",
    ]) &&
    value.schema_version === 1 &&
    value.receipt_type === "hermes-core-update" &&
    value.outcome === "success" &&
    value.strategy === "hermes-update-clean-candidate" &&
    validCommonIdentity(value) &&
    matches(value.previous_version, VERSION_PATTERN) &&
    (value.previous_commit_sha === null ||
      matches(value.previous_commit_sha, COMMIT_PATTERN)) &&
    matches(value.new_version, VERSION_PATTERN) &&
    matches(value.started_at, TIMESTAMP_PATTERN) &&
    matches(value.completed_at, TIMESTAMP_PATTERN) &&
    matches(value.upstream_receipt_sha256, SHA256_PATTERN)
  );
}

function validateValidationSummary(value) {
  return (
    hasExactKeys(value, [
      "schema_version",
      "summary_type",
      "outcome",
      "artifact_identity_sha256",
      "runtime_binding_sha256",
      "profile_contract",
      "checks",
      "sensitive_values_included",
    ]) &&
    value.schema_version === 1 &&
    value.summary_type === "hermes-core-validation" &&
    value.outcome === "success" &&
    matches(value.artifact_identity_sha256, SHA256_PATTERN) &&
    matches(value.runtime_binding_sha256, SHA256_PATTERN) &&
    hasExactKeys(value.profile_contract, [
      "profile_count",
      "wecom_enabled_count",
      "wecom_disabled_count",
      "qiwe_preserved_count",
    ]) &&
    value.profile_contract.profile_count === 7 &&
    value.profile_contract.wecom_enabled_count === 5 &&
    value.profile_contract.wecom_disabled_count === 2 &&
    value.profile_contract.qiwe_preserved_count === 1 &&
    hasExactKeys(value.checks, [
      "schema_validation",
      "source_integrity",
      "dependency_lock",
      "hermes_cli_smoke",
    ]) &&
    value.checks.schema_validation === "passed" &&
    value.checks.source_integrity === "passed" &&
    value.checks.dependency_lock === "passed" &&
    value.checks.hermes_cli_smoke === "passed" &&
    value.sensitive_values_included === false
  );
}

function requireDirectory(directoryPath, owner, expectedMode, code) {
  let metadata;
  try {
    metadata = fs.lstatSync(directoryPath);
  } catch {
    fail(code);
  }
  if (metadata.isSymbolicLink() || !metadata.isDirectory()) {
    fail(code);
  }
  if (owner && !sameOwner(metadata, owner)) {
    fail("artifact_owner_invalid");
  }
  if (expectedMode && fileMode(metadata) !== expectedMode) {
    fail("artifact_mode_invalid");
  }
  return metadata;
}

function requireRegularFile(filePath, owner, expectedMode, sizeLimit) {
  let metadata;
  try {
    metadata = fs.lstatSync(filePath);
  } catch {
    fail("artifact_file_missing");
  }
  if (metadata.isSymbolicLink() || !metadata.isFile()) {
    fail("artifact_entry_not_regular");
  }
  if (metadata.nlink !== 1) {
    fail("artifact_hardlink_rejected");
  }
  if (!sameOwner(metadata, owner)) {
    fail("artifact_owner_invalid");
  }
  if (expectedMode && fileMode(metadata) !== expectedMode) {
    fail("artifact_mode_invalid");
  }
  if (metadata.size > sizeLimit) {
    fail("artifact_size_invalid");
  }
  return metadata;
}

function readBoundedJson(root, name, owner) {
  const filePath = path.join(root, name);
  const metadata = requireRegularFile(filePath, owner, "0444", MAX_METADATA_BYTES);
  if (metadata.size <= 0) {
    fail("artifact_metadata_invalid");
  }
  try {
    return JSON.parse(fs.readFileSync(filePath, "utf8"));
  } catch {
    fail("artifact_metadata_invalid");
  }
}

function validateRelativePath(relativePath) {
  if (
    typeof relativePath !== "string" ||
    relativePath.length === 0 ||
    relativePath.length > 1024 ||
    relativePath.includes("\\") ||
    relativePath.includes("\0") ||
    path.posix.isAbsolute(relativePath)
  ) {
    fail("artifact_path_invalid");
  }
  const segments = relativePath.split("/");
  if (
    segments.some((segment) => segment === "" || segment === "." || segment === "..")
  ) {
    fail("artifact_path_invalid");
  }
}

function scanArtifactTree(root, owner, topLevel, invalidCode, byteBudget) {
  const treePath = path.join(root, topLevel);
  requireDirectory(treePath, owner, "0555", invalidCode);
  const entries = [];

  const visit = (directoryPath, relativeDirectory) => {
    let names;
    try {
      names = fs.readdirSync(directoryPath).sort();
    } catch {
      fail(invalidCode);
    }
    for (const name of names) {
      const absolutePath = path.join(directoryPath, name);
      const relativePath = path.posix.join(relativeDirectory, name);
      validateRelativePath(relativePath);
      let metadata;
      try {
        metadata = fs.lstatSync(absolutePath);
      } catch {
        fail("artifact_entry_not_regular");
      }
      if (metadata.isSymbolicLink()) {
        fail("artifact_symlink_rejected");
      }
      if (!sameOwner(metadata, owner)) {
        fail("artifact_owner_invalid");
      }
      if (metadata.isDirectory()) {
        if (fileMode(metadata) !== "0555") {
          fail("artifact_mode_invalid");
        }
        entries.push({
          path: relativePath,
          type: "directory",
          mode: "0555",
          owner: "release-owner",
        });
        visit(absolutePath, relativePath);
      } else if (metadata.isFile()) {
        if (metadata.nlink !== 1) {
          fail("artifact_hardlink_rejected");
        }
        const mode = fileMode(metadata);
        if (mode !== "0444" && mode !== "0555") {
          fail("artifact_mode_invalid");
        }
        if (metadata.size > MAX_FILE_BYTES) {
          fail("artifact_size_invalid");
        }
        byteBudget.total += metadata.size;
        if (byteBudget.total > MAX_TOTAL_BYTES) {
          fail("artifact_size_invalid");
        }
        entries.push({
          path: relativePath,
          type: "file",
          mode,
          owner: "release-owner",
          size_bytes: metadata.size,
          sha256: sha256File(absolutePath),
        });
      } else {
        fail("artifact_special_file_rejected");
      }
      if (entries.length > MAX_ENTRIES) {
        fail("artifact_entry_count_invalid");
      }
    }
  };

  visit(treePath, topLevel);
  if (entries.length === 0 || !entries.some((entry) => entry.type === "file")) {
    fail(invalidCode);
  }
  return entries.sort((left, right) => left.path.localeCompare(right.path));
}

function scanArtifact(root, owner) {
  const byteBudget = { total: 0 };
  const entries = [
    ...scanArtifactTree(root, owner, "core", "artifact_core_invalid", byteBudget),
    ...scanArtifactTree(root, owner, "runtime", "artifact_runtime_invalid", byteBudget),
  ];
  if (entries.length > MAX_ENTRIES) {
    fail("artifact_entry_count_invalid");
  }
  return entries.sort((left, right) => left.path.localeCompare(right.path));
}

function parseChecksums(root, owner) {
  const checksumPath = path.join(root, "SHA256SUMS");
  const metadata = requireRegularFile(checksumPath, owner, "0444", MAX_METADATA_BYTES);
  if (metadata.size <= 0) {
    fail("checksum_manifest_invalid");
  }
  const checksums = new Map();
  for (const line of fs.readFileSync(checksumPath, "utf8").split("\n")) {
    if (!line) {
      continue;
    }
    const match = line.match(/^([0-9a-f]{64})  ([A-Za-z0-9._+@/-]+)$/);
    if (!match) {
      fail("checksum_manifest_invalid");
    }
    const [, digest, relativePath] = match;
    validateRelativePath(relativePath);
    if (checksums.has(relativePath) || relativePath === "SHA256SUMS") {
      fail("checksum_manifest_invalid");
    }
    checksums.set(relativePath, digest);
  }
  return checksums;
}

function canonicalTimestamp(value) {
  const epoch = Date.parse(value);
  const canonical = Number.isFinite(epoch) ? new Date(epoch).toISOString() : "";
  return (
    Number.isFinite(epoch) &&
    (canonical === value || canonical.replace(".000Z", "Z") === value)
  );
}

function sameIdentity(left, right, fields) {
  return fields.every((field) => left[field] === right[field]);
}

function assertExpectedInputs(expected) {
  if (
    !TAG_PATTERN.test(expected.tag) ||
    !COMMIT_PATTERN.test(expected.commit) ||
    !SHA256_PATTERN.test(expected.sourceSha256) ||
    !SHA256_PATTERN.test(expected.identitySha256) ||
    !SHA256_PATTERN.test(expected.manifestSha256)
  ) {
    fail("invalid_invocation");
  }
}

export function verifyHermesCoreArtifact({
  artifactDir,
  expectedTag,
  expectedCommit,
  expectedSourceSha256,
  expectedIdentitySha256,
  expectedManifestSha256,
}) {
  const expected = {
    tag: expectedTag,
    commit: expectedCommit,
    sourceSha256: expectedSourceSha256,
    identitySha256: expectedIdentitySha256,
    manifestSha256: expectedManifestSha256,
  };
  assertExpectedInputs(expected);

  const root = path.resolve(artifactDir);
  const rootMetadata = requireDirectory(root, null, null, "artifact_root_invalid");
  const owner = { uid: rootMetadata.uid, gid: rootMetadata.gid };
  let topLevel;
  try {
    topLevel = fs.readdirSync(root).sort();
  } catch {
    fail("artifact_root_invalid");
  }
  if (JSON.stringify(topLevel) !== JSON.stringify([...TOP_LEVEL_ENTRIES].sort())) {
    fail("artifact_layout_invalid");
  }

  const manifestPath = path.join(root, "artifact-manifest.json");
  requireRegularFile(manifestPath, owner, "0444", MAX_METADATA_BYTES);
  if (sha256File(manifestPath) !== expected.manifestSha256) {
    fail("manifest_digest_mismatch");
  }

  const manifest = readBoundedJson(root, "artifact-manifest.json", owner);
  const buildReceipt = readBoundedJson(root, "build-receipt.json", owner);
  const updateReceipt = readBoundedJson(root, "update-receipt.json", owner);
  const validationSummary = readBoundedJson(root, "validation-summary.json", owner);
  if (!validateManifest(manifest)) {
    fail("manifest_schema_invalid");
  }
  if (!validateBuildReceipt(buildReceipt)) {
    fail("build_receipt_schema_invalid");
  }
  if (!validateUpdateReceipt(updateReceipt)) {
    fail("update_receipt_schema_invalid");
  }
  if (!validateValidationSummary(validationSummary)) {
    fail("validation_summary_schema_invalid");
  }

  const runtimeVersion = manifest.runtime.site_packages_path.match(
    /^runtime\/venv\/lib\/python([0-9]+)\.([0-9]+)\/site-packages$/
  );
  if (
    !runtimeVersion ||
    `${runtimeVersion[1]}${runtimeVersion[2]}` !== manifest.python_abi.slice(2)
  ) {
    fail("runtime_binding_invalid");
  }

  if (
    manifest.repository !== OFFICIAL_REPOSITORY ||
    manifest.tag !== expected.tag ||
    manifest.commit_sha !== expected.commit ||
    manifest.source_archive_sha256 !== expected.sourceSha256 ||
    manifest.artifact_identity_sha256 !== expected.identitySha256 ||
    manifest.artifact_name !== `hermes-core-${expected.commit}`
  ) {
    fail("artifact_identity_mismatch");
  }
  if (computeHermesCoreArtifactIdentity(manifest) !== expected.identitySha256) {
    fail("artifact_identity_invalid");
  }

  const fullIdentityFields = [
    "repository",
    "tag",
    "commit_sha",
    "source_archive_sha256",
    "artifact_identity_sha256",
    "python_abi",
    "dependency_lock_sha256",
    "runtime_binding_sha256",
    "hermes_cli_version",
  ];
  const updateIdentityFields = [
    "repository",
    "tag",
    "commit_sha",
    "source_archive_sha256",
    "artifact_identity_sha256",
  ];
  if (!sameIdentity(manifest, buildReceipt, fullIdentityFields)) {
    fail("build_receipt_identity_mismatch");
  }
  if (!sameIdentity(manifest, updateReceipt, updateIdentityFields)) {
    fail("update_receipt_identity_mismatch");
  }
  if (
    updateReceipt.new_version !== manifest.hermes_cli_version ||
    validationSummary.artifact_identity_sha256 !== expected.identitySha256 ||
    validationSummary.runtime_binding_sha256 !== manifest.runtime_binding_sha256
  ) {
    fail("receipt_identity_mismatch");
  }

  for (const receipt of [buildReceipt, updateReceipt]) {
    if (
      !canonicalTimestamp(receipt.started_at) ||
      !canonicalTimestamp(receipt.completed_at) ||
      Date.parse(receipt.completed_at) < Date.parse(receipt.started_at)
    ) {
      fail("receipt_timestamp_invalid");
    }
  }
  if (
    Date.parse(updateReceipt.started_at) < Date.parse(buildReceipt.started_at) ||
    Date.parse(updateReceipt.completed_at) > Date.parse(buildReceipt.completed_at)
  ) {
    fail("receipt_timestamp_invalid");
  }

  for (const [key, name] of [
    ["build", "build-receipt.json"],
    ["update", "update-receipt.json"],
    ["validation", "validation-summary.json"],
  ]) {
    if (manifest.receipts[key].sha256 !== sha256File(path.join(root, name))) {
      fail("receipt_digest_mismatch");
    }
  }

  const actualInventory = scanArtifact(root, owner);
  const requiredRuntimePaths = [
    manifest.runtime.interpreter_path,
    manifest.runtime.site_packages_path,
    manifest.runtime.launcher_path,
  ];
  for (const requiredPath of requiredRuntimePaths) {
    const entry = actualInventory.find((candidate) => candidate.path === requiredPath);
    if (!entry) {
      fail("runtime_binding_missing");
    }
    const expectedType =
      requiredPath === manifest.runtime.site_packages_path ? "directory" : "file";
    if (entry.type !== expectedType || entry.mode !== "0555") {
      fail("runtime_binding_invalid");
    }
  }
  const manifestInventory = [...manifest.files].sort((left, right) =>
    left.path.localeCompare(right.path)
  );
  if (
    new Set(manifestInventory.map((entry) => entry.path)).size !==
    manifestInventory.length
  ) {
    fail("artifact_inventory_invalid");
  }
  if (JSON.stringify(actualInventory) !== JSON.stringify(manifestInventory)) {
    fail("artifact_inventory_mismatch");
  }

  const checksums = parseChecksums(root, owner);
  const expectedChecksumPaths = [
    ...METADATA_FILES,
    ...actualInventory
      .filter((entry) => entry.type === "file")
      .map((entry) => entry.path),
  ].sort();
  if (
    JSON.stringify([...checksums.keys()].sort()) !==
    JSON.stringify(expectedChecksumPaths)
  ) {
    fail("checksum_inventory_mismatch");
  }
  for (const [relativePath, digest] of checksums) {
    if (sha256File(path.join(root, ...relativePath.split("/"))) !== digest) {
      fail("checksum_mismatch");
    }
  }

  return {
    commit: manifest.commit_sha,
    tag: manifest.tag,
    fileCount: actualInventory.filter((entry) => entry.type === "file").length,
  };
}

function parseArguments(argv) {
  const allowed = new Set([
    "--artifact-dir",
    "--expected-tag",
    "--expected-commit",
    "--expected-source-sha256",
    "--expected-identity-sha256",
    "--expected-manifest-sha256",
  ]);
  if (argv.length !== allowed.size * 2) {
    fail("invalid_invocation");
  }
  const values = new Map();
  for (let index = 0; index < argv.length; index += 2) {
    const key = argv[index];
    const value = argv[index + 1];
    if (!allowed.has(key) || values.has(key) || !value) {
      fail("invalid_invocation");
    }
    values.set(key, value);
  }
  return {
    artifactDir: values.get("--artifact-dir"),
    expectedTag: values.get("--expected-tag"),
    expectedCommit: values.get("--expected-commit"),
    expectedSourceSha256: values.get("--expected-source-sha256"),
    expectedIdentitySha256: values.get("--expected-identity-sha256"),
    expectedManifestSha256: values.get("--expected-manifest-sha256"),
  };
}

function main() {
  try {
    const result = verifyHermesCoreArtifact(parseArguments(process.argv.slice(2)));
    console.log("hermes_core_artifact_verification=ready");
    console.log(`hermes_core_artifact_tag=${result.tag}`);
    console.log(`hermes_core_artifact_commit=${result.commit}`);
    console.log(`hermes_core_artifact_file_count=${result.fileCount}`);
  } catch (error) {
    const code =
      error instanceof VerificationError && /^[a-z][a-z0-9_]+$/.test(error.message)
        ? error.message
        : "artifact_verification_invalid";
    console.error("hermes_core_artifact_verification=blocked");
    console.error(`hermes_core_artifact_error=${code}`);
    process.exitCode = code === "invalid_invocation" ? 2 : 1;
  }
}

if (
  process.argv[1] &&
  path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url))
) {
  main();
}
