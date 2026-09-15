#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import { verifyHermesCoreArtifact } from "./verify-hermes-core-artifact.mjs";

const FIXED_CORE_ROOT = "/var/lib/qintopia-hermes-core";
const FIXED_TRUSTED_PARENT = "/var/lib";
const MIN_FREE_BYTES = 5 * 1024 * 1024 * 1024;
const COMMIT_PATTERN = /^[0-9a-f]{40}$/;
const SHA256_PATTERN = /^[0-9a-f]{64}$/;
const TAG_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/;
const MANIFEST_LIMIT = 8 * 1024 * 1024;
const ROOT_ENTRIES = Object.freeze([
  "current",
  "incoming",
  "lineage",
  "previous",
  "quarantine",
  "releases",
  "rollback-reserve",
  "state",
]);
const LINEAGE_ROLES = Object.freeze(["current", "previous", "rollback-reserve"]);

class PlanningError extends Error {
  constructor(code) {
    super(code);
    this.name = "PlanningError";
  }
}

const fail = (code) => {
  throw new PlanningError(code);
};

const modeOf = (metadata) => (metadata.mode & 0o7777).toString(8).padStart(4, "0");

const sameOwner = (metadata, owner) =>
  metadata.uid === owner.uid && metadata.gid === owner.gid;

export const computeHermesCoreLineageFingerprint = ({
  currentCommit,
  previousCommit,
  rollbackReserveCommit,
}) =>
  crypto
    .createHash("sha256")
    .update(`${currentCommit}\n${previousCommit}\n${rollbackReserveCommit}\n`)
    .digest("hex");

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
  if (!sameOwner(metadata, owner)) {
    fail("core_root_owner_invalid");
  }
  if (modeOf(metadata) !== expectedMode) {
    fail("core_root_mode_invalid");
  }
  return metadata;
}

function requireRegularFile(filePath, owner, expectedMode, code) {
  let metadata;
  try {
    metadata = fs.lstatSync(filePath);
  } catch {
    fail(code);
  }
  if (metadata.isSymbolicLink() || !metadata.isFile() || metadata.nlink !== 1) {
    fail(code);
  }
  if (!sameOwner(metadata, owner)) {
    fail("core_root_owner_invalid");
  }
  if (modeOf(metadata) !== expectedMode) {
    fail("core_root_mode_invalid");
  }
  return metadata;
}

function assertNoSymlinkPath(pathValue) {
  const resolved = path.resolve(pathValue);
  const root = path.parse(resolved).root;
  let current = root;
  for (const segment of path.relative(root, resolved).split(path.sep).filter(Boolean)) {
    current = path.join(current, segment);
    let metadata;
    try {
      metadata = fs.lstatSync(current);
    } catch {
      fail("core_root_path_invalid");
    }
    if (metadata.isSymbolicLink() || !metadata.isDirectory()) {
      fail("core_root_path_invalid");
    }
  }
  if (fs.realpathSync.native(resolved) !== resolved) {
    fail("core_root_path_invalid");
  }
}

function validateLineageArguments(expected) {
  if (
    !COMMIT_PATTERN.test(expected.currentCommit) ||
    !COMMIT_PATTERN.test(expected.previousCommit) ||
    !COMMIT_PATTERN.test(expected.rollbackReserveCommit)
  ) {
    fail("invalid_invocation");
  }
  if (expected.currentCommit === expected.previousCommit) {
    fail("lineage_identity_invalid");
  }
}

function validateArguments(expected) {
  validateLineageArguments(expected);
  if (
    !TAG_PATTERN.test(expected.tag) ||
    !COMMIT_PATTERN.test(expected.commit) ||
    !SHA256_PATTERN.test(expected.sourceSha256) ||
    !SHA256_PATTERN.test(expected.identitySha256) ||
    !SHA256_PATTERN.test(expected.manifestSha256)
  ) {
    fail("invalid_invocation");
  }
  if (expected.commit === expected.currentCommit) {
    fail("lineage_identity_invalid");
  }
}

