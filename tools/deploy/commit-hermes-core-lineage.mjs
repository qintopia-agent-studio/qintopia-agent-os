#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import {
  computeHermesCoreLineageFingerprint,
  planHermesCoreRelease,
  verifyHermesCoreLineage,
  verifyHermesCoreManagerBoundary,
  verifyHermesCoreManagerLockBoundary,
} from "./plan-hermes-core-release.mjs";
import { verifyHermesCoreArtifact } from "./verify-hermes-core-artifact.mjs";

const FIXED_CORE_ROOT = "/var/lib/qintopia-hermes-core";
const FIXED_TRUSTED_PARENT = "/var/lib";
const MIN_FREE_BYTES = 5 * 1024 * 1024 * 1024;
const MANIFEST_LIMIT = 8 * 1024 * 1024;
const COMMIT_PATTERN = /^[0-9a-f]{40}$/;
const SHA256_PATTERN = /^[0-9a-f]{64}$/;
const TAG_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/;
const LINEAGE_ROLES = Object.freeze([
  ["current", "currentCommit"],
  ["previous", "previousCommit"],
  ["rollback-reserve", "rollbackReserveCommit"],
]);

class LineageCommitError extends Error {
  constructor(code) {
    super(code);
    this.name = "LineageCommitError";
  }
}

const fail = (code) => {
  throw new LineageCommitError(code);
};

const modeOf = (metadata) => (metadata.mode & 0o7777).toString(8).padStart(4, "0");

const sameOwner = (metadata, owner) =>
  metadata.uid === owner.uid && metadata.gid === owner.gid;

function entryExists(entryPath) {
  try {
    fs.lstatSync(entryPath);
    return true;
  } catch (error) {
    if (error?.code === "ENOENT") {
      return false;
    }
    fail("filesystem_state_invalid");
  }
}

function validateExpected(expected) {
  if (
    !TAG_PATTERN.test(expected.tag) ||
    !COMMIT_PATTERN.test(expected.commit) ||
    !SHA256_PATTERN.test(expected.sourceSha256) ||
    !SHA256_PATTERN.test(expected.identitySha256) ||
    !SHA256_PATTERN.test(expected.manifestSha256) ||
    !COMMIT_PATTERN.test(expected.currentCommit) ||
    !COMMIT_PATTERN.test(expected.previousCommit) ||
    !COMMIT_PATTERN.test(expected.rollbackReserveCommit)
  ) {
    fail("invalid_invocation");
  }
  if (
    expected.commit === expected.currentCommit ||
    expected.commit === expected.previousCommit ||
    expected.commit === expected.rollbackReserveCommit ||
    expected.currentCommit === expected.previousCommit
  ) {
    fail("lineage_identity_invalid");
  }
}

function validateLineage(lineage, code) {
  if (
    !lineage ||
    !COMMIT_PATTERN.test(lineage.currentCommit) ||
    !COMMIT_PATTERN.test(lineage.previousCommit) ||
    !COMMIT_PATTERN.test(lineage.rollbackReserveCommit) ||
    lineage.currentCommit === lineage.previousCommit
  ) {
    fail(code);
  }
}

