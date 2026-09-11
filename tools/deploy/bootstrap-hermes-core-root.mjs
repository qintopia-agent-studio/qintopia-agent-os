#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import { computeHermesCoreLineageFingerprint } from "./plan-hermes-core-release.mjs";
import { verifyHermesCoreArtifact } from "./verify-hermes-core-artifact.mjs";

export const computeHermesCoreBootstrapLineageFingerprint =
  computeHermesCoreLineageFingerprint;

export const FIXED_CORE_ROOT = "/var/lib/qintopia-hermes-core";
export const FIXED_CORE_PARENT = "/var/lib";
export const FIXED_INGRESS_ROOT =
  "/var/lib/qintopia-agent-os-deploy/hermes-core-ingress";
export const FIXED_INGRESS_PARENT = "/var/lib/qintopia-agent-os-deploy";
export const FIXED_BOOTSTRAP_LOCK_PATH =
  "/var/lib/qintopia-agent-os-deploy/hermes-core-bootstrap.lock";
export const FIXED_BOOTSTRAP_LOCK_PARENT = FIXED_INGRESS_PARENT;
export const FIXED_BOOTSTRAP_QUARANTINE =
  "/var/lib/qintopia-agent-os-deploy/hermes-core-bootstrap-quarantine";
export const FIXED_BOOTSTRAP_QUARANTINE_PARENT = FIXED_INGRESS_PARENT;
export const FIXED_PENDING_ROOT_NAME = ".qintopia-hermes-core.bootstrap.pending";

const ROOT_OWNER = Object.freeze({ uid: 0, gid: 0 });
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
const LINEAGE_ROLES = Object.freeze([
  ["current", "currentCommit"],
  ["previous", "previousCommit"],
  ["rollback-reserve", "rollbackReserveCommit"],
]);
const MAX_METADATA_BYTES = 8 * 1024 * 1024;
const MIN_FREE_BYTES = 5 * 1024 * 1024 * 1024;
const BOOTSTRAP_LOCK_MODE = 0o600;
const BOOTSTRAP_LOCK_CREATE_FLAGS =
  fs.constants.O_RDWR |
  fs.constants.O_CREAT |
  fs.constants.O_EXCL |
  (fs.constants.O_NOFOLLOW ?? 0);
const BOOTSTRAP_LOCK_OPEN_FLAGS = fs.constants.O_RDWR | (fs.constants.O_NOFOLLOW ?? 0);
const COMMIT_PATTERN = /^[0-9a-f]{40}$/;
const SHA256_PATTERN = /^[0-9a-f]{64}$/;
const TAG_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/;
const SAFE_ERROR_PATTERN = /^[a-z][a-z0-9_]+$/;

export class BootstrapError extends Error {
  constructor(code) {
    super(code);
    this.name = "BootstrapError";
    this.code = code;
  }
}

const fail = (code) => {
  throw new BootstrapError(code);
};

const modeOf = (metadata) => (metadata.mode & 0o7777).toString(8).padStart(4, "0");

const sameOwner = (metadata, owner) =>
  metadata.uid === owner.uid && metadata.gid === owner.gid;

const isObject = (value) =>
  value !== null && typeof value === "object" && !Array.isArray(value);

function normalizeOwner(owner) {
  if (
    !isObject(owner) ||
    !Number.isInteger(owner.uid) ||
    !Number.isInteger(owner.gid) ||
    owner.uid < 0 ||
    owner.gid < 0
  ) {
    fail("invalid_invocation");
  }
  return { uid: owner.uid, gid: owner.gid };
}

function lstatOrNull(targetPath) {
  try {
    return fs.lstatSync(targetPath);
  } catch (error) {
    if (error?.code === "ENOENT") {
      return null;
    }
    throw error;
  }
}

function pathExists(targetPath) {
  return lstatOrNull(targetPath) !== null;
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
  try {
    if (fs.realpathSync.native(resolved) !== resolved) {
      fail(code);
    }
  } catch {
    fail(code);
  }
}

function validateTrustedParent(parentPath, owner, code) {
  const resolved = path.resolve(parentPath);
  assertNoSymlinkPath(resolved, code);
  let metadata;
  try {
    metadata = fs.lstatSync(resolved);
  } catch {
    fail(code);
  }
  if (
    metadata.isSymbolicLink() ||
    !metadata.isDirectory() ||
    !sameOwner(metadata, owner) ||
    (metadata.mode & 0o022) !== 0
  ) {
    fail(code);
  }
  return resolved;
}

function requireDirectory(targetPath, owner, expectedMode, code) {
  let metadata;
  try {
    metadata = fs.lstatSync(targetPath);
  } catch {
    fail(code);
  }
  if (
    metadata.isSymbolicLink() ||
    !metadata.isDirectory() ||
    !sameOwner(metadata, owner) ||
    (expectedMode !== null && modeOf(metadata) !== expectedMode)
  ) {
    fail(code);
  }
  return metadata;
}

function requireRegularFile(targetPath, owner, expectedMode, code) {
  let metadata;
  try {
    metadata = fs.lstatSync(targetPath);
  } catch {
    fail(code);
  }
  if (
    metadata.isSymbolicLink() ||
    !metadata.isFile() ||
    metadata.nlink !== 1 ||
    !sameOwner(metadata, owner) ||
    (expectedMode !== null && modeOf(metadata) !== expectedMode)
  ) {
    fail(code);
  }
  return metadata;
}

function requireSymlink(targetPath, expectedTarget, owner, code) {
  let metadata;
  let actualTarget;
  try {
    metadata = fs.lstatSync(targetPath);
    actualTarget = fs.readlinkSync(targetPath);
  } catch {
    fail(code);
  }
  if (
    !metadata.isSymbolicLink() ||
    !sameOwner(metadata, owner) ||
    actualTarget !== expectedTarget
  ) {
    fail(code);
  }
}