function validateFixedRoot({ coreRoot, trustedParent, owner }) {
  const resolvedRoot = path.resolve(coreRoot);
  const resolvedParent = path.resolve(trustedParent);
  if (resolvedRoot !== path.join(resolvedParent, "qintopia-hermes-core")) {
    fail("core_root_path_invalid");
  }
  assertNoSymlinkPath(resolvedParent);
  assertNoSymlinkPath(resolvedRoot);
  const parentMetadata = fs.lstatSync(resolvedParent);
  if (!sameOwner(parentMetadata, owner) || (parentMetadata.mode & 0o022) !== 0) {
    fail("core_root_parent_invalid");
  }
  requireDirectory(resolvedRoot, owner, "0755", "core_root_invalid");
  requireDirectory(
    path.join(resolvedRoot, "releases"),
    owner,
    "0755",
    "release_directory_invalid"
  );
  requireDirectory(
    path.join(resolvedRoot, "lineage"),
    owner,
    "0755",
    "lineage_directory_invalid"
  );
  requireDirectory(
    path.join(resolvedRoot, "lineage", "generations"),
    owner,
    "0755",
    "lineage_directory_invalid"
  );
  for (const name of ["incoming", "quarantine", "state"]) {
    requireDirectory(
      path.join(resolvedRoot, name),
      owner,
      "0700",
      "private_directory_invalid"
    );
  }
  requireDirectory(
    path.join(resolvedRoot, "state", "transactions"),
    owner,
    "0700",
    "transaction_directory_invalid"
  );
  requireRegularFile(
    path.join(resolvedRoot, "state", "manager.lock"),
    owner,
    "0600",
    "manager_lock_invalid"
  );
  let rootEntries;
  try {
    rootEntries = fs.readdirSync(resolvedRoot).sort();
  } catch {
    fail("core_root_invalid");
  }
  if (JSON.stringify(rootEntries) !== JSON.stringify([...ROOT_ENTRIES].sort())) {
    fail("core_root_layout_invalid");
  }
  return resolvedRoot;
}

function validateInheritedLock(coreRoot, owner, lockFd) {
  const lockPath = path.join(coreRoot, "state", "manager.lock");
  let descriptorMetadata;
  let pathMetadata;
  try {
    descriptorMetadata = fs.fstatSync(lockFd);
    pathMetadata = fs.lstatSync(lockPath);
  } catch {
    fail("manager_lock_not_held");
  }
  if (
    !descriptorMetadata.isFile() ||
    !pathMetadata.isFile() ||
    pathMetadata.isSymbolicLink() ||
    descriptorMetadata.nlink !== 1 ||
    pathMetadata.nlink !== 1 ||
    descriptorMetadata.dev !== pathMetadata.dev ||
    descriptorMetadata.ino !== pathMetadata.ino ||
    !sameOwner(descriptorMetadata, owner) ||
    modeOf(descriptorMetadata) !== "0600"
  ) {
    fail("manager_lock_not_held");
  }
}

function readManifestIdentity(releasePath, owner) {
  const manifestPath = path.join(releasePath, "artifact-manifest.json");
  const metadata = requireRegularFile(
    manifestPath,
    owner,
    "0444",
    "release_manifest_invalid"
  );
  if (metadata.size <= 0 || metadata.size > MANIFEST_LIMIT) {
    fail("release_manifest_invalid");
  }
  let bytes;
  let manifest;
  try {
    bytes = fs.readFileSync(manifestPath);
    manifest = JSON.parse(bytes.toString("utf8"));
  } catch {
    fail("release_manifest_invalid");
  }
  for (const [value, pattern] of [
    [manifest.tag, TAG_PATTERN],
    [manifest.commit_sha, COMMIT_PATTERN],
    [manifest.source_archive_sha256, SHA256_PATTERN],
    [manifest.artifact_identity_sha256, SHA256_PATTERN],
  ]) {
    if (typeof value !== "string" || !pattern.test(value)) {
      fail("release_manifest_invalid");
    }
  }
  return {
    tag: manifest.tag,
    commit: manifest.commit_sha,
    sourceSha256: manifest.source_archive_sha256,
    identitySha256: manifest.artifact_identity_sha256,
    manifestSha256: cryptoSha256(bytes),
  };
}

function cryptoSha256(bytes) {
  return crypto.createHash("sha256").update(bytes).digest("hex");
}

function validateReleaseArtifact(releasePath, owner, expected) {
  requireDirectory(releasePath, owner, "0555", "release_artifact_invalid");
  try {
    verifyHermesCoreArtifact({
      artifactDir: releasePath,
      expectedTag: expected.tag,
      expectedCommit: expected.commit,
      expectedSourceSha256: expected.sourceSha256,
      expectedIdentitySha256: expected.identitySha256,
      expectedManifestSha256: expected.manifestSha256,
    });
  } catch {
    fail("release_artifact_invalid");
  }
}

