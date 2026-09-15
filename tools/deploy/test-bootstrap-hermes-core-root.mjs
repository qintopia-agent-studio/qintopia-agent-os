#!/usr/bin/env node

import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";
import {
  BootstrapError,
  FIXED_PENDING_ROOT_NAME,
  bootstrapHermesCoreRoot,
  computeHermesCoreBootstrapLineageFingerprint,
  openHermesCoreBootstrapLock,
  verifyBootstrappedHermesCoreRoot,
} from "./bootstrap-hermes-core-root.mjs";
import { computeHermesCoreArtifactIdentity } from "./verify-hermes-core-artifact.mjs";

const tmpRoot = fs.realpathSync.native(
  fs.mkdtempSync(path.join(os.tmpdir(), "hermes-core-bootstrap-"))
);
const owner = {
  uid: process.getuid?.() ?? 0,
  gid: process.getgid?.() ?? 0,
};
const currentCommit = "1".repeat(40);
const previousCommit = "2".repeat(40);
const alternateCommit = "3".repeat(40);
let fixtureNumber = 0;

try {
  testSuccessfulBootstrapAndRepeat();
  testIdentityAndLineageRejection();
  testSymlinkHardlinkAndModeRejection();
  testFsyncAndRenameBoundaries();
  testQuarantineFailureBoundaries();
  testQuarantinePersistenceBoundaries();
  await testBootstrapLockCreationAndConcurrency();
  testMissingLockDoesNotWrite();
  testInsufficientSpaceDoesNotWrite();
} finally {
  makeWritable(tmpRoot);
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Hermes core bootstrap test passed.");

function testSuccessfulBootstrapAndRepeat() {
  const fixture = createFixture();
  const result = runBootstrap(fixture);
  assert.deepEqual(result, {
    status: "bootstrapped",
    alreadyBootstrapped: false,
    pointerChanges: 0,
    serviceChanges: 0,
    legacyCheckoutTouched: false,
    externalLock: true,
  });
  assert.equal(
    fs.existsSync(path.join(fixture.coreParent, FIXED_PENDING_ROOT_NAME)),
    false
  );
  assert.equal(fs.existsSync(fixture.coreRoot), true);
  assert.equal(fs.existsSync(fixture.quarantineRoot), true);
  assert.deepEqual(fs.readdirSync(fixture.quarantineRoot), []);

  const verified = verifyBootstrappedHermesCoreRoot({
    root: fixture.coreRoot,
    currentArtifact: fixture.current,
    previousArtifact: fixture.previous,
    owner,
  });
  assert.equal(verified.verified, true);
  assert.equal(
    verified.fingerprint,
    computeHermesCoreBootstrapLineageFingerprint({
      currentCommit,
      previousCommit,
      rollbackReserveCommit: previousCommit,
    })
  );
  assert.equal(
    fs.readlinkSync(path.join(fixture.coreRoot, "current")),
    "lineage/active/current"
  );
  assert.equal(
    fs.readlinkSync(path.join(fixture.coreRoot, "previous")),
    "lineage/active/previous"
  );
  assert.equal(
    fs.readlinkSync(path.join(fixture.coreRoot, "rollback-reserve")),
    "lineage/active/rollback-reserve"
  );
  assert.equal(
    fs
      .readFileSync(
        path.join(fixture.coreRoot, "releases", currentCommit, "update-receipt.json"),
        "utf8"
      )
      .includes(`"previous_commit_sha": "${previousCommit}"`),
    true
  );
  assertModeAndOwner(fixture.coreRoot, 0o755);
  assertModeAndOwner(path.join(fixture.coreRoot, "releases"), 0o755);
  assertModeAndOwner(path.join(fixture.coreRoot, "releases", currentCommit), 0o555);
  assertModeAndOwner(path.join(fixture.coreRoot, "state"), 0o700);
  assertModeAndOwner(path.join(fixture.coreRoot, "state", "manager.lock"), 0o600);
  assertModeAndOwner(fixture.ingressRoot, 0o700);
  assertModeAndOwner(fixture.lockPath, 0o600);
  assertModeAndOwner(fixture.quarantineRoot, 0o700);

  const beforeRepeat = snapshotTree(fixture.coreParent);
  const repeatResult = runBootstrap(fixture);
  assert.deepEqual(repeatResult, {
    status: "already_bootstrapped",
    alreadyBootstrapped: true,
    pointerChanges: 0,
    serviceChanges: 0,
    legacyCheckoutTouched: false,
    externalLock: true,
  });
  assert.deepEqual(snapshotTree(fixture.coreParent), beforeRepeat);
}

function testIdentityAndLineageRejection() {
  let fixture = createFixture();
  const before = snapshotFixture(fixture);
  assertBootstrapError(
    () =>
      runBootstrap(fixture, {
        current: {
          ...fixture.current,
          expected: {
            ...fixture.current.expected,
            identitySha256: "f".repeat(64),
          },
        },
      }),
    "bootstrap_current_artifact_invalid"
  );
  assertFixtureUnchanged(fixture, before);

  fixture = createFixture({ currentPreviousCommit: alternateCommit });
  assertBootstrapError(
    () => runBootstrap(fixture),
    "bootstrap_current_lineage_binding_invalid"
  );
  assert.equal(fs.existsSync(fixture.coreRoot), false);
  assert.equal(
    fs.existsSync(path.join(fixture.coreParent, FIXED_PENDING_ROOT_NAME)),
    false
  );
  assert.equal(fs.existsSync(fixture.quarantineRoot), false);

  fixture = createFixture();
  const lineageBefore = snapshotFixture(fixture);
  assertBootstrapError(
    () =>
      runBootstrap(fixture, {
        lineage: {
          currentCommit,
          previousCommit: alternateCommit,
          rollbackReserveCommit: alternateCommit,
        },
      }),
    "bootstrap_lineage_invalid"
  );
  assertFixtureUnchanged(fixture, lineageBefore);
}

function testSymlinkHardlinkAndModeRejection() {
  let fixture = createFixture();
  replaceWithSymlink(fixture.current.artifactDir);
  assertBootstrapError(
    () => runBootstrap(fixture),
    "bootstrap_current_artifact_invalid"
  );
  assert.equal(fs.existsSync(fixture.coreRoot), false);

  fixture = createFixture();
  addHardlink(fixture.current.artifactDir);
  assertBootstrapError(
    () => runBootstrap(fixture),
    "bootstrap_current_artifact_invalid"
  );
  assert.equal(fs.existsSync(fixture.coreRoot), false);

  fixture = createFixture();
  fs.chmodSync(fixture.ingressRoot, 0o755);
  assertBootstrapError(() => runBootstrap(fixture), "bootstrap_ingress_invalid");
  assert.equal(fs.existsSync(fixture.coreRoot), false);

  fixture = createFixture();
  fs.chmodSync(fixture.lockPath, 0o640);
  assertBootstrapError(() => runBootstrap(fixture), "bootstrap_lock_not_held");
  assert.equal(fs.existsSync(fixture.coreRoot), false);

  fixture = createFixture();
  fs.chmodSync(fixture.coreParent, 0o770);
  assertBootstrapError(() => runBootstrap(fixture), "bootstrap_core_parent_invalid");
  assert.equal(fs.existsSync(fixture.coreRoot), false);

  fixture = createFixture();
  const wrongOwner = { uid: owner.uid + 1, gid: owner.gid };
  assertBootstrapError(
    () => runBootstrap(fixture, { owner: wrongOwner }),
    "bootstrap_core_parent_invalid"
  );
  assert.equal(fs.existsSync(fixture.coreRoot), false);

  fixture = createFixture();
  rewriteReadOnly(
    path.join(fixture.current.artifactDir, "core", "hermes_cli", "main.py"),
    "tampered\n"
  );
  assertBootstrapError(
    () => runBootstrap(fixture),
    "bootstrap_current_artifact_invalid"
  );
  assert.equal(fs.existsSync(fixture.coreRoot), false);
}

function testFsyncAndRenameBoundaries() {
  let fixture = createFixture();
  let pendingRootSyncFailed = false;
  assertBootstrapError(
    () =>
      runBootstrap(fixture, {
        syncPath: (targetPath) => {
          if (
            !pendingRootSyncFailed &&
            targetPath === path.join(fixture.coreParent, FIXED_PENDING_ROOT_NAME)
          ) {
            pendingRootSyncFailed = true;
            throw new Error("fixture pending root fsync failure");
          }
          fsyncFixturePath(targetPath);
        },
      }),
    "bootstrap_prepare_failed"
  );
  assertQuarantinedPending(fixture);

  fixture = createFixture();
  let generationSyncFailed = false;
  const generationRoot = expectedGenerationRoot(fixture);
  assertBootstrapError(
    () =>
      runBootstrap(fixture, {
        syncPath: (targetPath) => {
          if (!generationSyncFailed && targetPath === generationRoot) {
            generationSyncFailed = true;
            throw new Error("fixture generation fsync failure");
          }
          fsyncFixturePath(targetPath);
        },
      }),
    "bootstrap_prepare_failed"
  );
  assertQuarantinedPending(fixture);

  fixture = createFixture();
  let renameAttempted = false;
  assertBootstrapError(
    () =>
      runBootstrap(fixture, {
        renamePath: (from, to) => {
          if (!renameAttempted && to === fixture.coreRoot) {
            renameAttempted = true;
            throw new Error("fixture rename failure");
          }
          fs.renameSync(from, to);
        },
      }),
    "bootstrap_rename_failed"
  );
  assertQuarantinedPending(fixture);

  fixture = createFixture();
  let postRenameSyncFailed = false;
  assertBootstrapError(
    () =>
      runBootstrap(fixture, {
        syncPath: (targetPath) => {
          if (
            !postRenameSyncFailed &&
            targetPath === fixture.coreParent &&
            fs.existsSync(fixture.coreRoot)
          ) {
            postRenameSyncFailed = true;
            throw new Error("fixture post-rename fsync failure");
          }
          fsyncFixturePath(targetPath);
        },
      }),
    "bootstrap_commit_uncertain"
  );
  assert.equal(fs.existsSync(fixture.coreRoot), true);
  assert.equal(runBootstrap(fixture).status, "already_bootstrapped");

  fixture = createFixture();
  runBootstrap(fixture);
  const recoverySyncs = [];
  let rootSynced = false;
  assertBootstrapError(
    () =>
      runBootstrap(fixture, {
        syncPath: (targetPath) => {
          recoverySyncs.push(targetPath);
          if (targetPath === fixture.coreRoot) {
            rootSynced = true;
          }
          if (targetPath === fixture.coreParent && rootSynced) {
            throw new Error("fixture existing root parent fsync failure");
          }
          fsyncFixturePath(targetPath);
        },
      }),
    "bootstrap_commit_uncertain"
  );
  assert.equal(recoverySyncs.includes(fixture.coreRoot), true);
  assert.equal(recoverySyncs.includes(fixture.coreParent), true);
  assert.equal(runBootstrap(fixture).status, "already_bootstrapped");
}

function testQuarantineFailureBoundaries() {
  let fixture = createFixture();
  let quarantineSyncFailed = false;
  let pendingBuildFailed = false;
  assertBootstrapError(
    () =>
      runBootstrap(fixture, {
        copyFile: () => {
          pendingBuildFailed = true;
          throw new Error("fixture copy failure");
        },
        syncPath: (targetPath) => {
          if (
            pendingBuildFailed &&
            !quarantineSyncFailed &&
            targetPath === fixture.quarantineRoot
          ) {
            quarantineSyncFailed = true;
            throw new Error("fixture quarantine fsync failure");
          }
          fsyncFixturePath(targetPath);
        },
      }),
    "bootstrap_quarantine_commit_uncertain"
  );
  assert.equal(
    fs.existsSync(path.join(fixture.coreParent, FIXED_PENDING_ROOT_NAME)),
    false
  );
  assert.equal(fs.readdirSync(fixture.quarantineRoot).length, 1);
  assert.equal(quarantineSyncFailed, true);

  fixture = createFixture();
  fs.mkdirSync(fixture.quarantineRoot);
  fs.chmodSync(fixture.quarantineRoot, 0o500);
  assertBootstrapError(
    () =>
      runBootstrap(fixture, {
        copyFile: () => {
          throw new Error("fixture copy failure");
        },
      }),
    "bootstrap_quarantine_invalid"
  );
  assert.equal(
    fs.existsSync(path.join(fixture.coreParent, FIXED_PENDING_ROOT_NAME)),
    false
  );
}

function testQuarantinePersistenceBoundaries() {
  let fixture = createFixture();
  const firstCreationSyncs = [];
  runBootstrap(fixture, {
    syncPath: (targetPath) => {
      firstCreationSyncs.push(targetPath);
      fsyncFixturePath(targetPath);
    },
  });
  assert.equal(firstCreationSyncs.includes(fixture.quarantineRoot), true);
  assert.equal(firstCreationSyncs.includes(fixture.externalParent), true);

  fixture = createFixture();
  let quarantineCreatedSyncFailed = false;
  const failedCreationSyncs = [];
  assertBootstrapError(
    () =>
      runBootstrap(fixture, {
        syncPath: (targetPath) => {
          failedCreationSyncs.push(targetPath);
          if (!quarantineCreatedSyncFailed && targetPath === fixture.quarantineRoot) {
            quarantineCreatedSyncFailed = true;
            throw new Error("fixture quarantine creation fsync failure");
          }
          fsyncFixturePath(targetPath);
        },
      }),
    "bootstrap_quarantine_commit_uncertain"
  );
  assert.equal(fs.existsSync(fixture.quarantineRoot), true);
  assert.equal(fs.existsSync(fixture.coreRoot), false);
  assert.equal(failedCreationSyncs.includes(fixture.externalParent), true);
  assert.equal(runBootstrap(fixture).status, "bootstrapped");

  fixture = createFixture();
  const moveSyncs = [];
  assertBootstrapError(
    () =>
      runBootstrap(fixture, {
        copyFile: () => {
          throw new Error("fixture copy failure before quarantine move");
        },
        syncPath: (targetPath) => {
          moveSyncs.push(targetPath);
          fsyncFixturePath(targetPath);
        },
      }),
    "bootstrap_prepare_failed"
  );
  assert.equal(moveSyncs.includes(fixture.coreParent), true);
  assert.equal(moveSyncs.includes(fixture.quarantineRoot), true);
  assert.equal(moveSyncs.includes(fixture.externalParent), true);
  assert.equal(fs.readdirSync(fixture.quarantineRoot).length, 1);

  const recoverySyncs = [];
  const recoveryResult = runBootstrap(fixture, {
    syncPath: (targetPath) => {
      recoverySyncs.push(targetPath);
      fsyncFixturePath(targetPath);
    },
  });
  assert.equal(recoveryResult.status, "bootstrapped");
  assert.equal(recoverySyncs.includes(fixture.quarantineRoot), true);
  assert.equal(recoverySyncs.includes(fixture.externalParent), true);
}

async function testBootstrapLockCreationAndConcurrency() {
  const fixture = createFixture();
  fs.unlinkSync(fixture.lockPath);
  const firstCreationSyncs = [];
  let firstCreationFlockInode = null;
  const firstLock = openHermesCoreBootstrapLock({
    lockPath: fixture.lockPath,
    trustedParent: fixture.externalParent,
    owner,
    syncPath: (targetPath) => {
      firstCreationSyncs.push(targetPath);
      fsyncFixturePath(targetPath);
    },
    flock: (lockFd) => {
      const pathMetadata = fs.lstatSync(fixture.lockPath);
      const descriptorMetadata = fs.fstatSync(lockFd);
      firstCreationFlockInode = {
        pathDev: pathMetadata.dev,
        pathIno: pathMetadata.ino,
        fdDev: descriptorMetadata.dev,
        fdIno: descriptorMetadata.ino,
      };
      return true;
    },
  });
  assert.equal(firstLock.created, true);
  assert.equal(firstCreationSyncs.includes(fixture.lockPath), true);
  assert.equal(firstCreationSyncs.includes(fixture.externalParent), true);
  assert.deepEqual(firstCreationFlockInode, {
    pathDev: firstCreationFlockInode.fdDev,
    pathIno: firstCreationFlockInode.fdIno,
    fdDev: firstCreationFlockInode.fdDev,
    fdIno: firstCreationFlockInode.fdIno,
  });
  fs.closeSync(firstLock.fd);
  fs.unlinkSync(fixture.lockPath);
  const bootstrapModuleUrl = pathToFileURL(
    path.resolve("tools/deploy/bootstrap-hermes-core-root.mjs")
  ).href;
  const workerSource = `
import fs from "node:fs";
import { openHermesCoreBootstrapLock } from ${JSON.stringify(bootstrapModuleUrl)};

const lockPath = process.argv[1];
const trustedParent = process.argv[2];
const owner = { uid: Number(process.argv[3]), gid: Number(process.argv[4]) };
const result = openHermesCoreBootstrapLock({
  lockPath,
  trustedParent,
  owner,
  syncPath: () => {},
  flock: (lockFd) => {
    const pathMetadata = fs.lstatSync(lockPath);
    const descriptorMetadata = fs.fstatSync(lockFd);
    if (
      pathMetadata.dev !== descriptorMetadata.dev ||
      pathMetadata.ino !== descriptorMetadata.ino
    ) {
      return false;
    }
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 75);
    return true;
  },
});
const pathMetadata = fs.lstatSync(lockPath);
const descriptorMetadata = fs.fstatSync(result.fd);
process.stdout.write(JSON.stringify({
  created: result.created,
  pathDev: pathMetadata.dev,
  pathIno: pathMetadata.ino,
  fdDev: descriptorMetadata.dev,
  fdIno: descriptorMetadata.ino,
}));
fs.closeSync(result.fd);
`;
  const results = await Promise.all(
    Array.from({ length: 8 }, () =>
      spawnBootstrapLockWorker({
        workerSource,
        lockPath: fixture.lockPath,
        trustedParent: fixture.externalParent,
        owner,
      })
    )
  );
  assert.equal(results.filter((result) => result.created).length, 1);
  const finalMetadata = fs.lstatSync(fixture.lockPath);
  for (const result of results) {
    assert.equal(result.pathDev, finalMetadata.dev);
    assert.equal(result.pathIno, finalMetadata.ino);
    assert.equal(result.fdDev, finalMetadata.dev);
    assert.equal(result.fdIno, finalMetadata.ino);
  }
  assertModeAndOwner(fixture.lockPath, 0o600);
}

function spawnBootstrapLockWorker({
  workerSource,
  lockPath,
  trustedParent,
  owner: workerOwner,
}) {
  return new Promise((resolve, reject) => {
    const child = spawn(
      process.execPath,
      [
        "--input-type=module",
        "-e",
        workerSource,
        lockPath,
        trustedParent,
        String(workerOwner.uid),
        String(workerOwner.gid),
      ],
      { stdio: ["ignore", "pipe", "ignore"] }
    );
    let output = "";
    child.stdout.setEncoding("utf8");
    child.stdout.on("data", (chunk) => {
      output += chunk;
    });
    child.once("error", () => reject(new Error("bootstrap lock worker failed")));
    child.once("close", (code) => {
      if (code !== 0) {
        reject(new Error("bootstrap lock worker failed"));
        return;
      }
      try {
        resolve(JSON.parse(output));
      } catch {
        reject(new Error("bootstrap lock worker output invalid"));
      }
    });
  });
}

function testMissingLockDoesNotWrite() {
  let fixture = createFixture();
  const before = snapshotFixture(fixture);
  fs.unlinkSync(fixture.lockPath);
  const afterRemoval = snapshotFixture(fixture);
  assertBootstrapError(() => runBootstrap(fixture), "bootstrap_lock_not_held");
  assert.deepEqual(snapshotFixture(fixture), afterRemoval);
  assert.notDeepEqual(before, afterRemoval);

  fixture = createFixture();
  const beforeNoLock = snapshotFixture(fixture);
  assertBootstrapError(
    () => runBootstrap(fixture, { lockHeld: false }),
    "bootstrap_lock_not_held"
  );
  assertFixtureUnchanged(fixture, beforeNoLock);
}

function testInsufficientSpaceDoesNotWrite() {
  const fixture = createFixture();
  const before = snapshotFixture(fixture);
  assertBootstrapError(
    () => runBootstrap(fixture, { minimumFreeBytes: Number.MAX_SAFE_INTEGER }),
    "bootstrap_space_insufficient"
  );
  assertFixtureUnchanged(fixture, before);
}

function runBootstrap(fixture, overrides = {}) {
  const suppliedLockFd = overrides.lockFd;
  const lockHeld = overrides.lockHeld ?? true;
  const lockFd = suppliedLockFd ?? (lockHeld ? openLockOrNull(fixture.lockPath) : null);
  try {
    return bootstrapHermesCoreRoot({
      currentArtifact: overrides.current ?? fixture.current,
      previousArtifact: overrides.previous ?? fixture.previous,
      lineage: overrides.lineage,
      coreRoot: fixture.coreRoot,
      coreTrustedParent: fixture.coreParent,
      ingressRoot: fixture.ingressRoot,
      ingressTrustedParent: fixture.externalParent,
      bootstrapLockPath: fixture.lockPath,
      bootstrapLockTrustedParent: fixture.externalParent,
      quarantineRoot: fixture.quarantineRoot,
      quarantineTrustedParent: fixture.externalParent,
      owner: overrides.owner ?? owner,
      minimumFreeBytes: overrides.minimumFreeBytes ?? 0,
      lockHeld,
      lockFd,
      copyFile: overrides.copyFile,
      renamePath: overrides.renamePath,
      syncPath: overrides.syncPath,
    });
  } finally {
    if (suppliedLockFd === undefined && lockFd !== null) {
      fs.closeSync(lockFd);
    }
  }
}

function openLockOrNull(lockPath) {
  try {
    return fs.openSync(lockPath, "r+");
  } catch (error) {
    if (error?.code === "ENOENT") {
      return null;
    }
    throw error;
  }
}

function createFixture({ currentPreviousCommit = previousCommit } = {}) {
  fixtureNumber += 1;
  const fixtureRoot = path.join(tmpRoot, `fixture-${fixtureNumber}`);
  const coreParent = path.join(fixtureRoot, "var-lib");
  const externalParent = path.join(fixtureRoot, "deploy-state");
  const coreRoot = path.join(coreParent, "qintopia-hermes-core");
  const ingressRoot = path.join(externalParent, "hermes-core-ingress");
  const quarantineRoot = path.join(externalParent, "hermes-core-bootstrap-quarantine");
  const lockPath = path.join(externalParent, "hermes-core-bootstrap.lock");
  fs.mkdirSync(coreParent, { recursive: true });
  fs.mkdirSync(externalParent, { recursive: true });
  fs.mkdirSync(ingressRoot);
  fs.chmodSync(coreParent, 0o700);
  fs.chmodSync(externalParent, 0o700);
  fs.chmodSync(ingressRoot, 0o700);
  writeEmptyFile(lockPath, 0o600);

  const currentArtifact = createArtifact(path.join(ingressRoot, currentCommit), {
    commit: currentCommit,
    tag: "v2.0.0",
    version: "2.0.0",
    previousCommit: currentPreviousCommit,
    sourceCharacter: "a",
  });
  const previousArtifact = createArtifact(path.join(ingressRoot, previousCommit), {
    commit: previousCommit,
    tag: "v1.9.0",
    version: "1.9.0",
    previousCommit: null,
    sourceCharacter: "b",
  });
  return {
    fixtureRoot,
    coreParent,
    coreRoot,
    externalParent,
    ingressRoot,
    quarantineRoot,
    lockPath,
    current: currentArtifact,
    previous: previousArtifact,
  };
}

function createArtifact(root, options) {
  const coreRoot = path.join(root, "core");
  const packageRoot = path.join(coreRoot, "hermes_cli");
  fs.mkdirSync(packageRoot, { recursive: true });
  fs.writeFileSync(
    path.join(coreRoot, "pyproject.toml"),
    "[project]\nname='hermes-agent'\n"
  );
  fs.writeFileSync(path.join(packageRoot, "main.py"), "print('fixture')\n");
  const runtime = {
    kind: "release-local-venv",
    platform: "linux-x86_64",
    interpreter_path: "runtime/venv/bin/python",
    site_packages_path: "runtime/venv/lib/python3.12/site-packages",
    launcher_path: "runtime/hermes-core-launcher.py",
  };
  const runtimeVenvBin = path.join(root, "runtime", "venv", "bin");
  const runtimeSitePackages = path.join(
    root,
    "runtime",
    "venv",
    "lib",
    "python3.12",
    "site-packages"
  );
  fs.mkdirSync(runtimeSitePackages, { recursive: true });
  fs.mkdirSync(runtimeVenvBin, { recursive: true });
  fs.writeFileSync(path.join(runtimeVenvBin, "python"), "#!/bin/sh\nexit 0\n");
  fs.writeFileSync(path.join(runtimeSitePackages, "fixture_dep.py"), "VALUE = 1\n");
  fs.writeFileSync(
    path.join(root, "runtime", "hermes-core-launcher.py"),
    "#!/usr/bin/env python3\n"
  );

  const sourceSha256 = options.sourceCharacter.repeat(64);
  const identityFields = {
    repository: "https://github.com/NousResearch/hermes-agent.git",
    tag: options.tag,
    commit_sha: options.commit,
    source_archive_sha256: sourceSha256,
    python_abi: "cp312",
    dependency_lock_sha256: "d".repeat(64),
    runtime_binding_sha256: crypto
      .createHash("sha256")
      .update(`${JSON.stringify(runtime)}\n`)
      .digest("hex"),
    hermes_cli_version: options.version,
  };
  const artifactIdentitySha256 = computeHermesCoreArtifactIdentity(identityFields);
  const sharedIdentity = {
    ...identityFields,
    artifact_identity_sha256: artifactIdentitySha256,
  };
  writeJson(path.join(root, "build-receipt.json"), {
    schema_version: 1,
    receipt_type: "hermes-core-build",
    outcome: "success",
    ...sharedIdentity,
    started_at: "2026-09-10T00:00:00.000Z",
    completed_at: "2026-09-10T00:10:00.000Z",
    builder: {
      os: "linux",
      arch: "x86_64",
      image_digest: `sha256:${"e".repeat(64)}`,
    },
    source_checkout: { clean: true, tag_commit_verified: true },
    dependency_install: { locked: true, hashes_verified: true },
  });
  writeJson(path.join(root, "update-receipt.json"), {
    schema_version: 1,
    receipt_type: "hermes-core-update",
    outcome: "success",
    strategy: "hermes-update-clean-candidate",
    repository: identityFields.repository,
    tag: options.tag,
    commit_sha: options.commit,
    source_archive_sha256: sourceSha256,
    artifact_identity_sha256: artifactIdentitySha256,
    previous_version: options.previousCommit ? "1.8.0" : "1.8.0",
    previous_commit_sha: options.previousCommit,
    new_version: options.version,
    started_at: "2026-09-10T00:02:00Z",
    completed_at: "2026-09-10T00:08:00.000Z",
    upstream_receipt_sha256: "f".repeat(64),
  });
  writeJson(path.join(root, "validation-summary.json"), {
    schema_version: 1,
    summary_type: "hermes-core-validation",
    outcome: "success",
    artifact_identity_sha256: artifactIdentitySha256,
    runtime_binding_sha256: identityFields.runtime_binding_sha256,
    profile_contract: {
      profile_count: 7,
      wecom_enabled_count: 5,
      wecom_disabled_count: 2,
      qiwe_preserved_count: 1,
    },
    checks: {
      schema_validation: "passed",
      source_integrity: "passed",
      dependency_lock: "passed",
      hermes_cli_smoke: "passed",
    },
    sensitive_values_included: false,
  });
  chmodTree(coreRoot);
  chmodTree(path.join(root, "runtime"));
  fs.chmodSync(path.join(runtimeVenvBin, "python"), 0o555);
  fs.chmodSync(path.join(root, "runtime", "hermes-core-launcher.py"), 0o555);

  const manifest = {
    schema_version: 1,
    artifact_type: "hermes-core",
    artifact_name: `hermes-core-${options.commit}`,
    ...sharedIdentity,
    runtime,
    files: inventoryCore(root),
    receipts: {
      build: {
        path: "build-receipt.json",
        sha256: sha256File(path.join(root, "build-receipt.json")),
      },
      update: {
        path: "update-receipt.json",
        sha256: sha256File(path.join(root, "update-receipt.json")),
      },
      validation: {
        path: "validation-summary.json",
        sha256: sha256File(path.join(root, "validation-summary.json")),
      },
    },
  };
  writeJson(path.join(root, "artifact-manifest.json"), manifest);
  writeChecksums(root, manifest);
  chmodTree(root);
  fs.chmodSync(path.join(root, "runtime", "venv", "bin", "python"), 0o555);
  fs.chmodSync(path.join(root, "runtime", "hermes-core-launcher.py"), 0o555);
  fs.chmodSync(root, 0o555);
  return {
    artifactDir: root,
    expected: {
      tag: options.tag,
      commit: options.commit,
      sourceSha256,
      identitySha256: artifactIdentitySha256,
      manifestSha256: sha256File(path.join(root, "artifact-manifest.json")),
    },
  };
}

function inventoryCore(root) {
  const entries = [];
  const visit = (directoryPath, relativeDirectory) => {
    for (const name of fs.readdirSync(directoryPath).sort()) {
      const absolutePath = path.join(directoryPath, name);
      const relativePath = path.posix.join(relativeDirectory, name);
      const metadata = fs.lstatSync(absolutePath);
      if (metadata.isDirectory()) {
        entries.push({
          path: relativePath,
          type: "directory",
          mode: "0555",
          owner: "release-owner",
        });
        visit(absolutePath, relativePath);
      } else {
        entries.push({
          path: relativePath,
          type: "file",
          mode: (metadata.mode & 0o7777).toString(8).padStart(4, "0"),
          owner: "release-owner",
          size_bytes: metadata.size,
          sha256: sha256File(absolutePath),
        });
      }
    }
  };
  visit(path.join(root, "core"), "core");
  visit(path.join(root, "runtime"), "runtime");
  return entries.sort((left, right) => left.path.localeCompare(right.path));
}

function writeChecksums(root, manifest) {
  const relativePaths = [
    "artifact-manifest.json",
    "build-receipt.json",
    "update-receipt.json",
    "validation-summary.json",
    ...manifest.files
      .filter((entry) => entry.type === "file")
      .map((entry) => entry.path),
  ].sort();
  const content = relativePaths
    .map(
      (relativePath) =>
        `${sha256File(path.join(root, ...relativePath.split("/")))}  ${relativePath}`
    )
    .join("\n");
  fs.writeFileSync(path.join(root, "SHA256SUMS"), `${content}\n`);
  fs.chmodSync(path.join(root, "SHA256SUMS"), 0o444);
}

function sha256File(targetPath) {
  return crypto.createHash("sha256").update(fs.readFileSync(targetPath)).digest("hex");
}

function writeJson(targetPath, value) {
  fs.writeFileSync(targetPath, `${JSON.stringify(value, null, 2)}\n`);
  fs.chmodSync(targetPath, 0o444);
}

function writeEmptyFile(targetPath, mode) {
  fs.writeFileSync(targetPath, "");
  fs.chmodSync(targetPath, mode);
}

function chmodTree(root) {
  for (const name of fs.readdirSync(root)) {
    const targetPath = path.join(root, name);
    const metadata = fs.lstatSync(targetPath);
    if (metadata.isDirectory()) {
      chmodTree(targetPath);
      fs.chmodSync(targetPath, 0o555);
    } else {
      fs.chmodSync(targetPath, 0o444);
    }
  }
}

function assertModeAndOwner(targetPath, expectedMode) {
  const metadata = fs.lstatSync(targetPath);
  assert.equal(metadata.uid, owner.uid);
  assert.equal(metadata.gid, owner.gid);
  assert.equal(metadata.mode & 0o7777, expectedMode);
}

function assertBootstrapError(callback, expectedCode) {
  assert.throws(callback, (error) => {
    assert.equal(error instanceof BootstrapError, true);
    assert.equal(error.code, expectedCode);
    assert.equal(error.message.includes("/"), false);
    return true;
  });
}

function assertQuarantinedPending(fixture) {
  assert.equal(fs.existsSync(fixture.coreRoot), false);
  assert.equal(
    fs.existsSync(path.join(fixture.coreParent, FIXED_PENDING_ROOT_NAME)),
    false
  );
  assert.equal(fs.readdirSync(fixture.quarantineRoot).length, 1);
  const quarantinePath = path.join(
    fixture.quarantineRoot,
    fs.readdirSync(fixture.quarantineRoot)[0]
  );
  assertModeAndOwner(quarantinePath, 0o700);
}

function assertFixtureUnchanged(fixture, snapshot) {
  assert.deepEqual(snapshotFixture(fixture), snapshot);
  assert.equal(fs.existsSync(fixture.coreRoot), false);
}

function snapshotFixture(fixture) {
  return {
    coreParent: snapshotTree(fixture.coreParent),
    externalParent: snapshotTree(fixture.externalParent),
  };
}

function snapshotTree(root) {
  const result = [];
  const visit = (directoryPath, relativeDirectory) => {
    for (const name of fs.readdirSync(directoryPath).sort()) {
      const absolutePath = path.join(directoryPath, name);
      const relativePath = path.join(relativeDirectory, name);
      const metadata = fs.lstatSync(absolutePath);
      const entry = {
        path: relativePath,
        mode: metadata.mode & 0o7777,
        uid: metadata.uid,
        gid: metadata.gid,
        type: metadata.isDirectory()
          ? "directory"
          : metadata.isSymbolicLink()
            ? "symlink"
            : metadata.isFile()
              ? "file"
              : "special",
      };
      if (metadata.isSymbolicLink()) {
        entry.target = fs.readlinkSync(absolutePath);
      } else if (metadata.isDirectory()) {
        visit(absolutePath, relativePath);
      } else if (metadata.isFile()) {
        entry.size = metadata.size;
        entry.sha256 = sha256File(absolutePath);
      }
      result.push(entry);
    }
  };
  visit(root, ".");
  return result;
}

function expectedGenerationRoot(fixture) {
  const fingerprint = computeHermesCoreBootstrapLineageFingerprint({
    currentCommit,
    previousCommit,
    rollbackReserveCommit: previousCommit,
  });
  return path.join(
    fixture.coreParent,
    FIXED_PENDING_ROOT_NAME,
    "lineage",
    "generations",
    `generation-${fingerprint}`
  );
}

function fsyncFixturePath(targetPath) {
  const descriptor = fs.openSync(targetPath, "r");
  try {
    fs.fsyncSync(descriptor);
  } finally {
    fs.closeSync(descriptor);
  }
}

function replaceWithSymlink(targetPath) {
  const backupPath = `${targetPath}.backup`;
  fs.renameSync(targetPath, backupPath);
  fs.symlinkSync(path.basename(backupPath), targetPath);
}

function addHardlink(artifactRoot) {
  const packageRoot = path.join(artifactRoot, "core", "hermes_cli");
  const sourcePath = path.join(packageRoot, "main.py");
  const hardlinkPath = path.join(packageRoot, "main-copy.py");
  fs.chmodSync(packageRoot, 0o755);
  fs.linkSync(sourcePath, hardlinkPath);
  fs.chmodSync(packageRoot, 0o555);
}

function rewriteReadOnly(targetPath, content) {
  const previousMode = fs.lstatSync(targetPath).mode & 0o7777;
  fs.chmodSync(targetPath, 0o644);
  fs.writeFileSync(targetPath, content);
  fs.chmodSync(targetPath, previousMode);
}

function makeWritable(root) {
  if (!fs.existsSync(root)) {
    return;
  }
  const metadata = fs.lstatSync(root);
  if (metadata.isDirectory()) {
    fs.chmodSync(root, 0o755);
    for (const name of fs.readdirSync(root)) {
      makeWritable(path.join(root, name));
    }
  } else if (metadata.isFile()) {
    fs.chmodSync(root, 0o644);
  }
}