function assertEmptyDirectory(directoryPath, code) {
  let entries;
  try {
    entries = fs.readdirSync(directoryPath);
  } catch {
    fail(code);
  }
  if (entries.length !== 0) {
    fail(code);
  }
}

function assertExactEntries(directoryPath, expectedEntries, code) {
  let entries;
  try {
    entries = fs.readdirSync(directoryPath).sort();
  } catch {
    fail(code);
  }
  if (JSON.stringify(entries) !== JSON.stringify([...expectedEntries].sort())) {
    fail(code);
  }
}

function validateExpected(expected) {
  if (
    !isObject(expected) ||
    typeof expected.tag !== "string" ||
    typeof expected.commit !== "string" ||
    typeof expected.sourceSha256 !== "string" ||
    typeof expected.identitySha256 !== "string" ||
    typeof expected.manifestSha256 !== "string" ||
    !TAG_PATTERN.test(expected.tag) ||
    !COMMIT_PATTERN.test(expected.commit) ||
    !SHA256_PATTERN.test(expected.sourceSha256) ||
    !SHA256_PATTERN.test(expected.identitySha256) ||
    !SHA256_PATTERN.test(expected.manifestSha256)
  ) {
    fail("invalid_invocation");
  }
  return {
    tag: expected.tag,
    commit: expected.commit,
    sourceSha256: expected.sourceSha256,
    identitySha256: expected.identitySha256,
    manifestSha256: expected.manifestSha256,
  };
}

function expectedField(value, camelName, snakeName) {
  return value?.[camelName] ?? value?.[snakeName];
}

function normalizeArtifactSpec(spec, role, ingressRoot = null, fallbackRoot = null) {
  if (!isObject(spec)) {
    fail("invalid_invocation");
  }
  const expectedInput = spec.expected ?? spec;
  const expected = validateExpected({
    tag: expectedField(expectedInput, "tag", "tag"),
    commit: expectedField(expectedInput, "commit", "commit_sha"),
    sourceSha256: expectedField(expectedInput, "sourceSha256", "source_archive_sha256"),
    identitySha256: expectedField(
      expectedInput,
      "identitySha256",
      "artifact_identity_sha256"
    ),
    manifestSha256: expectedField(expectedInput, "manifestSha256", "manifest_sha256"),
  });
  const suppliedPath =
    spec.artifactDir ?? spec.path ?? spec.root ?? spec.artifactPath ?? null;
  const artifactDir =
    suppliedPath ??
    (ingressRoot
      ? path.join(ingressRoot, expected.commit)
      : fallbackRoot
        ? path.join(fallbackRoot, "releases", expected.commit)
        : null);
  if (typeof artifactDir !== "string" || artifactDir.length === 0) {
    fail("invalid_invocation");
  }
  return { role, artifactDir: path.resolve(artifactDir), expected };
}

function normalizeSpecs(options, ingressRoot = null, fallbackRoot = null) {
  const currentInput =
    options.currentArtifact ??
    options.current ??
    (options.currentExpected ? { expected: options.currentExpected } : null);
  const previousInput =
    options.previousArtifact ??
    options.previous ??
    (options.previousExpected ? { expected: options.previousExpected } : null);
  if (!currentInput || !previousInput) {
    fail("invalid_invocation");
  }
  const current = normalizeArtifactSpec(
    currentInput,
    "current",
    ingressRoot,
    fallbackRoot
  );
  const previous = normalizeArtifactSpec(
    previousInput,
    "previous",
    ingressRoot,
    fallbackRoot
  );
  if (current.expected.commit === previous.expected.commit) {
    fail("bootstrap_lineage_invalid");
  }
  return { current, previous };
}

function validateLineageOverride(options, current, previous) {
  const supplied = options.lineage ?? options.expectedLineage;
  if (supplied === undefined) {
    return;
  }
  if (!isObject(supplied)) {
    fail("bootstrap_lineage_invalid");
  }
  const suppliedKeys = Object.keys(supplied).sort();
  if (
    JSON.stringify(suppliedKeys) !==
      JSON.stringify(
        ["currentCommit", "previousCommit", "rollbackReserveCommit"].sort()
      ) &&
    JSON.stringify(suppliedKeys) !==
      JSON.stringify(
        ["current_commit", "previous_commit", "rollback_reserve_commit"].sort()
      )
  ) {
    fail("bootstrap_lineage_invalid");
  }
  const requested = {
    currentCommit: supplied.currentCommit ?? supplied.current_commit,
    previousCommit: supplied.previousCommit ?? supplied.previous_commit,
    rollbackReserveCommit:
      supplied.rollbackReserveCommit ?? supplied.rollback_reserve_commit,
  };
  if (
    requested.currentCommit !== current.expected.commit ||
    requested.previousCommit !== previous.expected.commit ||
    requested.rollbackReserveCommit !== previous.expected.commit
  ) {
    fail("bootstrap_lineage_invalid");
  }
}

function validateArtifactIngressPath(artifact, ingressRoot, owner) {
  const expectedPath = path.join(ingressRoot, artifact.expected.commit);
  if (artifact.artifactDir !== path.resolve(expectedPath)) {
    fail("bootstrap_artifact_path_invalid");
  }
  assertNoSymlinkPath(ingressRoot, "bootstrap_ingress_invalid");
  requireDirectory(
    artifact.artifactDir,
    owner,
    "0555",
    `bootstrap_${artifact.role}_artifact_invalid`
  );
}