function validateCandidateLineageBinding(candidatePath, expectedCurrentCommit) {
  let receipt;
  try {
    receipt = JSON.parse(
      fs.readFileSync(path.join(candidatePath, "update-receipt.json"), "utf8")
    );
  } catch {
    fail("candidate_lineage_binding_invalid");
  }
  if (receipt.previous_commit_sha !== expectedCurrentCommit) {
    fail("candidate_lineage_binding_invalid");
  }
}

function validateReleaseCollection(coreRoot, owner) {
  const releasesRoot = path.join(coreRoot, "releases");
  let names;
  try {
    names = fs.readdirSync(releasesRoot).sort();
  } catch {
    fail("release_directory_invalid");
  }
  for (const name of names) {
    if (!COMMIT_PATTERN.test(name)) {
      fail("release_directory_invalid");
    }
    requireDirectory(
      path.join(releasesRoot, name),
      owner,
      "0555",
      "release_directory_invalid"
    );
  }
}

function validateLineage(coreRoot, expected, owner) {
  const lineageRoot = path.join(coreRoot, "lineage");
  const generationsRoot = path.join(lineageRoot, "generations");
  const fingerprint = computeHermesCoreLineageFingerprint(expected);
  const generationName = `generation-${fingerprint}`;
  const generationPath = path.join(generationsRoot, generationName);

  let lineageEntries;
  let activeMetadata;
  let activeTarget;
  try {
    lineageEntries = fs.readdirSync(lineageRoot).sort();
    activeMetadata = fs.lstatSync(path.join(lineageRoot, "active"));
    activeTarget = fs.readlinkSync(path.join(lineageRoot, "active"));
  } catch {
    fail("lineage_pointer_invalid");
  }
  if (
    JSON.stringify(lineageEntries) !== JSON.stringify(["active", "generations"]) ||
    !activeMetadata.isSymbolicLink() ||
    activeTarget !== `generations/${generationName}`
  ) {
    fail("lineage_pointer_invalid");
  }
  requireDirectory(generationPath, owner, "0555", "lineage_generation_invalid");

  let generationEntries;
  try {
    generationEntries = fs.readdirSync(generationPath).sort();
  } catch {
    fail("lineage_generation_invalid");
  }
  if (
    JSON.stringify(generationEntries) !==
    JSON.stringify([...LINEAGE_ROLES, "lineage.json"].sort())
  ) {
    fail("lineage_generation_invalid");
  }

  const manifestPath = path.join(generationPath, "lineage.json");
  const manifestMetadata = requireRegularFile(
    manifestPath,
    owner,
    "0444",
    "lineage_generation_invalid"
  );
  if (manifestMetadata.size <= 0 || manifestMetadata.size > MANIFEST_LIMIT) {
    fail("lineage_generation_invalid");
  }
  let manifest;
  try {
    manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
  } catch {
    fail("lineage_generation_invalid");
  }
  const expectedManifest = {
    schema_version: 1,
    generation_type: "hermes-core-lineage",
    fingerprint_sha256: fingerprint,
    current_commit: expected.currentCommit,
    previous_commit: expected.previousCommit,
    rollback_reserve_commit: expected.rollbackReserveCommit,
  };
  if (JSON.stringify(manifest) !== JSON.stringify(expectedManifest)) {
    fail("lineage_identity_invalid");
  }

  const releasePaths = {};
  for (const [role, commit] of [
    ["current", expected.currentCommit],
    ["previous", expected.previousCommit],
    ["rollback-reserve", expected.rollbackReserveCommit],
  ]) {
    const generationPointer = path.join(generationPath, role);
    const topLevelPointer = path.join(coreRoot, role);
    let generationMetadata;
    let generationTarget;
    let topLevelMetadata;
    let topLevelTarget;
    try {
      generationMetadata = fs.lstatSync(generationPointer);
      generationTarget = fs.readlinkSync(generationPointer);
      topLevelMetadata = fs.lstatSync(topLevelPointer);
      topLevelTarget = fs.readlinkSync(topLevelPointer);
    } catch {
      fail("lineage_pointer_invalid");
    }
    const expectedGenerationTarget = `../../../releases/${commit}`;
    const expectedTopLevelTarget = `lineage/active/${role}`;
    const releasePath = path.join(coreRoot, "releases", commit);
    if (
      !generationMetadata.isSymbolicLink() ||
      generationTarget !== expectedGenerationTarget ||
      !topLevelMetadata.isSymbolicLink() ||
      topLevelTarget !== expectedTopLevelTarget ||
      fs.realpathSync.native(generationPointer) !== releasePath ||
      fs.realpathSync.native(topLevelPointer) !== releasePath
    ) {
      fail("lineage_pointer_invalid");
    }
    requireDirectory(releasePath, owner, "0555", "lineage_pointer_invalid");
    releasePaths[role] = releasePath;
  }
  return { fingerprint, generationName, releasePaths };
}

