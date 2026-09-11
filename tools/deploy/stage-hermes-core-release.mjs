#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import { verifyHermesCoreArtifact } from "./verify-hermes-core-artifact.mjs";

const FIXED_CORE_ROOT = "/var/lib/qintopia-hermes-core";
const FIXED_CORE_PARENT = "/var/lib";
const FIXED_INGRESS_ROOT = "/var/lib/qintopia-agent-os-deploy/hermes-core-ingress";
const FIXED_INGRESS_PARENT = "/var/lib/qintopia-agent-os-deploy";
const MIN_FREE_BYTES = 5 * 1024 * 1024 * 1024;
const COMMIT_PATTERN = /^[0-9a-f]{40}$/;
const SHA256_PATTERN = /^[0-9a-f]{64}$/;
const TAG_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/;
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

class StagingError extends Error {
  constructor(code) {
    super(code);
    this.name = "StagingError";
  }
}

const fail = (code) => {
  throw new StagingError(code);
};

const modeOf = (metadata) => (metadata.mode & 0o7777).toString(8).padStart(4, "0");

const sameOwner = (metadata, owner) =>
  metadata.uid === owner.uid && metadata.gid === owner.gid;

function requireDirectory(directoryPath, owner, expectedMode, code) {
  let metadata;
  try {
    metadata = fs.lstatSync(directoryPath);
  } catch {
    fail(code);
  }
  if (
    metadata.isSymbolicLink() ||
    !metadata.isDirectory() ||
    !sameOwner(metadata, owner)
  ) {
    fail(code);
  }
  if (expectedMode !== null && modeOf(metadata) !== expectedMode) {
    fail(code);
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
  if (
    metadata.isSymbolicLink() ||
    !metadata.isFile() ||
    metadata.nlink !== 1 ||
    !sameOwner(metadata, owner) ||
    modeOf(metadata) !== expectedMode
  ) {
    fail(code);
  }
  return metadata;
}

function assertNoSymlinkPath(pathValue, code) {
  const resolved = path.resolve(pathValue);
  const root = path.parse(resolved).root;
  let current = root;
  for (const segment of path.relative(root, resolved).split(path.sep).filter(Boolean)) {
    current = path.join(current, segment);
    let metadata;
    try {
      metadata = fs.lstatSync(current);
    } catch {
      fail(code);
    }
    if (metadata.isSymbolicLink() || !metadata.isDirectory()) {
      fail(code);
    }
  }
  if (fs.realpathSync.native(resolved) !== resolved) {
    fail(code);
  }
}

function validateExpected(expected) {
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

function validateManagerRoot({ coreRoot, trustedParent, owner }) {
  const resolvedRoot = path.resolve(coreRoot);
  const resolvedParent = path.resolve(trustedParent);
  if (resolvedRoot !== path.join(resolvedParent, "qintopia-hermes-core")) {
    fail("core_root_path_invalid");
  }
  assertNoSymlinkPath(resolvedParent, "core_root_path_invalid");
  assertNoSymlinkPath(resolvedRoot, "core_root_path_invalid");
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
  const entries = fs.readdirSync(resolvedRoot).sort();
  if (JSON.stringify(entries) !== JSON.stringify([...ROOT_ENTRIES].sort())) {
    fail("core_root_layout_invalid");
  }
  return resolvedRoot;
}

function validateIngressRoot({ ingressRoot, trustedParent, owner, commit }) {
  const resolvedRoot = path.resolve(ingressRoot);
  const resolvedParent = path.resolve(trustedParent);
  if (resolvedRoot !== path.join(resolvedParent, "hermes-core-ingress")) {
    fail("ingress_root_path_invalid");
  }
  assertNoSymlinkPath(resolvedParent, "ingress_root_path_invalid");
  assertNoSymlinkPath(resolvedRoot, "ingress_root_path_invalid");
  const parentMetadata = fs.lstatSync(resolvedParent);
  if (!sameOwner(parentMetadata, owner) || (parentMetadata.mode & 0o022) !== 0) {
    fail("ingress_root_parent_invalid");
  }
  requireDirectory(resolvedRoot, owner, "0700", "ingress_root_invalid");
  const sourcePath = path.join(resolvedRoot, commit);
  requireDirectory(sourcePath, owner, "0555", "source_artifact_invalid");
  return sourcePath;
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

function verifyArtifact(artifactPath, expected, code) {
  try {
    verifyHermesCoreArtifact({
      artifactDir: artifactPath,
      expectedTag: expected.tag,
      expectedCommit: expected.commit,
      expectedSourceSha256: expected.sourceSha256,
      expectedIdentitySha256: expected.identitySha256,
      expectedManifestSha256: expected.manifestSha256,
    });
  } catch {
    fail(code);
  }
}

function fsyncPath(targetPath) {
  const descriptor = fs.openSync(targetPath, "r");
  try {
    fs.fsyncSync(descriptor);
  } finally {
    fs.closeSync(descriptor);
  }
}

function copyArtifactTree(sourceRoot, targetRoot, owner, copyFile, syncPath) {
  fs.mkdirSync(targetRoot, { mode: 0o700 });
  fs.chownSync(targetRoot, owner.uid, owner.gid);

  const copyDirectory = (sourceDirectory, targetDirectory) => {
    for (const name of fs.readdirSync(sourceDirectory).sort()) {
      const sourcePath = path.join(sourceDirectory, name);
      const targetPath = path.join(targetDirectory, name);
      const metadata = fs.lstatSync(sourcePath);
      if (!sameOwner(metadata, owner) || metadata.isSymbolicLink()) {
        fail("source_artifact_changed");
      }
      if (metadata.isDirectory()) {
        fs.mkdirSync(targetPath, { mode: 0o700 });
        fs.chownSync(targetPath, owner.uid, owner.gid);
        copyDirectory(sourcePath, targetPath);
        fs.chmodSync(targetPath, metadata.mode & 0o7777);
        syncPath(targetPath);
      } else if (metadata.isFile() && metadata.nlink === 1) {
        copyFile(sourcePath, targetPath, fs.constants.COPYFILE_EXCL);
        fs.chownSync(targetPath, owner.uid, owner.gid);
        fs.chmodSync(targetPath, metadata.mode & 0o7777);
        syncPath(targetPath);
      } else {
        fail("source_artifact_changed");
      }
    }
  };

  copyDirectory(sourceRoot, targetRoot);
  fs.chmodSync(targetRoot, 0o555);
  syncPath(targetRoot);
}

function quarantinePartial(coreRoot, temporaryPath, commit, syncPath) {
  if (!fs.existsSync(temporaryPath)) {
    return false;
  }
  const metadata = fs.lstatSync(temporaryPath);
  if (metadata.isSymbolicLink() || !metadata.isDirectory()) {
    return false;
  }
  const quarantineRoot = path.join(coreRoot, "quarantine");
  const quarantineName = `failed-${commit}-${crypto.randomBytes(8).toString("hex")}`;
  fs.chmodSync(temporaryPath, 0o700);
  let renamed = false;
  try {
    fs.renameSync(temporaryPath, path.join(quarantineRoot, quarantineName));
    renamed = true;
    syncPath(path.join(coreRoot, "incoming"));
    syncPath(quarantineRoot);
  } catch (error) {
    if (renamed) {
      fail("candidate_quarantine_commit_uncertain");
    }
    throw error;
  }
  return true;
}

function recoverInterruptedStaging(
  coreRoot,
  incomingRoot,
  incomingEntries,
  commit,
  owner,
  syncPath
) {
  const stagingPattern = new RegExp(`^\\.staging-${commit}-[0-9a-f]{16}$`);
  let recovered = 0;
  for (const name of incomingEntries) {
    if (name === commit) {
      continue;
    }
    if (!stagingPattern.test(name)) {
      fail("candidate_location_invalid");
    }
    const temporaryPath = path.join(incomingRoot, name);
    let metadata;
    try {
      metadata = fs.lstatSync(temporaryPath);
    } catch {
      fail("candidate_location_invalid");
    }
    const mode = modeOf(metadata);
    if (
      metadata.isSymbolicLink() ||
      !metadata.isDirectory() ||
      !sameOwner(metadata, owner) ||
      (mode !== "0700" && mode !== "0555")
    ) {
      fail("candidate_location_invalid");
    }
    if (!quarantinePartial(coreRoot, temporaryPath, commit, syncPath)) {
      fail("candidate_quarantine_failed");
    }
    recovered += 1;
  }
  return recovered;
}

export function stageHermesCoreRelease({
  expected,
  coreRoot = FIXED_CORE_ROOT,
  coreTrustedParent = FIXED_CORE_PARENT,
  ingressRoot = FIXED_INGRESS_ROOT,
  ingressTrustedParent = FIXED_INGRESS_PARENT,
  owner = { uid: 0, gid: 0 },
  minimumFreeBytes = MIN_FREE_BYTES,
  lockHeld = false,
  lockFd = null,
  verifyInheritedLock = true,
  copyFile = fs.copyFileSync,
  syncPath = fsyncPath,
}) {
  validateExpected(expected);
  const resolvedRoot = validateManagerRoot({
    coreRoot,
    trustedParent: coreTrustedParent,
    owner,
  });
  if (!lockHeld) {
    fail("manager_lock_not_held");
  }
  if (verifyInheritedLock) {
    validateInheritedLock(resolvedRoot, owner, lockFd);
  }
  validateFreeSpace(resolvedRoot, minimumFreeBytes);
  const sourcePath = validateIngressRoot({
    ingressRoot,
    trustedParent: ingressTrustedParent,
    owner,
    commit: expected.commit,
  });

  const incomingRoot = path.join(resolvedRoot, "incoming");
  const incomingEntries = fs.readdirSync(incomingRoot).sort();
  const candidatePath = path.join(incomingRoot, expected.commit);
  if (fs.existsSync(path.join(resolvedRoot, "releases", expected.commit))) {
    fail("candidate_location_invalid");
  }
  const recoveredStagingCount = recoverInterruptedStaging(
    resolvedRoot,
    incomingRoot,
    incomingEntries,
    expected.commit,
    owner,
    syncPath
  );
  const remainingEntries = fs.readdirSync(incomingRoot).sort();
  if (remainingEntries.length === 1 && remainingEntries[0] === expected.commit) {
    verifyArtifact(candidatePath, expected, "staged_artifact_invalid");
    try {
      syncPath(candidatePath);
      syncPath(incomingRoot);
    } catch {
      fail("candidate_staging_commit_uncertain");
    }
    return {
      candidateCommit: expected.commit,
      sourceVerified: true,
      stagedVerified: true,
      candidateInstalled: true,
      alreadyStaged: true,
      pointerChanges: 0,
      serviceChanges: 0,
      quarantined: recoveredStagingCount > 0,
    };
  }
  if (remainingEntries.length !== 0) {
    fail("candidate_location_invalid");
  }
  verifyArtifact(sourcePath, expected, "source_artifact_invalid");

  const temporaryName = `.staging-${expected.commit}-${crypto
    .randomBytes(8)
    .toString("hex")}`;
  const temporaryPath = path.join(incomingRoot, temporaryName);
  let quarantined = false;
  let candidateRenamed = false;
  try {
    copyArtifactTree(sourcePath, temporaryPath, owner, copyFile, syncPath);
    verifyArtifact(temporaryPath, expected, "staged_artifact_invalid");
    fs.renameSync(temporaryPath, candidatePath);
    candidateRenamed = true;
    syncPath(incomingRoot);
  } catch (error) {
    if (candidateRenamed) {
      fail("candidate_staging_commit_uncertain");
    }
    try {
      quarantined = quarantinePartial(
        resolvedRoot,
        temporaryPath,
        expected.commit,
        syncPath
      );
    } catch (quarantineError) {
      if (
        quarantineError instanceof StagingError &&
        quarantineError.message === "candidate_quarantine_commit_uncertain"
      ) {
        throw quarantineError;
      }
      quarantined = false;
    }
    if (fs.existsSync(temporaryPath) && !quarantined) {
      fail("candidate_quarantine_failed");
    }
    if (error instanceof StagingError) {
      throw error;
    }
    fail("candidate_staging_failed");
  }

  return {
    candidateCommit: expected.commit,
    sourceVerified: true,
    stagedVerified: true,
    candidateInstalled: true,
    alreadyStaged: false,
    pointerChanges: 0,
    serviceChanges: 0,
    quarantined: quarantined || recoveredStagingCount > 0,
  };
}

function parseArguments(argv) {
  const valueOptions = new Set([
    "--expected-tag",
    "--expected-commit",
    "--expected-source-sha256",
    "--expected-identity-sha256",
    "--expected-manifest-sha256",
  ]);
  if (!argv.includes("--stage") || argv.length !== valueOptions.size * 2 + 1) {
    fail("invalid_invocation");
  }
  const values = new Map();
  for (let index = 0; index < argv.length;) {
    const key = argv[index];
    if (key === "--stage") {
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
  };
}

function main() {
  try {
    const expected = parseArguments(process.argv.slice(2));
    if (process.env.QINTOPIA_HERMES_CORE_LOCK_HELD !== "1") {
      fail("manager_lock_not_held");
    }
    const result = stageHermesCoreRelease({
      expected,
      lockHeld: true,
      lockFd: 9,
    });
    console.log("hermes_core_candidate_staging=ready");
    console.log(`hermes_core_candidate_commit=${result.candidateCommit}`);
    console.log("hermes_core_source_verified=true");
    console.log("hermes_core_staged_verified=true");
    console.log("hermes_core_candidate_installed=true");
    console.log(`hermes_core_candidate_already_staged=${result.alreadyStaged}`);
    console.log("hermes_core_pointer_changes=0");
    console.log("hermes_core_service_changes=0");
  } catch (error) {
    const code =
      error instanceof StagingError && /^[a-z][a-z0-9_]+$/.test(error.message)
        ? error.message
        : "hermes_core_candidate_staging_invalid";
    console.error("hermes_core_candidate_staging=blocked");
    console.error(`hermes_core_candidate_staging_error=${code}`);
    process.exitCode = code === "invalid_invocation" ? 2 : 1;
  }
}

if (
  process.argv[1] &&
  path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url))
) {
  main();
}