function verifyArtifactAtPath(artifact, code) {
  try {
    verifyHermesCoreArtifact({
      artifactDir: artifact.artifactDir,
      expectedTag: artifact.expected.tag,
      expectedCommit: artifact.expected.commit,
      expectedSourceSha256: artifact.expected.sourceSha256,
      expectedIdentitySha256: artifact.expected.identitySha256,
      expectedManifestSha256: artifact.expected.manifestSha256,
    });
  } catch {
    fail(code);
  }
}

function readBoundedJson(targetPath, owner, code) {
  const metadata = requireRegularFile(targetPath, owner, "0444", code);
  if (metadata.size <= 0 || metadata.size > MAX_METADATA_BYTES) {
    fail(code);
  }
  try {
    return JSON.parse(fs.readFileSync(targetPath, "utf8"));
  } catch {
    fail(code);
  }
}

function verifyCurrentReceiptBinding(releasePath, previousCommit, owner, code) {
  const receipt = readBoundedJson(
    path.join(releasePath, "update-receipt.json"),
    owner,
    code
  );
  if (!isObject(receipt) || receipt.previous_commit_sha !== previousCommit) {
    fail(code);
  }
}

function verifyIngressArtifacts({ current, previous, ingressRoot, owner }) {
  validateArtifactIngressPath(current, ingressRoot, owner);
  validateArtifactIngressPath(previous, ingressRoot, owner);
  verifyArtifactAtPath(current, "bootstrap_current_artifact_invalid");
  verifyArtifactAtPath(previous, "bootstrap_previous_artifact_invalid");
  verifyCurrentReceiptBinding(
    current.artifactDir,
    previous.expected.commit,
    owner,
    "bootstrap_current_lineage_binding_invalid"
  );
}

function fsyncPath(targetPath) {
  const descriptor = fs.openSync(targetPath, "r");
  try {
    fs.fsyncSync(descriptor);
  } finally {
    fs.closeSync(descriptor);
  }
}

function validateFreeSpace(targetPath, minimumFreeBytes) {
  if (!Number.isSafeInteger(minimumFreeBytes) || minimumFreeBytes < 0) {
    fail("invalid_invocation");
  }
  let available;
  try {
    const stats = fs.statfsSync(targetPath, { bigint: true });
    available = stats.bavail * stats.bsize;
  } catch {
    fail("bootstrap_space_invalid");
  }
  if (available < BigInt(minimumFreeBytes)) {
    fail("bootstrap_space_insufficient");
  }
}

function chownPath(targetPath, owner) {
  fs.chownSync(targetPath, owner.uid, owner.gid);
}

function chownSymlink(targetPath, owner) {
  if (typeof fs.lchownSync !== "function") {
    fail("bootstrap_symlink_owner_invalid");
  }
  fs.lchownSync(targetPath, owner.uid, owner.gid);
}

function createDirectory(targetPath, mode, owner) {
  fs.mkdirSync(targetPath, { mode });
  chownPath(targetPath, owner);
  fs.chmodSync(targetPath, Number.parseInt(mode.toString(8), 8));
}

function createEmptyFile(targetPath, mode, owner) {
  const flags =
    fs.constants.O_WRONLY |
    fs.constants.O_CREAT |
    fs.constants.O_EXCL |
    (fs.constants.O_NOFOLLOW ?? 0);
  const descriptor = fs.openSync(
    targetPath,
    flags,
    Number.parseInt(mode.toString(8), 8)
  );
  try {
    fs.fchownSync(descriptor, owner.uid, owner.gid);
    fs.fchmodSync(descriptor, Number.parseInt(mode.toString(8), 8));
    fs.fsyncSync(descriptor);
  } finally {
    fs.closeSync(descriptor);
  }
}

function writeLineageManifest(targetPath, lineage, fingerprint, owner) {
  const value = {
    schema_version: 1,
    generation_type: "hermes-core-lineage",
    fingerprint_sha256: fingerprint,
    current_commit: lineage.currentCommit,
    previous_commit: lineage.previousCommit,
    rollback_reserve_commit: lineage.rollbackReserveCommit,
  };
  const flags =
    fs.constants.O_WRONLY |
    fs.constants.O_CREAT |
    fs.constants.O_EXCL |
    (fs.constants.O_NOFOLLOW ?? 0);
  const descriptor = fs.openSync(targetPath, flags, 0o600);
  try {
    fs.writeFileSync(descriptor, `${JSON.stringify(value, null, 2)}\n`, "utf8");
    fs.fchownSync(descriptor, owner.uid, owner.gid);
    fs.fchmodSync(descriptor, 0o444);
    fs.fsyncSync(descriptor);
  } finally {
    fs.closeSync(descriptor);
  }
}

function copyArtifactTree(sourceRoot, targetRoot, owner, copyFile, sync) {
  createDirectory(targetRoot, 0o700, owner);

  const copyDirectory = (sourceDirectory, targetDirectory) => {
    let names;
    try {
      names = fs.readdirSync(sourceDirectory).sort();
    } catch {
      fail("bootstrap_pending_build_failed");
    }
    for (const name of names) {
      const sourcePath = path.join(sourceDirectory, name);
      const targetPath = path.join(targetDirectory, name);
      let metadata;
      try {
        metadata = fs.lstatSync(sourcePath);
      } catch {
        fail("bootstrap_pending_build_failed");
      }
      if (
        metadata.isSymbolicLink() ||
        !sameOwner(metadata, owner) ||
        (metadata.isDirectory() && modeOf(metadata) !== "0555") ||
        (metadata.isFile() &&
          (metadata.nlink !== 1 ||
            (modeOf(metadata) !== "0444" && modeOf(metadata) !== "0555")))
      ) {
        fail("bootstrap_pending_artifact_invalid");
      }
      if (metadata.isDirectory()) {
        createDirectory(targetPath, 0o700, owner);
        copyDirectory(sourcePath, targetPath);
        fs.chmodSync(targetPath, 0o555);
        sync(targetPath);
      } else if (metadata.isFile()) {
        copyFile(sourcePath, targetPath, fs.constants.COPYFILE_EXCL);
        chownPath(targetPath, owner);
        fs.chmodSync(targetPath, Number.parseInt(modeOf(metadata), 8));
        sync(targetPath);
      } else {
        fail("bootstrap_pending_artifact_invalid");
      }
    }
  };

  copyDirectory(sourceRoot, targetRoot);
  fs.chmodSync(targetRoot, 0o555);
  sync(targetRoot);
}