function validateRestoreRelationship(fromLineage, toLineage) {
  validateLineage(fromLineage, "lineage_rollback_identity_invalid");
  validateLineage(toLineage, "lineage_rollback_identity_invalid");
  if (
    fromLineage.previousCommit !== toLineage.currentCommit ||
    fromLineage.rollbackReserveCommit !== toLineage.previousCommit ||
    fromLineage.currentCommit === toLineage.currentCommit ||
    fromLineage.currentCommit === toLineage.previousCommit ||
    fromLineage.currentCommit === toLineage.rollbackReserveCommit
  ) {
    fail("lineage_rollback_identity_invalid");
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

function verifyCandidateLineageBinding(artifactPath, expectedPreviousCommit) {
  let receipt;
  try {
    receipt = JSON.parse(
      fs.readFileSync(path.join(artifactPath, "update-receipt.json"), "utf8")
    );
  } catch {
    fail("candidate_lineage_binding_invalid");
  }
  if (receipt.previous_commit_sha !== expectedPreviousCommit) {
    fail("candidate_lineage_binding_invalid");
  }
}

function verifyRecoverableCandidate(
  candidatePath,
  expected,
  expectedPreviousCommit,
  owner
) {
  let metadata;
  try {
    metadata = fs.lstatSync(candidatePath);
  } catch {
    fail("candidate_location_invalid");
  }
  if (
    metadata.isSymbolicLink() ||
    !metadata.isDirectory() ||
    !sameOwner(metadata, owner)
  ) {
    fail("candidate_location_invalid");
  }
  const mode = modeOf(metadata);
  let normalized = false;
  if (mode === "0700") {
    fs.chmodSync(candidatePath, 0o555);
    normalized = true;
  } else if (mode !== "0555") {
    fail("candidate_location_invalid");
  }
  try {
    verifyArtifact(candidatePath, expected, "release_artifact_invalid");
    verifyCandidateLineageBinding(candidatePath, expectedPreviousCommit);
  } catch (error) {
    if (normalized) {
      try {
        fs.chmodSync(candidatePath, 0o700);
      } catch {
        fail("candidate_mode_recovery_failed");
      }
    }
    throw error;
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

function fsyncVisibleEntry(entryPath, parentPath, errorCode, syncPath) {
  try {
    syncPath(entryPath);
    syncPath(parentPath);
  } catch {
    fail(errorCode);
  }
}

function writeLineageManifest(filePath, lineage, fingerprint, owner) {
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
  const descriptor = fs.openSync(filePath, flags, 0o600);
  try {
    fs.writeFileSync(descriptor, `${JSON.stringify(value, null, 2)}\n`, "utf8");
    fs.fchownSync(descriptor, owner.uid, owner.gid);
    fs.fchmodSync(descriptor, 0o444);
    fs.fsyncSync(descriptor);
  } finally {
    fs.closeSync(descriptor);
  }
}

function requireGeneration(generationPath, lineage, owner) {
  let metadata;
  let entries;
  try {
    metadata = fs.lstatSync(generationPath);
    entries = fs.readdirSync(generationPath).sort();
  } catch {
    fail("lineage_generation_invalid");
  }
  if (
    metadata.isSymbolicLink() ||
    !metadata.isDirectory() ||
    !sameOwner(metadata, owner) ||
    modeOf(metadata) !== "0555" ||
    JSON.stringify(entries) !==
      JSON.stringify([...LINEAGE_ROLES.map(([role]) => role), "lineage.json"].sort())
  ) {
    fail("lineage_generation_invalid");
  }

  const fingerprint = computeHermesCoreLineageFingerprint(lineage);
  const manifestPath = path.join(generationPath, "lineage.json");
  let manifestMetadata;
  let manifest;
  try {
    manifestMetadata = fs.lstatSync(manifestPath);
    manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
  } catch {
    fail("lineage_generation_invalid");
  }
  if (
    manifestMetadata.isSymbolicLink() ||
    !manifestMetadata.isFile() ||
    manifestMetadata.nlink !== 1 ||
    !sameOwner(manifestMetadata, owner) ||
    modeOf(manifestMetadata) !== "0444" ||
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
    fail("lineage_generation_invalid");
  }

  for (const [role, property] of LINEAGE_ROLES) {
    const pointerPath = path.join(generationPath, role);
    let pointerMetadata;
    let pointerTarget;
    try {
      pointerMetadata = fs.lstatSync(pointerPath);
      pointerTarget = fs.readlinkSync(pointerPath);
    } catch {
      fail("lineage_generation_invalid");
    }
    if (
      !pointerMetadata.isSymbolicLink() ||
      !sameOwner(pointerMetadata, owner) ||
      pointerTarget !== `../../../releases/${lineage[property]}`
    ) {
      fail("lineage_generation_invalid");
    }
  }
}

function rollbackArtifactExpected(releasePath, owner) {
  const manifestPath = path.join(releasePath, "artifact-manifest.json");
  let metadata;
  let bytes;
  let manifest;
  try {
    metadata = fs.lstatSync(manifestPath);
    if (
      metadata.isSymbolicLink() ||
      !metadata.isFile() ||
      metadata.nlink !== 1 ||
      !sameOwner(metadata, owner) ||
      modeOf(metadata) !== "0444" ||
      metadata.size <= 0 ||
      metadata.size > MANIFEST_LIMIT
    ) {
      fail("lineage_rollback_target_invalid");
    }
    bytes = fs.readFileSync(manifestPath);
    manifest = JSON.parse(bytes.toString("utf8"));
  } catch (error) {
    if (error instanceof LineageCommitError) {
      throw error;
    }
    fail("lineage_rollback_target_invalid");
  }
  if (
    !manifest ||
    typeof manifest !== "object" ||
    Array.isArray(manifest) ||
    !TAG_PATTERN.test(manifest.tag) ||
    !COMMIT_PATTERN.test(manifest.commit_sha) ||
    !SHA256_PATTERN.test(manifest.source_archive_sha256) ||
    !SHA256_PATTERN.test(manifest.artifact_identity_sha256)
  ) {
    fail("lineage_rollback_target_invalid");
  }
  return {
    tag: manifest.tag,
    commit: manifest.commit_sha,
    sourceSha256: manifest.source_archive_sha256,
    identitySha256: manifest.artifact_identity_sha256,
    manifestSha256: crypto.createHash("sha256").update(bytes).digest("hex"),
  };
}

function verifyRollbackTarget(coreRoot, lineage, owner) {
  const fingerprint = computeHermesCoreLineageFingerprint(lineage);
  const generationPath = path.join(
    coreRoot,
    "lineage",
    "generations",
    `generation-${fingerprint}`
  );
  try {
    requireGeneration(generationPath, lineage, owner);
    const verifiedReleases = new Set();
    for (const [, property] of LINEAGE_ROLES) {
      const commit = lineage[property];
      const releasePath = path.join(coreRoot, "releases", commit);
      let metadata;
      try {
        metadata = fs.lstatSync(releasePath);
      } catch {
        fail("lineage_rollback_target_invalid");
      }
      if (
        metadata.isSymbolicLink() ||
        !metadata.isDirectory() ||
        !sameOwner(metadata, owner) ||
        modeOf(metadata) !== "0555"
      ) {
        fail("lineage_rollback_target_invalid");
      }
      if (verifiedReleases.has(releasePath)) {
        continue;
      }
      const expected = rollbackArtifactExpected(releasePath, owner);
      if (expected.commit !== commit) {
        fail("lineage_rollback_target_invalid");
      }
      verifyArtifact(releasePath, expected, "lineage_rollback_target_invalid");
      verifiedReleases.add(releasePath);
    }
  } catch (error) {
    if (
      error instanceof LineageCommitError &&
      error.message === "lineage_rollback_target_invalid"
    ) {
      throw error;
    }
    fail("lineage_rollback_target_invalid");
  }
}

function buildGeneration(pendingPath, lineage, fingerprint, owner, syncPath) {
  fs.mkdirSync(pendingPath, { mode: 0o700 });
  fs.chownSync(pendingPath, owner.uid, owner.gid);
  writeLineageManifest(
    path.join(pendingPath, "lineage.json"),
    lineage,
    fingerprint,
    owner
  );
  for (const [role, property] of LINEAGE_ROLES) {
    fs.symlinkSync(
      `../../../releases/${lineage[property]}`,
      path.join(pendingPath, role)
    );
  }
  syncPath(pendingPath);
  fs.chmodSync(pendingPath, 0o555);
  syncPath(pendingPath);
}

function quarantinePendingGeneration(
  coreRoot,
  generationsRoot,
  pendingPath,
  fingerprint,
  syncPath
) {
  if (!entryExists(pendingPath)) {
    return;
  }
  const metadata = fs.lstatSync(pendingPath);
  if (metadata.isSymbolicLink() || !metadata.isDirectory()) {
    fail("generation_recovery_invalid");
  }
  const quarantineRoot = path.join(coreRoot, "quarantine");
  const quarantinePath = path.join(
    quarantineRoot,
    `lineage-failed-${fingerprint}-${crypto.randomBytes(8).toString("hex")}`
  );
  fs.chmodSync(pendingPath, 0o700);
  let renamed = false;
  try {
    fs.renameSync(pendingPath, quarantinePath);
    renamed = true;
    syncPath(generationsRoot);
    syncPath(quarantineRoot);
  } catch (error) {
    if (renamed) {
      fail("generation_quarantine_commit_uncertain");
    }
    throw error;
  }
}

function ensureGeneration(coreRoot, lineage, owner, syncPath) {
  const fingerprint = computeHermesCoreLineageFingerprint(lineage);
  const generationsRoot = path.join(coreRoot, "lineage", "generations");
  const generationName = `generation-${fingerprint}`;
  const generationPath = path.join(generationsRoot, generationName);
  const pendingPath = path.join(generationsRoot, `.${generationName}.pending`);

  if (entryExists(generationPath)) {
    requireGeneration(generationPath, lineage, owner);
    fsyncVisibleEntry(
      generationPath,
      generationsRoot,
      "generation_commit_uncertain",
      syncPath
    );
    return { fingerprint, generationName, recovered: true };
  }

  if (entryExists(pendingPath)) {
    try {
      requireGeneration(pendingPath, lineage, owner);
      fs.renameSync(pendingPath, generationPath);
      fsyncVisibleEntry(
        generationPath,
        generationsRoot,
        "generation_commit_uncertain",
        syncPath
      );
      return { fingerprint, generationName, recovered: true };
    } catch (error) {
      if (
        error instanceof LineageCommitError &&
        error.message !== "lineage_generation_invalid"
      ) {
        throw error;
      }
      try {
        quarantinePendingGeneration(
          coreRoot,
          generationsRoot,
          pendingPath,
          fingerprint,
          syncPath
        );
      } catch (quarantineError) {
        if (
          quarantineError instanceof LineageCommitError &&
          quarantineError.message === "generation_quarantine_commit_uncertain"
        ) {
          throw quarantineError;
        }
        fail("generation_quarantine_failed");
      }
    }
  }

  let renamed = false;
  try {
    buildGeneration(pendingPath, lineage, fingerprint, owner, syncPath);
    requireGeneration(pendingPath, lineage, owner);
    fs.renameSync(pendingPath, generationPath);
    renamed = true;
    fsyncVisibleEntry(
      generationPath,
      generationsRoot,
      "generation_commit_uncertain",
      syncPath
    );
  } catch (error) {
    if (renamed) {
      fail("generation_commit_uncertain");
    }
    try {
      quarantinePendingGeneration(
        coreRoot,
        generationsRoot,
        pendingPath,
        fingerprint,
        syncPath
      );
    } catch (quarantineError) {
      if (
        quarantineError instanceof LineageCommitError &&
        quarantineError.message === "generation_quarantine_commit_uncertain"
      ) {
        throw quarantineError;
      }
      fail("generation_quarantine_failed");
    }
    if (error instanceof LineageCommitError) {
      throw error;
    }
    fail("generation_create_failed");
  }
  return { fingerprint, generationName, recovered: false };
}

function clearPreparedActivePointer(
  lineageRoot,
  nextGenerationTarget,
  owner,
  syncPath
) {
  const pendingPath = path.join(lineageRoot, ".active.pending");
  if (!entryExists(pendingPath)) {
    return false;
  }
  let metadata;
  let target;
  try {
    metadata = fs.lstatSync(pendingPath);
    target = fs.readlinkSync(pendingPath);
  } catch {
    fail("lineage_recovery_invalid");
  }
  if (
    !metadata.isSymbolicLink() ||
    !sameOwner(metadata, owner) ||
    target !== nextGenerationTarget
  ) {
    fail("lineage_recovery_invalid");
  }
  fs.unlinkSync(pendingPath);
  try {
    syncPath(lineageRoot);
  } catch {
    fail("lineage_recovery_commit_uncertain");
  }
  return true;
}

function switchActivePointer(lineageRoot, nextGenerationTarget, syncPath) {
  const pendingPath = path.join(lineageRoot, ".active.pending");
  const activePath = path.join(lineageRoot, "active");
  let renamed = false;
  try {
    fs.symlinkSync(nextGenerationTarget, pendingPath);
    syncPath(lineageRoot);
    fs.renameSync(pendingPath, activePath);
    renamed = true;
    fsyncVisibleEntry(activePath, lineageRoot, "lineage_commit_uncertain", syncPath);
  } catch {
    if (renamed) {
      fail("lineage_commit_uncertain");
    }
    fail("lineage_prepare_failed");
  }
}

function activeTarget(coreRoot, owner) {
  try {
    const activePath = path.join(coreRoot, "lineage", "active");
    const metadata = fs.lstatSync(activePath);
    if (!metadata.isSymbolicLink() || !sameOwner(metadata, owner)) {
      fail("lineage_pointer_invalid");
    }
    return fs.readlinkSync(activePath);
  } catch (error) {
    if (error instanceof LineageCommitError) {
      throw error;
    }
    fail("lineage_pointer_invalid");
  }
}

function commitHermesCoreLineageInternal({
  expected,
  coreRoot = FIXED_CORE_ROOT,
  trustedParent = FIXED_TRUSTED_PARENT,
  owner = { uid: 0, gid: 0 },
  minimumFreeBytes = MIN_FREE_BYTES,
  lockHeld = false,
  lockFd = null,
  verifyInheritedLock = true,
  syncPath = fsyncPath,
}) {
  validateExpected(expected);
  const oldLineage = {
    currentCommit: expected.currentCommit,
    previousCommit: expected.previousCommit,
    rollbackReserveCommit: expected.rollbackReserveCommit,
  };
  const nextLineage = {
    currentCommit: expected.commit,
    previousCommit: expected.currentCommit,
    rollbackReserveCommit: expected.previousCommit,
  };
  const nextFingerprint = computeHermesCoreLineageFingerprint(nextLineage);
  const nextGenerationName = `generation-${nextFingerprint}`;
  const nextGenerationTarget = `generations/${nextGenerationName}`;

  const resolvedRoot = verifyHermesCoreManagerLockBoundary({
    coreRoot,
    trustedParent,
    owner,
    lockHeld,
    lockFd,
    verifyInheritedLock,
  });
  const recoveryReleasePath = path.join(resolvedRoot, "releases", expected.commit);
  const observedActiveTarget = activeTarget(resolvedRoot, owner);
  if (fs.existsSync(recoveryReleasePath)) {
    try {
      verifyRecoverableCandidate(
        recoveryReleasePath,
        expected,
        expected.currentCommit,
        owner
      );
      fsyncVisibleEntry(
        recoveryReleasePath,
        path.join(resolvedRoot, "releases"),
        "release_install_commit_uncertain",
        syncPath
      );
    } catch (error) {
      if (observedActiveTarget === nextGenerationTarget) {
        fail("lineage_commit_uncertain");
      }
      throw error;
    }
  }
  verifyHermesCoreManagerBoundary({
    coreRoot: resolvedRoot,
    trustedParent,
    owner,
    minimumFreeBytes,
    lockHeld,
    lockFd,
    verifyInheritedLock,
  });

  if (observedActiveTarget === nextGenerationTarget) {
    let verified;
    try {
      fsyncVisibleEntry(
        path.join(resolvedRoot, "lineage", "active"),
        path.join(resolvedRoot, "lineage"),
        "lineage_commit_uncertain",
        syncPath
      );
      verified = verifyHermesCoreLineage({
        expectedLineage: nextLineage,
        coreRoot: resolvedRoot,
        trustedParent,
        owner,
        minimumFreeBytes,
        lockHeld,
        lockFd,
        verifyInheritedLock,
      });
      verifyRecoverableCandidate(
        path.join(resolvedRoot, "releases", expected.commit),
        expected,
        expected.currentCommit,
        owner
      );
    } catch {
      fail("lineage_commit_uncertain");
    }
    return {
      ...verified,
      candidateCommit: expected.commit,
      alreadyCommitted: true,
      recovered: true,
      pointerChanges: 0,
      serviceChanges: 0,
    };
  }

  const lineageRoot = path.join(resolvedRoot, "lineage");
  const preparedPointerRemoved = clearPreparedActivePointer(
    lineageRoot,
    nextGenerationTarget,
    owner,
    syncPath
  );
  const verifiedOld = verifyHermesCoreLineage({
    expectedLineage: oldLineage,
    coreRoot: resolvedRoot,
    trustedParent,
    owner,
    minimumFreeBytes,
    lockHeld,
    lockFd,
    verifyInheritedLock,
  });

  const incomingPath = path.join(resolvedRoot, "incoming", expected.commit);
  const releasePath = path.join(resolvedRoot, "releases", expected.commit);
  const incomingExists = fs.existsSync(incomingPath);
  const releaseExists = fs.existsSync(releasePath);
  if (incomingExists === releaseExists) {
    fail("candidate_location_invalid");
  }

  let releaseRecovered = false;
  if (incomingExists) {
    verifyRecoverableCandidate(incomingPath, expected, expected.currentCommit, owner);
    planHermesCoreRelease({
      expected,
      coreRoot: resolvedRoot,
      trustedParent,
      owner,
      minimumFreeBytes,
      lockHeld,
      lockFd,
      verifyInheritedLock,
    });
    try {
      fs.chmodSync(incomingPath, 0o700);
      if (entryExists(releasePath)) {
        fs.chmodSync(incomingPath, 0o555);
        fail("candidate_location_invalid");
      }
      fs.renameSync(incomingPath, releasePath);
      fs.chmodSync(releasePath, 0o555);
      syncPath(releasePath);
      syncPath(path.join(resolvedRoot, "incoming"));
      syncPath(path.join(resolvedRoot, "releases"));
    } catch (error) {
      if (fs.existsSync(releasePath)) {
        try {
          fs.chmodSync(releasePath, 0o555);
        } catch {
          fail("release_install_commit_uncertain");
        }
        fail("release_install_commit_uncertain");
      }
      try {
        fs.chmodSync(incomingPath, 0o555);
      } catch {
        fail("release_install_failed");
      }
      if (error instanceof LineageCommitError) {
        throw error;
      }
      fail("release_install_failed");
    }
  } else {
    verifyRecoverableCandidate(releasePath, expected, expected.currentCommit, owner);
    if (fs.readdirSync(path.join(resolvedRoot, "incoming")).length !== 0) {
      fail("candidate_location_invalid");
    }
    releaseRecovered = true;
  }

  if (modeOf(fs.lstatSync(releasePath)) !== "0555") {
    fail("release_install_commit_uncertain");
  }

  verifyArtifact(releasePath, expected, "release_artifact_invalid");
  verifyCandidateLineageBinding(releasePath, expected.currentCommit);
  const generation = ensureGeneration(resolvedRoot, nextLineage, owner, syncPath);
  switchActivePointer(lineageRoot, nextGenerationTarget, syncPath);

  let verifiedNext;
  try {
    verifiedNext = verifyHermesCoreLineage({
      expectedLineage: nextLineage,
      coreRoot: resolvedRoot,
      trustedParent,
      owner,
      minimumFreeBytes,
      lockHeld,
      lockFd,
      verifyInheritedLock,
    });
    verifyArtifact(releasePath, expected, "release_artifact_invalid");
    verifyCandidateLineageBinding(releasePath, expected.currentCommit);
  } catch {
    fail("lineage_commit_uncertain");
  }

  return {
    ...verifiedNext,
    candidateCommit: expected.commit,
    originalLineageFingerprint: verifiedOld.lineageFingerprint,
    alreadyCommitted: false,
    recovered: preparedPointerRemoved || releaseRecovered || generation.recovered,
    pointerChanges: 1,
    serviceChanges: 0,
  };
}

function restoreHermesCoreLineageInternal({
  fromLineage,
  toLineage,
  coreRoot = FIXED_CORE_ROOT,
  trustedParent = FIXED_TRUSTED_PARENT,
  owner = { uid: 0, gid: 0 },
  lockHeld = false,
  lockFd = null,
  verifyInheritedLock = true,
  syncPath = fsyncPath,
}) {
  validateRestoreRelationship(fromLineage, toLineage);
  const resolvedRoot = verifyHermesCoreManagerLockBoundary({
    coreRoot,
    trustedParent,
    owner,
    lockHeld,
    lockFd,
    verifyInheritedLock,
  });
  const lineageRoot = path.join(resolvedRoot, "lineage");
  const fromFingerprint = computeHermesCoreLineageFingerprint(fromLineage);
  const toFingerprint = computeHermesCoreLineageFingerprint(toLineage);
  const fromTarget = `generations/generation-${fromFingerprint}`;
  const toTarget = `generations/generation-${toFingerprint}`;
  const observedTarget = activeTarget(resolvedRoot, owner);

  if (observedTarget === toTarget) {
    let recovered;
    let verified;
    try {
      fsyncVisibleEntry(
        path.join(lineageRoot, "active"),
        lineageRoot,
        "lineage_rollback_commit_uncertain",
        syncPath
      );
      recovered = clearPreparedActivePointer(lineageRoot, fromTarget, owner, syncPath);
      verified = verifyHermesCoreLineage({
        expectedLineage: toLineage,
        coreRoot: resolvedRoot,
        trustedParent,
        owner,
        minimumFreeBytes: 0,
        lockHeld,
        lockFd,
        verifyInheritedLock,
      });
    } catch {
      fail("lineage_rollback_commit_uncertain");
    }
    return {
      ...verified,
      restoredFromCommit: fromLineage.currentCommit,
      alreadyRestored: true,
      recovered,
      pointerChanges: 0,
      serviceChanges: 0,
    };
  }
  if (observedTarget !== fromTarget) {
    fail("lineage_rollback_pointer_mismatch");
  }

  clearPreparedActivePointer(lineageRoot, toTarget, owner, syncPath);
  try {
    verifyRollbackTarget(resolvedRoot, toLineage, owner);
  } catch (error) {
    if (error instanceof LineageCommitError) {
      if (error.message === "lineage_rollback_target_invalid") {
        throw error;
      }
    }
    fail("lineage_rollback_target_invalid");
  }
  try {
    verifyHermesCoreLineage({
      expectedLineage: fromLineage,
      coreRoot: resolvedRoot,
      trustedParent,
      owner,
      minimumFreeBytes: 0,
      lockHeld,
      lockFd,
      verifyInheritedLock,
    });
  } catch (error) {
    if (error instanceof LineageCommitError) {
      throw error;
    }
    fail("lineage_rollback_generation_invalid");
  }

  try {
    switchActivePointer(lineageRoot, toTarget, syncPath);
  } catch (error) {
    if (
      error instanceof LineageCommitError &&
      error.message === "lineage_prepare_failed"
    ) {
      fail("lineage_rollback_prepare_failed");
    }
    fail("lineage_rollback_commit_uncertain");
  }

  let verified;
  try {
    verified = verifyHermesCoreLineage({
      expectedLineage: toLineage,
      coreRoot: resolvedRoot,
      trustedParent,
      owner,
      minimumFreeBytes: 0,
      lockHeld,
      lockFd,
      verifyInheritedLock,
    });
  } catch {
    fail("lineage_rollback_commit_uncertain");
  }
  return {
    ...verified,
    restoredFromCommit: fromLineage.currentCommit,
    alreadyRestored: false,
    recovered: false,
    pointerChanges: 1,
    serviceChanges: 0,
  };
}

export function commitHermesCoreLineage({ expected }) {
  if (process.env.QINTOPIA_HERMES_CORE_LOCK_HELD !== "1") {
    fail("manager_lock_not_held");
  }
  return commitHermesCoreLineageInternal({
    expected,
    coreRoot: FIXED_CORE_ROOT,
    trustedParent: FIXED_TRUSTED_PARENT,
    owner: { uid: 0, gid: 0 },
    minimumFreeBytes: MIN_FREE_BYTES,
    lockHeld: true,
    lockFd: 9,
    verifyInheritedLock: true,
  });
}

export function commitHermesCoreLineageForTest(options) {
  return commitHermesCoreLineageInternal(options);
}

export function restoreHermesCoreLineage({ fromLineage, toLineage }) {
  if (process.env.QINTOPIA_HERMES_CORE_LOCK_HELD !== "1") {
    fail("manager_lock_not_held");
  }
  return restoreHermesCoreLineageInternal({
    fromLineage,
    toLineage,
    coreRoot: FIXED_CORE_ROOT,
    trustedParent: FIXED_TRUSTED_PARENT,
    owner: { uid: 0, gid: 0 },
    lockHeld: true,
    lockFd: 9,
    verifyInheritedLock: true,
  });
}

export function restoreHermesCoreLineageForTest(options) {
  return restoreHermesCoreLineageInternal(options);
}