function validateFreeSpace(coreRoot, minimumFreeBytes) {
  let available;
  try {
    const stats = fs.statfsSync(coreRoot, { bigint: true });
    available = stats.bavail * stats.bsize;
  } catch {
    fail("core_root_space_invalid");
  }
  if (available < BigInt(minimumFreeBytes)) {
    fail("core_root_space_insufficient");
  }
}

export function verifyHermesCoreLineage({
  expectedLineage,
  coreRoot = FIXED_CORE_ROOT,
  trustedParent = FIXED_TRUSTED_PARENT,
  owner = { uid: 0, gid: 0 },
  minimumFreeBytes = MIN_FREE_BYTES,
  lockHeld = false,
  lockFd = null,
  verifyInheritedLock = true,
}) {
  validateLineageArguments(expectedLineage);
  const resolvedRoot = verifyHermesCoreManagerBoundary({
    coreRoot,
    trustedParent,
    owner,
    minimumFreeBytes,
    lockHeld,
    lockFd,
    verifyInheritedLock,
  });
  const lineage = validateLineage(resolvedRoot, expectedLineage, owner);

  const verified = new Map();
  for (const [releasePath, commit] of [
    [lineage.releasePaths.current, expectedLineage.currentCommit],
    [lineage.releasePaths.previous, expectedLineage.previousCommit],
    [lineage.releasePaths["rollback-reserve"], expectedLineage.rollbackReserveCommit],
  ]) {
    if (verified.has(releasePath)) {
      continue;
    }
    const identity = readManifestIdentity(releasePath, owner);
    if (identity.commit !== commit) {
      fail("lineage_identity_invalid");
    }
    validateReleaseArtifact(releasePath, owner, identity);
    verified.set(releasePath, true);
  }

  return {
    coreRoot: resolvedRoot,
    currentCommit: expectedLineage.currentCommit,
    previousCommit: expectedLineage.previousCommit,
    rollbackReserveCommit: expectedLineage.rollbackReserveCommit,
    lineageFingerprint: lineage.fingerprint,
    lineageGeneration: lineage.generationName,
    verifiedLineageCount: verified.size,
  };
}

export function verifyHermesCoreManagerBoundary({
  coreRoot = FIXED_CORE_ROOT,
  trustedParent = FIXED_TRUSTED_PARENT,
  owner = { uid: 0, gid: 0 },
  minimumFreeBytes = MIN_FREE_BYTES,
  lockHeld = false,
  lockFd = null,
  verifyInheritedLock = true,
}) {
  const resolvedRoot = verifyHermesCoreManagerLockBoundary({
    coreRoot,
    trustedParent,
    owner,
    lockHeld,
    lockFd,
    verifyInheritedLock,
  });
  validateFreeSpace(resolvedRoot, minimumFreeBytes);
  validateReleaseCollection(resolvedRoot, owner);
  return resolvedRoot;
}

export function verifyHermesCoreManagerLockBoundary({
  coreRoot = FIXED_CORE_ROOT,
  trustedParent = FIXED_TRUSTED_PARENT,
  owner = { uid: 0, gid: 0 },
  lockHeld = false,
  lockFd = null,
  verifyInheritedLock = true,
}) {
  const resolvedRoot = validateFixedRoot({ coreRoot, trustedParent, owner });
  if (!lockHeld) {
    fail("manager_lock_not_held");
  }
  if (verifyInheritedLock) {
    validateInheritedLock(resolvedRoot, owner, lockFd);
  }
  return resolvedRoot;
}