function createLineage(pendingRoot, lineage, owner, sync) {
  const lineageRoot = path.join(pendingRoot, "lineage");
  const generationsRoot = path.join(lineageRoot, "generations");
  const fingerprint = computeHermesCoreLineageFingerprint(lineage);
  const generationName = `generation-${fingerprint}`;
  const generationRoot = path.join(generationsRoot, generationName);

  createDirectory(generationRoot, 0o700, owner);
  writeLineageManifest(
    path.join(generationRoot, "lineage.json"),
    lineage,
    fingerprint,
    owner
  );
  for (const [role, property] of LINEAGE_ROLES) {
    const target = `../../../releases/${lineage[property]}`;
    const pointerPath = path.join(generationRoot, role);
    fs.symlinkSync(target, pointerPath);
    chownSymlink(pointerPath, owner);
  }
  sync(generationRoot);
  fs.chmodSync(generationRoot, 0o555);
  sync(generationRoot);

  const activePath = path.join(lineageRoot, "active");
  fs.symlinkSync(`generations/${generationName}`, activePath);
  chownSymlink(activePath, owner);
  sync(lineageRoot);
  return { fingerprint, generationName };
}

function createTopLevelPointers(pendingRoot, owner, sync) {
  const lineageRoot = path.join(pendingRoot, "lineage");
  for (const [role] of LINEAGE_ROLES) {
    const pointerPath = path.join(pendingRoot, role);
    fs.symlinkSync(`lineage/active/${role}`, pointerPath);
    chownSymlink(pointerPath, owner);
  }
  sync(lineageRoot);
}

function createPendingRoot({
  pendingRoot,
  coreParent,
  current,
  previous,
  owner,
  copyFile,
  sync,
}) {
  const lineage = {
    currentCommit: current.expected.commit,
    previousCommit: previous.expected.commit,
    rollbackReserveCommit: previous.expected.commit,
  };

  createDirectory(pendingRoot, 0o700, owner);
  createDirectory(path.join(pendingRoot, "releases"), 0o755, owner);
  createDirectory(path.join(pendingRoot, "incoming"), 0o700, owner);
  createDirectory(path.join(pendingRoot, "quarantine"), 0o700, owner);
  createDirectory(path.join(pendingRoot, "state"), 0o700, owner);
  createDirectory(path.join(pendingRoot, "state", "transactions"), 0o700, owner);
  createDirectory(path.join(pendingRoot, "lineage"), 0o755, owner);
  createDirectory(path.join(pendingRoot, "lineage", "generations"), 0o755, owner);
  createEmptyFile(path.join(pendingRoot, "state", "manager.lock"), 0o600, owner);

  copyArtifactTree(
    current.artifactDir,
    path.join(pendingRoot, "releases", current.expected.commit),
    owner,
    copyFile,
    sync
  );
  copyArtifactTree(
    previous.artifactDir,
    path.join(pendingRoot, "releases", previous.expected.commit),
    owner,
    copyFile,
    sync
  );

  verifyArtifactAtPath(
    {
      artifactDir: path.join(pendingRoot, "releases", current.expected.commit),
      expected: current.expected,
    },
    "bootstrap_current_release_invalid"
  );
  verifyArtifactAtPath(
    {
      artifactDir: path.join(pendingRoot, "releases", previous.expected.commit),
      expected: previous.expected,
    },
    "bootstrap_previous_release_invalid"
  );
  verifyCurrentReceiptBinding(
    path.join(pendingRoot, "releases", current.expected.commit),
    previous.expected.commit,
    owner,
    "bootstrap_current_lineage_binding_invalid"
  );

  const generation = createLineage(pendingRoot, lineage, owner, sync);
  createTopLevelPointers(pendingRoot, owner, sync);
  sync(path.join(pendingRoot, "releases"));
  sync(path.join(pendingRoot, "state"));
  sync(path.join(pendingRoot, "lineage", "generations"));
  sync(path.join(pendingRoot, "lineage"));
  fs.chmodSync(pendingRoot, 0o755);
  sync(pendingRoot);
  sync(coreParent);
  return generation;
}

function verifyReleaseTree(releasePath, artifact, owner, code) {
  requireDirectory(releasePath, owner, "0555", code);
  verifyArtifactAtPath({ artifactDir: releasePath, expected: artifact.expected }, code);
}

function verifyBootstrappedRootInternal({ root, current, previous, owner, code }) {
  requireDirectory(root, owner, "0755", code);
  assertExactEntries(root, ROOT_ENTRIES, code);

  const releasesRoot = path.join(root, "releases");
  requireDirectory(releasesRoot, owner, "0755", code);
  assertExactEntries(
    releasesRoot,
    [current.expected.commit, previous.expected.commit],
    code
  );
  verifyReleaseTree(
    path.join(releasesRoot, current.expected.commit),
    current,
    owner,
    code
  );
  verifyReleaseTree(
    path.join(releasesRoot, previous.expected.commit),
    previous,
    owner,
    code
  );
  verifyCurrentReceiptBinding(
    path.join(releasesRoot, current.expected.commit),
    previous.expected.commit,
    owner,
    code
  );

  for (const name of ["incoming", "quarantine"]) {
    requireDirectory(path.join(root, name), owner, "0700", code);
    assertEmptyDirectory(path.join(root, name), code);
  }
  requireDirectory(path.join(root, "state"), owner, "0700", code);
  assertExactEntries(path.join(root, "state"), ["manager.lock", "transactions"], code);
  requireDirectory(path.join(root, "state", "transactions"), owner, "0700", code);
  assertEmptyDirectory(path.join(root, "state", "transactions"), code);
  const lockMetadata = requireRegularFile(
    path.join(root, "state", "manager.lock"),
    owner,
    "0600",
    code
  );
  if (lockMetadata.size !== 0) {
    fail(code);
  }

  const lineage = {
    currentCommit: current.expected.commit,
    previousCommit: previous.expected.commit,
    rollbackReserveCommit: previous.expected.commit,
  };
  const fingerprint = computeHermesCoreLineageFingerprint(lineage);
  const generationName = `generation-${fingerprint}`;
  const lineageRoot = path.join(root, "lineage");
  requireDirectory(lineageRoot, owner, "0755", code);
  assertExactEntries(lineageRoot, ["active", "generations"], code);
  requireDirectory(path.join(lineageRoot, "generations"), owner, "0755", code);
  assertExactEntries(path.join(lineageRoot, "generations"), [generationName], code);
  requireSymlink(
    path.join(lineageRoot, "active"),
    `generations/${generationName}`,
    owner,
    code
  );

  const generationRoot = path.join(lineageRoot, "generations", generationName);
  requireDirectory(generationRoot, owner, "0555", code);
  assertExactEntries(
    generationRoot,
    [...LINEAGE_ROLES.map(([role]) => role), "lineage.json"],
    code
  );
  const manifest = readBoundedJson(
    path.join(generationRoot, "lineage.json"),
    owner,
    code
  );
  if (
    JSON.stringify(manifest) !==
    JSON.stringify({
      schema_version: 1,
      generation_type: "hermes-core-lineage",
      fingerprint_sha256: fingerprint,
      current_commit: lineage.currentCommit,
      previous_commit: lineage.previousCommit,
      rollback_reserve_commit: lineage.rollbackReserveCommit,
    })
  ) {
    fail(code);
  }
  for (const [role, property] of LINEAGE_ROLES) {
    requireSymlink(
      path.join(generationRoot, role),
      `../../../releases/${lineage[property]}`,
      owner,
      code
    );
  }
  for (const [role] of LINEAGE_ROLES) {
    requireSymlink(path.join(root, role), `lineage/active/${role}`, owner, code);
  }
  return { verified: true, fingerprint };
}

export function verifyBootstrappedHermesCoreRoot({
  root = FIXED_CORE_ROOT,
  currentArtifact,
  previousArtifact,
  current,
  previous,
  owner = ROOT_OWNER,
}) {
  const normalizedOwner = normalizeOwner(owner);
  const specs = normalizeSpecs(
    {
      currentArtifact: currentArtifact ?? current,
      previousArtifact: previousArtifact ?? previous,
    },
    null,
    path.resolve(root)
  );
  return verifyBootstrappedRootInternal({
    root: path.resolve(root),
    current: specs.current,
    previous: specs.previous,
    owner: normalizedOwner,
    code: "bootstrap_root_invalid",
  });
}

function validateLockInode({ lockPath, lockFd, owner, code }) {
  let pathMetadata;
  let descriptorMetadata;
  try {
    pathMetadata = fs.lstatSync(lockPath);
    descriptorMetadata = fs.fstatSync(lockFd);
  } catch {
    fail(code);
  }
  if (
    pathMetadata.isSymbolicLink() ||
    !pathMetadata.isFile() ||
    pathMetadata.nlink !== 1 ||
    !sameOwner(pathMetadata, owner) ||
    modeOf(pathMetadata) !== "0600" ||
    !descriptorMetadata.isFile() ||
    descriptorMetadata.nlink !== 1 ||
    !sameOwner(descriptorMetadata, owner) ||
    modeOf(descriptorMetadata) !== "0600" ||
    pathMetadata.dev !== descriptorMetadata.dev ||
    pathMetadata.ino !== descriptorMetadata.ino
  ) {
    fail(code);
  }
  return { pathMetadata, descriptorMetadata };
}

function closeLockDescriptor(lockFd) {
  if (!Number.isInteger(lockFd) || lockFd < 0) {
    return;
  }
  try {
    fs.closeSync(lockFd);
  } catch {
    // Preserve the original lock failure.
  }
}