export function planHermesCoreRelease({
  expected,
  coreRoot = FIXED_CORE_ROOT,
  trustedParent = FIXED_TRUSTED_PARENT,
  owner = { uid: 0, gid: 0 },
  minimumFreeBytes = MIN_FREE_BYTES,
  lockHeld = false,
  lockFd = null,
  verifyInheritedLock = true,
}) {
  validateArguments(expected);
  const lineage = verifyHermesCoreLineage({
    expectedLineage: expected,
    coreRoot,
    trustedParent,
    owner,
    minimumFreeBytes,
    lockHeld,
    lockFd,
    verifyInheritedLock,
  });
  const resolvedRoot = lineage.coreRoot;

  const incomingRoot = path.join(resolvedRoot, "incoming");
  const incomingEntries = fs.readdirSync(incomingRoot).sort();
  if (
    incomingEntries.length !== 1 ||
    incomingEntries[0] !== expected.commit ||
    fs.existsSync(path.join(resolvedRoot, "releases", expected.commit))
  ) {
    fail("candidate_location_invalid");
  }
  const candidatePath = path.join(incomingRoot, expected.commit);
  validateReleaseArtifact(candidatePath, owner, expected);
  validateCandidateLineageBinding(candidatePath, expected.currentCommit);

  return {
    candidateCommit: expected.commit,
    currentCommit: expected.currentCommit,
    previousCommit: expected.previousCommit,
    rollbackReserveCommit: expected.rollbackReserveCommit,
    lineageFingerprint: lineage.lineageFingerprint,
    lineageGeneration: lineage.lineageGeneration,
    verifiedLineageCount: lineage.verifiedLineageCount,
    pointerChanges: 0,
    serviceChanges: 0,
  };
}

function parseArguments(argv) {
  const valueOptions = new Set([
    "--expected-tag",
    "--expected-commit",
    "--expected-source-sha256",
    "--expected-identity-sha256",
    "--expected-manifest-sha256",
    "--expected-current-commit",
    "--expected-previous-commit",
    "--expected-rollback-reserve-commit",
  ]);
  if (!argv.includes("--dry-run") || argv.length !== valueOptions.size * 2 + 1) {
    fail("invalid_invocation");
  }
  const values = new Map();
  for (let index = 0; index < argv.length;) {
    const key = argv[index];
    if (key === "--dry-run") {
      if (values.has(key)) {
        fail("invalid_invocation");
      }
      values.set(key, true);
      index += 1;
      continue;
    }
    if (!valueOptions.has(key) || values.has(key) || !argv[index + 1]) {
      fail("invalid_invocation");
    }
    values.set(key, argv[index + 1]);
    index += 2;
  }
  return {
    tag: values.get("--expected-tag"),
    commit: values.get("--expected-commit"),
    sourceSha256: values.get("--expected-source-sha256"),
    identitySha256: values.get("--expected-identity-sha256"),
    manifestSha256: values.get("--expected-manifest-sha256"),
    currentCommit: values.get("--expected-current-commit"),
    previousCommit: values.get("--expected-previous-commit"),
    rollbackReserveCommit: values.get("--expected-rollback-reserve-commit"),
  };
}

function main() {
  try {
    const expected = parseArguments(process.argv.slice(2));
    if (process.env.QINTOPIA_HERMES_CORE_LOCK_HELD !== "1") {
      fail("manager_lock_not_held");
    }
    const result = planHermesCoreRelease({
      expected,
      lockHeld: true,
      lockFd: 9,
    });
    console.log("hermes_core_release_dry_run=ready");
    console.log(`hermes_core_candidate_commit=${result.candidateCommit}`);
    console.log(`hermes_core_current_commit=${result.currentCommit}`);
    console.log(`hermes_core_previous_commit=${result.previousCommit}`);
    console.log(`hermes_core_rollback_reserve_commit=${result.rollbackReserveCommit}`);
    console.log(`hermes_core_lineage_fingerprint=${result.lineageFingerprint}`);
    console.log(`hermes_core_lineage_generation=${result.lineageGeneration}`);
    console.log(`hermes_core_verified_lineage_count=${result.verifiedLineageCount}`);
    console.log("hermes_core_pointer_changes=0");
    console.log("hermes_core_service_changes=0");
  } catch (error) {
    const code =
      error instanceof PlanningError && /^[a-z][a-z0-9_]+$/.test(error.message)
        ? error.message
        : "hermes_core_release_plan_invalid";
    console.error("hermes_core_release_dry_run=blocked");
    console.error(`hermes_core_release_error=${code}`);
    process.exitCode = code === "invalid_invocation" ? 2 : 1;
  }
}

if (
  process.argv[1] &&
  path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url))
) {
  main();
}