export function openHermesCoreBootstrapLock({
  lockPath = FIXED_BOOTSTRAP_LOCK_PATH,
  trustedParent = FIXED_BOOTSTRAP_LOCK_PARENT,
  owner = ROOT_OWNER,
  syncPath = fsyncPath,
  flock,
}) {
  const normalizedOwner = normalizeOwner(owner);
  const parent = validateTrustedParent(
    trustedParent,
    normalizedOwner,
    "bootstrap_lock_parent_invalid"
  );
  const resolvedLockPath = path.resolve(lockPath);
  if (resolvedLockPath !== path.join(parent, "hermes-core-bootstrap.lock")) {
    fail("invalid_invocation");
  }

  let lockFd = null;
  let created = false;
  try {
    try {
      lockFd = fs.openSync(
        resolvedLockPath,
        BOOTSTRAP_LOCK_CREATE_FLAGS,
        BOOTSTRAP_LOCK_MODE
      );
      created = true;
    } catch (error) {
      if (error?.code !== "EEXIST") {
        fail("bootstrap_lock_invalid");
      }
      try {
        lockFd = fs.openSync(resolvedLockPath, BOOTSTRAP_LOCK_OPEN_FLAGS);
      } catch {
        fail("bootstrap_lock_invalid");
      }
    }

    if (created) {
      try {
        fs.fchownSync(lockFd, normalizedOwner.uid, normalizedOwner.gid);
        fs.fchmodSync(lockFd, BOOTSTRAP_LOCK_MODE);
        fs.fsyncSync(lockFd);
      } catch {
        fail("bootstrap_lock_invalid");
      }
    }

    validateLockInode({
      lockPath: resolvedLockPath,
      lockFd,
      owner: normalizedOwner,
      code: "bootstrap_lock_invalid",
    });

    if (created) {
      let syncFailed = false;
      for (const targetPath of [resolvedLockPath, parent]) {
        try {
          syncPath(targetPath);
        } catch {
          syncFailed = true;
        }
      }
      if (syncFailed) {
        fail("bootstrap_lock_commit_uncertain");
      }
    }

    if (typeof flock !== "function") {
      fail("bootstrap_lock_not_held");
    }
    let flockResult;
    try {
      flockResult = flock(lockFd);
    } catch {
      fail("bootstrap_lock_busy");
    }
    if (flockResult !== true) {
      fail("bootstrap_lock_busy");
    }
    validateLockInode({
      lockPath: resolvedLockPath,
      lockFd,
      owner: normalizedOwner,
      code: "bootstrap_lock_not_held",
    });
    return { fd: lockFd, created, lockPath: resolvedLockPath, parent };
  } catch (error) {
    closeLockDescriptor(lockFd);
    if (error instanceof BootstrapError) {
      throw error;
    }
    fail("bootstrap_lock_invalid");
  }
}

function validateExternalLock({
  lockPath,
  trustedParent,
  owner,
  lockHeld,
  lockFd,
  sync,
}) {
  const parent = validateTrustedParent(
    trustedParent,
    owner,
    "bootstrap_lock_parent_invalid"
  );
  const resolvedLockPath = path.resolve(lockPath);
  if (resolvedLockPath !== path.join(parent, "hermes-core-bootstrap.lock")) {
    fail("invalid_invocation");
  }
  if (lockHeld !== true) {
    fail("bootstrap_lock_not_held");
  }
  if (!Number.isInteger(lockFd) || lockFd < 0) {
    fail("bootstrap_lock_not_held");
  }
  validateLockInode({
    lockPath: resolvedLockPath,
    lockFd,
    owner,
    code: "bootstrap_lock_not_held",
  });
  let syncFailed = false;
  for (const targetPath of [resolvedLockPath, parent]) {
    try {
      sync(targetPath);
    } catch {
      syncFailed = true;
    }
  }
  if (syncFailed) {
    fail("bootstrap_lock_commit_uncertain");
  }
  return { parent, lockPath: resolvedLockPath };
}

function ensureIngressRoot({ ingressRoot, trustedParent, owner }) {
  const parent = validateTrustedParent(
    trustedParent,
    owner,
    "bootstrap_ingress_parent_invalid"
  );
  const resolvedIngressRoot = path.resolve(ingressRoot);
  if (resolvedIngressRoot !== path.join(parent, "hermes-core-ingress")) {
    fail("invalid_invocation");
  }
  assertNoSymlinkPath(resolvedIngressRoot, "bootstrap_ingress_invalid");
  requireDirectory(resolvedIngressRoot, owner, "0700", "bootstrap_ingress_invalid");
  return { parent, ingressRoot: resolvedIngressRoot };
}

function ensureQuarantineRoot({ quarantineRoot, trustedParent, owner, sync }) {
  const parent = validateTrustedParent(
    trustedParent,
    owner,
    "bootstrap_quarantine_parent_invalid"
  );
  const resolved = path.resolve(quarantineRoot);
  if (resolved !== path.join(parent, "hermes-core-bootstrap-quarantine")) {
    fail("invalid_invocation");
  }
  const metadata = lstatOrNull(resolved);
  if (!metadata) {
    try {
      createDirectory(resolved, 0o700, owner);
    } catch {
      fail("bootstrap_quarantine_invalid");
    }
  } else {
    requireDirectory(resolved, owner, "0700", "bootstrap_quarantine_invalid");
  }

  let syncFailed = false;
  for (const targetPath of [resolved, parent]) {
    try {
      sync(targetPath);
    } catch {
      syncFailed = true;
    }
  }
  if (syncFailed) {
    fail("bootstrap_quarantine_commit_uncertain");
  }
  return { parent, quarantineRoot: resolved };
}

function selectQuarantinePath(quarantineRoot, fingerprint) {
  const baseName = `failed-${fingerprint}`;
  for (let suffix = 0; suffix <= 1024; suffix += 1) {
    const name = suffix === 0 ? baseName : `${baseName}-${suffix}`;
    const candidate = path.join(quarantineRoot, name);
    if (!pathExists(candidate)) {
      return candidate;
    }
  }
  fail("bootstrap_quarantine_full");
}

function quarantinePendingRoot({
  pendingRoot,
  coreParent,
  quarantineRoot,
  quarantineParent,
  fingerprint,
  owner,
  renamePath,
  sync,
}) {
  const metadata = lstatOrNull(pendingRoot);
  if (!metadata) {
    return false;
  }
  if (
    metadata.isSymbolicLink() ||
    !metadata.isDirectory() ||
    !sameOwner(metadata, owner)
  ) {
    fail("bootstrap_pending_invalid");
  }
  const quarantinePath = selectQuarantinePath(quarantineRoot, fingerprint);
  let renamed = false;
  try {
    fs.chmodSync(pendingRoot, 0o700);
    renamePath(pendingRoot, quarantinePath);
    renamed = true;
    sync(coreParent);
    sync(quarantineRoot);
    sync(quarantineParent);
  } catch {
    if (renamed) {
      fail("bootstrap_quarantine_commit_uncertain");
    }
    fail("bootstrap_quarantine_failed");
  }
  return true;
}

function quarantineAfterFailure({
  pendingRoot,
  coreParent,
  quarantineRoot,
  quarantineParent,
  fingerprint,
  owner,
  renamePath,
  sync,
  originalCode,
}) {
  let quarantined = false;
  try {
    quarantined = quarantinePendingRoot({
      pendingRoot,
      coreParent,
      quarantineRoot,
      quarantineParent,
      fingerprint,
      owner,
      renamePath,
      sync,
    });
  } catch (error) {
    if (error instanceof BootstrapError) {
      throw error;
    }
    fail("bootstrap_quarantine_failed");
  }
  if (pathExists(pendingRoot) && !quarantined) {
    fail("bootstrap_quarantine_failed");
  }
  fail(originalCode);
}

function inspectRootState(root) {
  try {
    return fs.lstatSync(root);
  } catch (error) {
    if (error?.code === "ENOENT") {
      return null;
    }
    fail("bootstrap_root_invalid");
  }
}

function makeLineageFingerprint(current, previous) {
  return computeHermesCoreLineageFingerprint({
    currentCommit: current.expected.commit,
    previousCommit: previous.expected.commit,
    rollbackReserveCommit: previous.expected.commit,
  });
}

export function bootstrapHermesCoreRoot({
  currentArtifact,
  previousArtifact,
  current,
  previous,
  lineage,
  coreRoot = FIXED_CORE_ROOT,
  coreTrustedParent = FIXED_CORE_PARENT,
  ingressRoot = FIXED_INGRESS_ROOT,
  ingressTrustedParent = FIXED_INGRESS_PARENT,
  bootstrapLockPath = FIXED_BOOTSTRAP_LOCK_PATH,
  bootstrapLockTrustedParent = FIXED_BOOTSTRAP_LOCK_PARENT,
  quarantineRoot = FIXED_BOOTSTRAP_QUARANTINE,
  quarantineTrustedParent = FIXED_BOOTSTRAP_QUARANTINE_PARENT,
  owner = ROOT_OWNER,
  minimumFreeBytes = MIN_FREE_BYTES,
  lockHeld = false,
  lockFd = null,
  copyFile = fs.copyFileSync,
  renamePath = fs.renameSync,
  syncPath = fsyncPath,
}) {
  const normalizedOwner = normalizeOwner(owner);
  const resolvedCoreParent = validateTrustedParent(
    coreTrustedParent,
    normalizedOwner,
    "bootstrap_core_parent_invalid"
  );
  const resolvedCoreRoot = path.resolve(coreRoot);
  if (resolvedCoreRoot !== path.join(resolvedCoreParent, "qintopia-hermes-core")) {
    fail("invalid_invocation");
  }
  validateExternalLock({
    lockPath: bootstrapLockPath,
    trustedParent: bootstrapLockTrustedParent,
    owner: normalizedOwner,
    lockHeld,
    lockFd,
    sync: syncPath,
  });
  const resolvedQuarantine = path.resolve(quarantineRoot);
  if (
    resolvedQuarantine !==
    path.join(path.resolve(quarantineTrustedParent), "hermes-core-bootstrap-quarantine")
  ) {
    fail("invalid_invocation");
  }
  const specs = normalizeSpecs(
    {
      currentArtifact: currentArtifact ?? current,
      previousArtifact: previousArtifact ?? previous,
    },
    path.resolve(ingressRoot)
  );
  validateLineageOverride({ lineage }, specs.current, specs.previous);
  const fingerprint = makeLineageFingerprint(specs.current, specs.previous);
  const pendingRoot = path.join(resolvedCoreParent, FIXED_PENDING_ROOT_NAME);
  const rootMetadata = inspectRootState(resolvedCoreRoot);

  if (rootMetadata) {
    if (rootMetadata.isSymbolicLink() || !rootMetadata.isDirectory()) {
      fail("bootstrap_root_invalid");
    }
    verifyBootstrappedRootInternal({
      root: resolvedCoreRoot,
      current: specs.current,
      previous: specs.previous,
      owner: normalizedOwner,
      code: "bootstrap_root_invalid",
    });
    try {
      syncPath(resolvedCoreRoot);
      syncPath(resolvedCoreParent);
    } catch {
      fail("bootstrap_commit_uncertain");
    }
    return {
      status: "already_bootstrapped",
      alreadyBootstrapped: true,
      pointerChanges: 0,
      serviceChanges: 0,
      legacyCheckoutTouched: false,
      externalLock: true,
    };
  }

  const ingress = ensureIngressRoot({
    ingressRoot,
    trustedParent: ingressTrustedParent,
    owner: normalizedOwner,
  });
  verifyIngressArtifacts({
    current: specs.current,
    previous: specs.previous,
    ingressRoot: ingress.ingressRoot,
    owner: normalizedOwner,
  });
  validateFreeSpace(resolvedCoreParent, minimumFreeBytes);
  const quarantine = ensureQuarantineRoot({
    quarantineRoot,
    trustedParent: quarantineTrustedParent,
    owner: normalizedOwner,
    sync: syncPath,
  });

  const pendingMetadata = lstatOrNull(pendingRoot);
  if (pendingMetadata) {
    if (pendingMetadata.isSymbolicLink() || !pendingMetadata.isDirectory()) {
      fail("bootstrap_pending_invalid");
    }
    try {
      verifyBootstrappedRootInternal({
        root: pendingRoot,
        current: specs.current,
        previous: specs.previous,
        owner: normalizedOwner,
        code: "bootstrap_pending_invalid",
      });
    } catch (error) {
      if (!(error instanceof BootstrapError)) {
        fail("bootstrap_pending_invalid");
      }
      quarantineAfterFailure({
        pendingRoot,
        coreParent: resolvedCoreParent,
        quarantineRoot: quarantine.quarantineRoot,
        quarantineParent: quarantine.parent,
        fingerprint,
        owner: normalizedOwner,
        renamePath,
        sync: syncPath,
        originalCode: "bootstrap_pending_invalid",
      });
    }
  } else {
    try {
      createPendingRoot({
        pendingRoot,
        coreParent: resolvedCoreParent,
        current: specs.current,
        previous: specs.previous,
        owner: normalizedOwner,
        copyFile,
        sync: syncPath,
      });
    } catch (error) {
      if (error instanceof BootstrapError) {
        quarantineAfterFailure({
          pendingRoot,
          coreParent: resolvedCoreParent,
          quarantineRoot: quarantine.quarantineRoot,
          quarantineParent: quarantine.parent,
          fingerprint,
          owner: normalizedOwner,
          renamePath,
          sync: syncPath,
          originalCode: error.code,
        });
      }
      quarantineAfterFailure({
        pendingRoot,
        coreParent: resolvedCoreParent,
        quarantineRoot: quarantine.quarantineRoot,
        quarantineParent: quarantine.parent,
        fingerprint,
        owner: normalizedOwner,
        renamePath,
        sync: syncPath,
        originalCode: "bootstrap_prepare_failed",
      });
    }
  }

  let renamed = false;
  try {
    renamePath(pendingRoot, resolvedCoreRoot);
    renamed = true;
  } catch {
    if (!pathExists(pendingRoot) && pathExists(resolvedCoreRoot)) {
      try {
        verifyBootstrappedRootInternal({
          root: resolvedCoreRoot,
          current: specs.current,
          previous: specs.previous,
          owner: normalizedOwner,
          code: "bootstrap_root_invalid",
        });
        syncPath(resolvedCoreRoot);
        syncPath(resolvedCoreParent);
        fail("bootstrap_commit_uncertain");
      } catch (error) {
        if (
          error instanceof BootstrapError &&
          error.code === "bootstrap_commit_uncertain"
        ) {
          throw error;
        }
        if (!(error instanceof BootstrapError)) {
          fail("bootstrap_commit_uncertain");
        }
      }
    }
    quarantineAfterFailure({
      pendingRoot,
      coreParent: resolvedCoreParent,
      quarantineRoot: quarantine.quarantineRoot,
      quarantineParent: quarantine.parent,
      fingerprint,
      owner: normalizedOwner,
      renamePath,
      sync: syncPath,
      originalCode: "bootstrap_rename_failed",
    });
  }
  if (!renamed) {
    fail("bootstrap_rename_failed");
  }
  try {
    syncPath(resolvedCoreParent);
  } catch {
    fail("bootstrap_commit_uncertain");
  }
  return {
    status: "bootstrapped",
    alreadyBootstrapped: false,
    pointerChanges: 0,
    serviceChanges: 0,
    legacyCheckoutTouched: false,
    externalLock: true,
  };
}

function parseArguments(argv) {
  const optionNames = [
    "current-tag",
    "current-commit",
    "current-source-sha256",
    "current-identity-sha256",
    "current-manifest-sha256",
    "previous-tag",
    "previous-commit",
    "previous-source-sha256",
    "previous-identity-sha256",
    "previous-manifest-sha256",
  ];
  const allowed = new Set(optionNames.map((name) => `--${name}`));
  const values = new Map();
  let bootstrapSeen = false;
  for (let index = 0; index < argv.length;) {
    const key = argv[index];
    if (key === "--bootstrap") {
      if (bootstrapSeen) {
        fail("invalid_invocation");
      }
      bootstrapSeen = true;
      index += 1;
      continue;
    }
    if (!allowed.has(key) || values.has(key) || !argv[index + 1]) {
      fail("invalid_invocation");
    }
    values.set(key, argv[index + 1]);
    index += 2;
  }
  if (!bootstrapSeen || values.size !== optionNames.length) {
    fail("invalid_invocation");
  }
  const expected = (role) => ({
    tag: values.get(`--${role}-tag`),
    commit: values.get(`--${role}-commit`),
    sourceSha256: values.get(`--${role}-source-sha256`),
    identitySha256: values.get(`--${role}-identity-sha256`),
    manifestSha256: values.get(`--${role}-manifest-sha256`),
  });
  return { current: expected("current"), previous: expected("previous") };
}

function main() {
  try {
    const input = parseArguments(process.argv.slice(2));
    const result = bootstrapHermesCoreRoot({
      currentArtifact: {
        artifactDir: path.join(FIXED_INGRESS_ROOT, input.current.commit),
        expected: input.current,
      },
      previousArtifact: {
        artifactDir: path.join(FIXED_INGRESS_ROOT, input.previous.commit),
        expected: input.previous,
      },
      lockHeld: process.env.QINTOPIA_HERMES_CORE_BOOTSTRAP_LOCK_HELD === "1",
      lockFd: 9,
    });
    console.log("hermes_core_bootstrap=ready");
    console.log(`hermes_core_bootstrap_status=${result.status}`);
  } catch (error) {
    const code =
      error instanceof BootstrapError && SAFE_ERROR_PATTERN.test(error.code)
        ? error.code
        : "hermes_core_bootstrap_invalid";
    console.error("hermes_core_bootstrap=blocked");
    console.error(`hermes_core_bootstrap_error=${code}`);
    process.exitCode = code === "invalid_invocation" ? 2 : 1;
  }
}

if (
  process.argv[1] &&
  path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url))
) {
  main();
}
