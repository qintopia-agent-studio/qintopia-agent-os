#!/usr/bin/env node

import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import Ajv2020 from "ajv/dist/2020.js";
import {
  computeHermesCoreLineageFingerprint,
  planHermesCoreRelease,
} from "./plan-hermes-core-release.mjs";
import {
  commitHermesCoreLineage,
  commitHermesCoreLineageForTest,
  restoreHermesCoreLineage,
  restoreHermesCoreLineageForTest,
} from "./commit-hermes-core-lineage.mjs";
import { stageHermesCoreRelease } from "./stage-hermes-core-release.mjs";
import { computeHermesCoreArtifactIdentity } from "./verify-hermes-core-artifact.mjs";

const repoRoot = process.cwd();
const plannerPath = path.join(
  repoRoot,
  "tools",
  "deploy",
  "plan-hermes-core-release.mjs"
);
const stagerPath = path.join(
  repoRoot,
  "tools",
  "deploy",
  "stage-hermes-core-release.mjs"
);
const tmpRoot = fs.realpathSync.native(
  fs.mkdtempSync(path.join(os.tmpdir(), "hermes-core-plan-"))
);
const owner = { uid: process.getuid(), gid: process.getgid() };
const currentCommit = "1".repeat(40);
const previousCommit = "2".repeat(40);
const candidateCommit = "3".repeat(40);
const lineageValidator = new Ajv2020({ allErrors: true, strict: true }).compile(
  JSON.parse(
    fs.readFileSync(
      path.join(
        repoRoot,
        "runtime",
        "hermes",
        "core-release-contracts",
        "lineage.schema.json"
      ),
      "utf8"
    )
  )
);
let sequence = 0;

try {
  let fixture = createManagerFixture({ candidateStaged: false });
  const lineageBeforeStage = snapshotTree(path.join(fixture.coreRoot, "lineage"));
  const releasesBeforeStage = snapshotTree(path.join(fixture.coreRoot, "releases"));
  const sourceBeforeStage = snapshotTree(
    path.join(fixture.ingressRoot, candidateCommit)
  );
  const stagingResult = runStage(fixture);
  assert.equal(stagingResult.candidateCommit, candidateCommit);
  assert.equal(stagingResult.sourceVerified, true);
  assert.equal(stagingResult.stagedVerified, true);
  assert.equal(stagingResult.candidateInstalled, true);
  assert.equal(stagingResult.alreadyStaged, false);
  assert.equal(stagingResult.pointerChanges, 0);
  assert.equal(stagingResult.serviceChanges, 0);
  assert.equal(stagingResult.quarantined, false);
  assert.deepEqual(
    snapshotTree(path.join(fixture.coreRoot, "lineage")),
    lineageBeforeStage,
    "staging changed lineage"
  );
  assert.deepEqual(
    snapshotTree(path.join(fixture.coreRoot, "releases")),
    releasesBeforeStage,
    "staging changed releases"
  );
  assert.deepEqual(
    snapshotTree(path.join(fixture.ingressRoot, candidateCommit)),
    sourceBeforeStage,
    "staging changed ingress artifact"
  );
  const repeatedStagingResult = runStage(fixture);
  assert.equal(repeatedStagingResult.alreadyStaged, true);
  assert.equal(repeatedStagingResult.pointerChanges, 0);
  assert.equal(repeatedStagingResult.serviceChanges, 0);

  const before = snapshotTree(fixture.coreRoot);
  const result = runPlan(fixture);
  const after = snapshotTree(fixture.coreRoot);
  assert.deepEqual(after, before, "dry-run changed the core release tree");
  assert.equal(result.candidateCommit, candidateCommit);
  assert.equal(result.currentCommit, currentCommit);
  assert.equal(result.previousCommit, previousCommit);
  assert.equal(result.rollbackReserveCommit, previousCommit);
  assert.equal(
    result.lineageFingerprint,
    computeHermesCoreLineageFingerprint(fixture.expected)
  );
  assert.equal(result.verifiedLineageCount, 2);
  assert.equal(result.pointerChanges, 0);
  assert.equal(result.serviceChanges, 0);

  fixture = createManagerFixture({ candidatePreviousCommit: "4".repeat(40) });
  assertPlanError(() => runPlan(fixture), "candidate_lineage_binding_invalid");
  assertCommitError(() => runCommit(fixture), "candidate_lineage_binding_invalid");
  assert.equal(
    fs.readlinkSync(path.join(fixture.coreRoot, "lineage", "active")),
    `generations/generation-${computeHermesCoreLineageFingerprint({
      currentCommit,
      previousCommit,
      rollbackReserveCommit: previousCommit,
    })}`
  );

  fixture = createManagerFixture({ candidatePreviousCommit: "4".repeat(40) });
  const invalidRecoveryCandidate = path.join(
    fixture.coreRoot,
    "incoming",
    candidateCommit
  );
  fs.chmodSync(invalidRecoveryCandidate, 0o700);
  assertCommitError(() => runCommit(fixture), "candidate_lineage_binding_invalid");
  assert.equal(modeOfPath(invalidRecoveryCandidate), 0o700);

  fixture = createManagerFixture();
  assertCommitError(
    () =>
      runCommit(fixture, {
        expected: { ...fixture.expected, commit: previousCommit },
      }),
    "lineage_identity_invalid"
  );

  fixture = createManagerFixture();
  const productionApiBefore = snapshotTree(fixture.coreRoot);
  assertCommitError(
    () => commitHermesCoreLineage({ expected: fixture.expected }),
    "manager_lock_not_held"
  );
  assert.deepEqual(snapshotTree(fixture.coreRoot), productionApiBefore);
  assertCommitError(
    () =>
      restoreHermesCoreLineage({
        fromLineage: expectedNextLineage(fixture),
        toLineage: originalLineage(fixture),
      }),
    "manager_lock_not_held"
  );
  assert.deepEqual(snapshotTree(fixture.coreRoot), productionApiBefore);

  fixture = createManagerFixture();
  const commitResult = runCommit(fixture);
  assert.equal(commitResult.candidateCommit, candidateCommit);
  assert.equal(commitResult.currentCommit, candidateCommit);
  assert.equal(commitResult.previousCommit, currentCommit);
  assert.equal(commitResult.rollbackReserveCommit, previousCommit);
  assert.equal(commitResult.alreadyCommitted, false);
  assert.equal(commitResult.pointerChanges, 1);
  assert.equal(commitResult.serviceChanges, 0);
  assertCommittedLineage(fixture);
  const repeatCommitResult = runCommit(fixture);
  assert.equal(repeatCommitResult.alreadyCommitted, true);
  assert.equal(repeatCommitResult.recovered, true);
  assert.equal(repeatCommitResult.pointerChanges, 0);
  assertCommittedLineage(fixture);

  const restoreResult = runRestore(fixture);
  assert.equal(restoreResult.currentCommit, currentCommit);
  assert.equal(restoreResult.previousCommit, previousCommit);
  assert.equal(restoreResult.rollbackReserveCommit, previousCommit);
  assert.equal(restoreResult.restoredFromCommit, candidateCommit);
  assert.equal(restoreResult.alreadyRestored, false);
  assert.equal(restoreResult.pointerChanges, 1);
  assertOriginalLineage(fixture);
  const repeatRestoreResult = runRestore(fixture);
  assert.equal(repeatRestoreResult.alreadyRestored, true);
  assert.equal(repeatRestoreResult.pointerChanges, 0);
  assertOriginalLineage(fixture);

  fixture = createManagerFixture();
  runCommit(fixture);
  const beforeInvalidRestore = snapshotTree(fixture.coreRoot);
  assertCommitError(
    () =>
      runRestore(fixture, {
        toLineage: {
          ...originalLineage(fixture),
          currentCommit: "4".repeat(40),
        },
      }),
    "lineage_rollback_identity_invalid"
  );
  assert.deepEqual(snapshotTree(fixture.coreRoot), beforeInvalidRestore);

  fixture = createManagerFixture();
  runCommit(fixture);
  const danglingRollbackGeneration = path.join(
    fixture.coreRoot,
    "lineage",
    expectedOriginalActiveTarget(fixture)
  );
  const danglingRollbackRole = path.join(danglingRollbackGeneration, "previous");
  fs.chmodSync(danglingRollbackGeneration, 0o700);
  fs.unlinkSync(danglingRollbackRole);
  fs.symlinkSync(`../../../releases/${"4".repeat(40)}`, danglingRollbackRole);
  fs.chmodSync(danglingRollbackGeneration, 0o555);
  assertCommitError(() => runRestore(fixture), "lineage_rollback_target_invalid");
  assert.equal(
    fs.readlinkSync(path.join(fixture.coreRoot, "lineage", "active")),
    expectedNextActiveTarget(fixture)
  );

  fixture = createManagerFixture();
  runCommit(fixture);
  const missingRoleRollbackGeneration = path.join(
    fixture.coreRoot,
    "lineage",
    expectedOriginalActiveTarget(fixture)
  );
  const missingRollbackRole = path.join(
    missingRoleRollbackGeneration,
    "rollback-reserve"
  );
  fs.chmodSync(missingRoleRollbackGeneration, 0o700);
  fs.unlinkSync(missingRollbackRole);
  fs.chmodSync(missingRoleRollbackGeneration, 0o555);
  assertCommitError(() => runRestore(fixture), "lineage_rollback_target_invalid");
  assert.equal(
    fs.readlinkSync(path.join(fixture.coreRoot, "lineage", "active")),
    expectedNextActiveTarget(fixture)
  );

  fixture = createManagerFixture();
  runCommit(fixture);
  rewriteReadOnly(
    path.join(
      fixture.coreRoot,
      "releases",
      currentCommit,
      "core",
      "hermes_cli",
      "main.py"
    ),
    "tampered rollback target\n"
  );
  assertCommitError(() => runRestore(fixture), "lineage_rollback_target_invalid");
  assert.equal(
    fs.readlinkSync(path.join(fixture.coreRoot, "lineage", "active")),
    expectedNextActiveTarget(fixture)
  );

  fixture = createManagerFixture();
  runCommit(fixture);
  let rollbackPrepareFailed = false;
  assertCommitError(
    () =>
      runRestore(fixture, {
        syncPath: (targetPath) => {
          if (
            !rollbackPrepareFailed &&
            targetPath === path.join(fixture.coreRoot, "lineage") &&
            fs.existsSync(path.join(targetPath, ".active.pending"))
          ) {
            rollbackPrepareFailed = true;
            throw new Error("fixture rollback prepare failure");
          }
          fsyncFixturePath(targetPath);
        },
      }),
    "lineage_rollback_prepare_failed"
  );
  assert.equal(
    fs.readlinkSync(path.join(fixture.coreRoot, "lineage", "active")),
    expectedNextActiveTarget(fixture)
  );
  assert.equal(runRestore(fixture).pointerChanges, 1);
  assertOriginalLineage(fixture);

  fixture = createManagerFixture();
  runCommit(fixture);
  let rollbackCommitUncertain = false;
  const originalTarget = expectedOriginalActiveTarget(fixture);
  assertCommitError(
    () =>
      runRestore(fixture, {
        syncPath: (targetPath) => {
          fsyncFixturePath(targetPath);
          if (
            !rollbackCommitUncertain &&
            targetPath === path.join(fixture.coreRoot, "lineage") &&
            fs.readlinkSync(path.join(targetPath, "active")) === originalTarget
          ) {
            rollbackCommitUncertain = true;
            throw new Error("fixture rollback commit uncertainty");
          }
        },
      }),
    "lineage_rollback_commit_uncertain"
  );
  const recoveredRestore = runRestore(fixture);
  assert.equal(recoveredRestore.alreadyRestored, true);
  assertOriginalLineage(fixture);

  fixture = createManagerFixture();
  let releaseSyncFailed = false;
  assertCommitError(
    () =>
      runCommit(fixture, {
        syncPath: (targetPath) => {
          if (
            !releaseSyncFailed &&
            targetPath === path.join(fixture.coreRoot, "releases") &&
            fs.existsSync(path.join(targetPath, candidateCommit))
          ) {
            releaseSyncFailed = true;
            throw new Error("fixture release fsync failure");
          }
          fsyncFixturePath(targetPath);
        },
      }),
    "release_install_commit_uncertain"
  );
  assert.equal(
    fs.existsSync(path.join(fixture.coreRoot, "incoming", candidateCommit)),
    false
  );
  assert.equal(
    fs.existsSync(path.join(fixture.coreRoot, "releases", candidateCommit)),
    true
  );
  const releaseRecoverySyncTargets = [];
  assert.equal(
    runCommit(fixture, {
      syncPath: (targetPath) => {
        releaseRecoverySyncTargets.push(targetPath);
        fsyncFixturePath(targetPath);
      },
    }).recovered,
    true
  );
  assert.equal(
    releaseRecoverySyncTargets.includes(
      path.join(fixture.coreRoot, "releases", candidateCommit)
    ),
    true
  );
  assert.equal(
    releaseRecoverySyncTargets.includes(path.join(fixture.coreRoot, "releases")),
    true
  );
  assertCommittedLineage(fixture);

  fixture = createManagerFixture();
  let generationQuarantineStarted = false;
  assertCommitError(
    () =>
      runCommit(fixture, {
        syncPath: (targetPath) => {
          if (
            !generationQuarantineStarted &&
            path.basename(targetPath).startsWith(".generation-")
          ) {
            generationQuarantineStarted = true;
            throw new Error("fixture generation build failure");
          }
          if (
            generationQuarantineStarted &&
            targetPath === path.join(fixture.coreRoot, "lineage", "generations") &&
            fs
              .readdirSync(path.join(fixture.coreRoot, "quarantine"))
              .some((name) => name.startsWith("lineage-failed-"))
          ) {
            throw new Error("fixture generation quarantine fsync failure");
          }
          fsyncFixturePath(targetPath);
        },
      }),
    "generation_quarantine_commit_uncertain"
  );
  assert.equal(
    fs
      .readdirSync(path.join(fixture.coreRoot, "quarantine"))
      .some((name) => name.startsWith("lineage-failed-")),
    true
  );
  assert.equal(runCommit(fixture).recovered, true);
  assertCommittedLineage(fixture);

  fixture = createManagerFixture();
  const interruptedIncoming = path.join(fixture.coreRoot, "incoming", candidateCommit);
  const interruptedRelease = path.join(fixture.coreRoot, "releases", candidateCommit);
  fs.chmodSync(interruptedIncoming, 0o700);
  fs.renameSync(interruptedIncoming, interruptedRelease);
  assert.equal(modeOfPath(interruptedRelease), 0o700);
  assert.equal(runCommit(fixture).recovered, true);
  assert.equal(modeOfPath(interruptedRelease), 0o555);
  assertCommittedLineage(fixture);

  fixture = createManagerFixture();
  const nextGenerationPath = expectedNextGenerationPath(fixture);
  let generationSyncFailed = false;
  assertCommitError(
    () =>
      runCommit(fixture, {
        syncPath: (targetPath) => {
          if (
            !generationSyncFailed &&
            targetPath === path.join(fixture.coreRoot, "lineage", "generations") &&
            fs.existsSync(nextGenerationPath)
          ) {
            generationSyncFailed = true;
            throw new Error("fixture generation fsync failure");
          }
          fsyncFixturePath(targetPath);
        },
      }),
    "generation_commit_uncertain"
  );
  assert.equal(fs.existsSync(nextGenerationPath), true);
  const generationRecoverySyncTargets = [];
  assert.equal(
    runCommit(fixture, {
      syncPath: (targetPath) => {
        generationRecoverySyncTargets.push(targetPath);
        fsyncFixturePath(targetPath);
      },
    }).recovered,
    true
  );
  assert.equal(generationRecoverySyncTargets.includes(nextGenerationPath), true);
  assert.equal(
    generationRecoverySyncTargets.includes(
      path.join(fixture.coreRoot, "lineage", "generations")
    ),
    true
  );
  assertCommittedLineage(fixture);

  fixture = createManagerFixture();
  let generationBuildFailed = false;
  assertCommitError(
    () =>
      runCommit(fixture, {
        syncPath: (targetPath) => {
          if (
            !generationBuildFailed &&
            path.basename(targetPath).startsWith(".generation-")
          ) {
            generationBuildFailed = true;
            throw new Error("fixture generation build fsync failure");
          }
          fsyncFixturePath(targetPath);
        },
      }),
    "generation_create_failed"
  );
  assert.equal(
    fs
      .readdirSync(path.join(fixture.coreRoot, "quarantine"))
      .some((name) => name.startsWith("lineage-failed-")),
    true
  );
  assert.equal(runCommit(fixture).recovered, true);
  assertCommittedLineage(fixture);

  fixture = createManagerFixture();
  const nextActiveTarget = expectedNextActiveTarget(fixture);
  let activeSyncFailed = false;
  assertCommitError(
    () =>
      runCommit(fixture, {
        syncPath: (targetPath) => {
          if (
            !activeSyncFailed &&
            targetPath === path.join(fixture.coreRoot, "lineage") &&
            fs.readlinkSync(path.join(targetPath, "active")) === nextActiveTarget
          ) {
            activeSyncFailed = true;
            throw new Error("fixture active fsync failure");
          }
          fsyncFixturePath(targetPath);
        },
      }),
    "lineage_commit_uncertain"
  );
  const activeRecoverySyncTargets = [];
  const recoveredCommit = runCommit(fixture, {
    syncPath: (targetPath) => {
      activeRecoverySyncTargets.push(targetPath);
      fsyncFixturePath(targetPath);
    },
  });
  assert.equal(recoveredCommit.alreadyCommitted, true);
  assert.equal(recoveredCommit.recovered, true);
  assert.equal(
    activeRecoverySyncTargets.includes(
      path.join(fixture.coreRoot, "lineage", "active")
    ),
    true
  );
  assert.equal(
    activeRecoverySyncTargets.includes(path.join(fixture.coreRoot, "lineage")),
    true
  );
  assertCommittedLineage(fixture);

  fixture = createManagerFixture();
  let postCommitArtifactChanged = false;
  assertCommitError(
    () =>
      runCommit(fixture, {
        syncPath: (targetPath) => {
          fsyncFixturePath(targetPath);
          if (
            !postCommitArtifactChanged &&
            targetPath === path.join(fixture.coreRoot, "lineage") &&
            fs.readlinkSync(path.join(targetPath, "active")) ===
              expectedNextActiveTarget(fixture)
          ) {
            postCommitArtifactChanged = true;
            rewriteReadOnly(
              path.join(
                fixture.coreRoot,
                "releases",
                candidateCommit,
                "core",
                "hermes_cli",
                "main.py"
              ),
              "tampered after commit\n"
            );
          }
        },
      }),
    "lineage_commit_uncertain"
  );
  assert.equal(
    fs.readlinkSync(path.join(fixture.coreRoot, "lineage", "active")),
    expectedNextActiveTarget(fixture)
  );
  assertCommitError(() => runCommit(fixture), "lineage_commit_uncertain");

  fixture = createManagerFixture();
  let activePrepareFailed = false;
  assertCommitError(
    () =>
      runCommit(fixture, {
        syncPath: (targetPath) => {
          if (
            !activePrepareFailed &&
            targetPath === path.join(fixture.coreRoot, "lineage") &&
            fs.existsSync(path.join(targetPath, ".active.pending"))
          ) {
            activePrepareFailed = true;
            throw new Error("fixture active prepare fsync failure");
          }
          fsyncFixturePath(targetPath);
        },
      }),
    "lineage_prepare_failed"
  );
  assert.equal(
    fs.readlinkSync(path.join(fixture.coreRoot, "lineage", ".active.pending")),
    expectedNextActiveTarget(fixture)
  );
  assert.equal(runCommit(fixture).recovered, true);
  assertCommittedLineage(fixture);

  fixture = createManagerFixture();
  const pendingActivePath = path.join(fixture.coreRoot, "lineage", ".active.pending");
  fs.symlinkSync(expectedNextActiveTarget(fixture), pendingActivePath);
  assertCommitError(
    () => runCommit(fixture, { lockHeld: false }),
    "manager_lock_not_held"
  );
  assert.equal(fs.readlinkSync(pendingActivePath), expectedNextActiveTarget(fixture));

  assertCommitError(
    () =>
      runCommit(fixture, {
        syncPath: (targetPath) => {
          if (
            targetPath === path.join(fixture.coreRoot, "lineage") &&
            !fs.existsSync(pendingActivePath)
          ) {
            throw new Error("fixture active recovery fsync failure");
          }
          fsyncFixturePath(targetPath);
        },
      }),
    "lineage_recovery_commit_uncertain"
  );
  assert.equal(fs.existsSync(pendingActivePath), false);
  assert.equal(runCommit(fixture).recovered, false);
  assertCommittedLineage(fixture);

  fixture = createManagerFixture({ candidateStaged: false });
  assertStageError(
    () => runStage(fixture, { lockHeld: false }),
    "manager_lock_not_held"
  );

  fixture = createManagerFixture({ candidateStaged: false });
  fs.chmodSync(fixture.ingressRoot, 0o755);
  assertStageError(() => runStage(fixture), "ingress_root_invalid");

  fixture = createManagerFixture({ candidateStaged: false });
  replaceIngressCandidateWithSymlink(fixture);
  assertStageError(() => runStage(fixture), "source_artifact_invalid");
  assert.deepEqual(fs.readdirSync(path.join(fixture.coreRoot, "incoming")), []);

  fixture = createManagerFixture({ candidateStaged: false });
  addIngressHardlink(fixture);
  assertStageError(() => runStage(fixture), "source_artifact_invalid");
  assert.deepEqual(fs.readdirSync(path.join(fixture.coreRoot, "incoming")), []);

  fixture = createManagerFixture({ candidateStaged: false });
  assertStageError(
    () => runStage(fixture, { owner: { uid: owner.uid + 1, gid: owner.gid } }),
    "core_root_parent_invalid"
  );

  fixture = createManagerFixture({ candidateStaged: false });
  let sourceMutated = false;
  assertStageError(
    () =>
      runStage(fixture, {
        copyFile: (sourcePath, targetPath, flags) => {
          fs.copyFileSync(sourcePath, targetPath, flags);
          if (!sourceMutated) {
            sourceMutated = true;
            rewriteReadOnly(
              path.join(
                fixture.ingressRoot,
                candidateCommit,
                "validation-summary.json"
              ),
              "{}\n"
            );
          }
        },
      }),
    "staged_artifact_invalid"
  );
  assert.equal(fs.readdirSync(path.join(fixture.coreRoot, "quarantine")).length, 1);

  fixture = createManagerFixture({ candidateStaged: false });
  assertStageError(
    () =>
      runStage(fixture, {
        copyFile: () => {
          throw new Error("fixture copy failure");
        },
        syncPath: (targetPath) => {
          if (targetPath === path.join(fixture.coreRoot, "quarantine")) {
            throw new Error("fixture quarantine fsync failure");
          }
          fsyncFixturePath(targetPath);
        },
      }),
    "candidate_quarantine_commit_uncertain"
  );
  assert.deepEqual(fs.readdirSync(path.join(fixture.coreRoot, "incoming")), []);
  assert.equal(fs.readdirSync(path.join(fixture.coreRoot, "quarantine")).length, 1);

  fixture = createManagerFixture({ candidateStaged: false });
  assertStageError(
    () =>
      runStage(fixture, {
        copyFile: () => {
          throw new Error("fixture copy failure");
        },
      }),
    "candidate_staging_failed"
  );
  assert.deepEqual(fs.readdirSync(path.join(fixture.coreRoot, "incoming")), []);
  assert.equal(fs.readdirSync(path.join(fixture.coreRoot, "quarantine")).length, 1);

  fixture = createManagerFixture({ candidateStaged: false });
  assertStageError(
    () =>
      runStage(fixture, {
        copyFile: () => {
          fs.chmodSync(path.join(fixture.coreRoot, "quarantine"), 0o500);
          throw new Error("fixture quarantine failure");
        },
      }),
    "candidate_quarantine_failed"
  );
  assert.equal(fs.readdirSync(path.join(fixture.coreRoot, "incoming")).length, 1);

  fixture = createManagerFixture({ candidateStaged: false });
  assertStageError(
    () =>
      runStage(fixture, {
        syncPath: (targetPath) => {
          if (
            targetPath === path.join(fixture.coreRoot, "incoming") &&
            fs.existsSync(path.join(targetPath, candidateCommit))
          ) {
            throw new Error("fixture parent fsync failure");
          }
          fsyncFixturePath(targetPath);
        },
      }),
    "candidate_staging_commit_uncertain"
  );
  assert.equal(
    fs.existsSync(path.join(fixture.coreRoot, "incoming", candidateCommit)),
    true
  );
  const recoverySyncPaths = [];
  const recoveredStagingResult = runStage(fixture, {
    syncPath: (targetPath) => {
      recoverySyncPaths.push(targetPath);
      fsyncFixturePath(targetPath);
    },
  });
  assert.equal(recoveredStagingResult.alreadyStaged, true);
  assert.ok(
    recoverySyncPaths.includes(path.join(fixture.coreRoot, "incoming", candidateCommit))
  );
  assert.ok(recoverySyncPaths.includes(path.join(fixture.coreRoot, "incoming")));

  fixture = createManagerFixture({ candidateStaged: false });
  const interruptedStaging = path.join(
    fixture.coreRoot,
    "incoming",
    `.staging-${candidateCommit}-${"a".repeat(16)}`
  );
  fs.mkdirSync(interruptedStaging, { mode: 0o700 });
  fs.chmodSync(interruptedStaging, 0o700);
  fs.writeFileSync(path.join(interruptedStaging, "partial"), "incomplete\n");
  const recoveredInterruptedStaging = runStage(fixture);
  assert.equal(recoveredInterruptedStaging.alreadyStaged, false);
  assert.equal(recoveredInterruptedStaging.quarantined, true);
  assert.deepEqual(fs.readdirSync(path.join(fixture.coreRoot, "incoming")), [
    candidateCommit,
  ]);
  assert.equal(fs.readdirSync(path.join(fixture.coreRoot, "quarantine")).length, 1);

  fixture = createManagerFixture({ candidateStaged: false });
  const invalidInterruptedStaging = path.join(
    fixture.coreRoot,
    "incoming",
    `.staging-${candidateCommit}-${"b".repeat(16)}`
  );
  fs.mkdirSync(invalidInterruptedStaging, { mode: 0o755 });
  fs.chmodSync(invalidInterruptedStaging, 0o755);
  assertStageError(() => runStage(fixture), "candidate_location_invalid");
  assert.equal(fs.existsSync(invalidInterruptedStaging), true);

  fixture = createManagerFixture({ candidateStaged: false });
  fs.writeFileSync(path.join(fixture.coreRoot, "incoming", "unexpected"), "x");
  assertStageError(() => runStage(fixture), "candidate_location_invalid");

  const invalidStageCli = spawnSync(
    process.execPath,
    [stagerPath, "--stage", "--source", path.join(tmpRoot, "escape")],
    { cwd: tmpRoot, encoding: "utf8" }
  );
  assert.equal(invalidStageCli.status, 2);
  assert.match(
    invalidStageCli.stderr,
    /hermes_core_candidate_staging_error=invalid_invocation/
  );
  assert.equal(invalidStageCli.stderr.includes(tmpRoot), false);

  fixture = createManagerFixture();
  assertPlanError(() => runPlan(fixture, { lockHeld: false }), "manager_lock_not_held");

  fixture = createManagerFixture();
  const wrongLockPath = path.join(fixture.coreRoot, "state", "wrong.lock");
  fs.writeFileSync(wrongLockPath, "");
  fs.chmodSync(wrongLockPath, 0o600);
  const wrongLockFd = fs.openSync(wrongLockPath, "r+");
  try {
    assertPlanError(
      () => runPlan(fixture, { lockFd: wrongLockFd }),
      "manager_lock_not_held"
    );
  } finally {
    fs.closeSync(wrongLockFd);
  }

  fixture = createManagerFixture();
  assertPlanError(
    () =>
      runPlan(fixture, {
        expected: { ...fixture.expected, currentCommit: "4".repeat(40) },
      }),
    "lineage_pointer_invalid"
  );

  fixture = createManagerFixture();
  assertPlanError(
    () =>
      runPlan(fixture, {
        expected: { ...fixture.expected, previousCommit: currentCommit },
      }),
    "lineage_identity_invalid"
  );

  fixture = createManagerFixture();
  const lineageManifestPath = activeLineageManifestPath(fixture);
  const invalidLineage = JSON.parse(fs.readFileSync(lineageManifestPath, "utf8"));
  invalidLineage.unexpected = true;
  assert.equal(lineageValidator(invalidLineage), false);
  rewriteReadOnly(lineageManifestPath, `${JSON.stringify(invalidLineage)}\n`);
  assertPlanError(() => runPlan(fixture), "lineage_identity_invalid");

  fixture = createManagerFixture();
  fs.chmodSync(fixture.coreRoot, 0o777);
  assertPlanError(() => runPlan(fixture), "core_root_mode_invalid");

  fixture = createManagerFixture();
  replaceCandidateWithSymlink(fixture);
  assertPlanError(() => runPlan(fixture), "release_artifact_invalid");

  fixture = createManagerFixture();
  fs.writeFileSync(path.join(fixture.coreRoot, "unexpected"), "rejected\n");
  assertPlanError(() => runPlan(fixture), "core_root_layout_invalid");

  fixture = createManagerFixture();
  assertPlanError(
    () => runPlan(fixture, { minimumFreeBytes: Number.MAX_SAFE_INTEGER }),
    "core_root_space_insufficient"
  );

  fixture = createManagerFixture();
  const currentMain = path.join(
    fixture.coreRoot,
    "releases",
    currentCommit,
    "core",
    "hermes_cli",
    "main.py"
  );
  rewriteReadOnly(currentMain, "tampered\n");
  assertPlanError(() => runPlan(fixture), "release_artifact_invalid");

  fixture = createManagerFixture();
  assertPlanError(
    () =>
      runPlan(fixture, {
        expected: { ...fixture.expected, identitySha256: "f".repeat(64) },
      }),
    "release_artifact_invalid"
  );

  const invalidCli = spawnSync(
    process.execPath,
    [plannerPath, "--dry-run", "--core-root", path.join(tmpRoot, "escape")],
    { cwd: tmpRoot, encoding: "utf8" }
  );
  assert.equal(invalidCli.status, 2);
  assert.match(invalidCli.stderr, /hermes_core_release_error=invalid_invocation/);
  assert.equal(invalidCli.stderr.includes(tmpRoot), false);
} finally {
  makeWritable(tmpRoot);
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Hermes core staging and dry-run planner test passed.");

function runPlan(fixture, overrides = {}) {
  const lockHeld = overrides.lockHeld ?? true;
  const suppliedLockFd = overrides.lockFd;
  const lockFd =
    suppliedLockFd ??
    (lockHeld
      ? fs.openSync(path.join(fixture.coreRoot, "state", "manager.lock"), "r+")
      : null);
  try {
    return planHermesCoreRelease({
      expected: overrides.expected ?? fixture.expected,
      coreRoot: fixture.coreRoot,
      trustedParent: fixture.trustedParent,
      owner: overrides.owner ?? owner,
      minimumFreeBytes: overrides.minimumFreeBytes ?? 0,
      lockHeld,
      lockFd,
      verifyInheritedLock: lockHeld,
    });
  } finally {
    if (suppliedLockFd === undefined && lockFd !== null) {
      fs.closeSync(lockFd);
    }
  }
}

function runStage(fixture, overrides = {}) {
  const lockHeld = overrides.lockHeld ?? true;
  const suppliedLockFd = overrides.lockFd;
  const lockFd =
    suppliedLockFd ??
    (lockHeld
      ? fs.openSync(path.join(fixture.coreRoot, "state", "manager.lock"), "r+")
      : null);
  try {
    return stageHermesCoreRelease({
      expected: overrides.expected ?? fixture.expected,
      coreRoot: fixture.coreRoot,
      coreTrustedParent: fixture.trustedParent,
      ingressRoot: fixture.ingressRoot,
      ingressTrustedParent: fixture.ingressTrustedParent,
      owner: overrides.owner ?? owner,
      minimumFreeBytes: overrides.minimumFreeBytes ?? 0,
      lockHeld,
      lockFd,
      verifyInheritedLock: lockHeld,
      copyFile: overrides.copyFile,
      syncPath: overrides.syncPath,
    });
  } finally {
    if (suppliedLockFd === undefined && lockFd !== null) {
      fs.closeSync(lockFd);
    }
  }
}

function runCommit(fixture, overrides = {}) {
  const lockHeld = overrides.lockHeld ?? true;
  const suppliedLockFd = overrides.lockFd;
  const lockFd =
    suppliedLockFd ??
    (lockHeld
      ? fs.openSync(path.join(fixture.coreRoot, "state", "manager.lock"), "r+")
      : null);
  try {
    return commitHermesCoreLineageForTest({
      expected: overrides.expected ?? fixture.expected,
      coreRoot: fixture.coreRoot,
      trustedParent: fixture.trustedParent,
      owner: overrides.owner ?? owner,
      minimumFreeBytes: overrides.minimumFreeBytes ?? 0,
      lockHeld,
      lockFd,
      verifyInheritedLock: lockHeld,
      syncPath: overrides.syncPath,
    });
  } finally {
    if (suppliedLockFd === undefined && lockFd !== null) {
      fs.closeSync(lockFd);
    }
  }
}

function runRestore(fixture, overrides = {}) {
  const lockHeld = overrides.lockHeld ?? true;
  const suppliedLockFd = overrides.lockFd;
  const lockFd =
    suppliedLockFd ??
    (lockHeld
      ? fs.openSync(path.join(fixture.coreRoot, "state", "manager.lock"), "r+")
      : null);
  try {
    return restoreHermesCoreLineageForTest({
      fromLineage: overrides.fromLineage ?? expectedNextLineage(fixture),
      toLineage: overrides.toLineage ?? originalLineage(fixture),
      coreRoot: fixture.coreRoot,
      trustedParent: fixture.trustedParent,
      owner: overrides.owner ?? owner,
      lockHeld,
      lockFd,
      verifyInheritedLock: lockHeld,
      syncPath: overrides.syncPath,
    });
  } finally {
    if (suppliedLockFd === undefined && lockFd !== null) {
      fs.closeSync(lockFd);
    }
  }
}

function createManagerFixture({
  candidateStaged = true,
  candidatePreviousCommit = currentCommit,
} = {}) {
  sequence += 1;
  const trustedParent = path.join(tmpRoot, `trusted-${sequence}`);
  const coreRoot = path.join(trustedParent, "qintopia-hermes-core");
  fs.mkdirSync(path.join(coreRoot, "releases"), { recursive: true });
  fs.mkdirSync(path.join(coreRoot, "incoming"));
  fs.mkdirSync(path.join(coreRoot, "quarantine"));
  fs.mkdirSync(path.join(coreRoot, "lineage", "generations"), {
    recursive: true,
  });
  fs.mkdirSync(path.join(coreRoot, "state", "transactions"), { recursive: true });
  fs.writeFileSync(path.join(coreRoot, "state", "manager.lock"), "");

  createArtifact(path.join(coreRoot, "releases", currentCommit), {
    commit: currentCommit,
    tag: "v1.2.2",
    version: "1.2.2",
    previousCommit,
    sourceCharacter: "a",
  });
  createArtifact(path.join(coreRoot, "releases", previousCommit), {
    commit: previousCommit,
    tag: "v1.2.1",
    version: "1.2.1",
    previousCommit: null,
    sourceCharacter: "b",
  });
  const ingressTrustedParent = path.join(trustedParent, "deploy-state");
  const ingressRoot = path.join(ingressTrustedParent, "hermes-core-ingress");
  fs.mkdirSync(ingressRoot, { recursive: true });
  const candidateOptions = {
    commit: candidateCommit,
    tag: "v1.2.3",
    version: "1.2.3",
    previousCommit: candidatePreviousCommit,
    sourceCharacter: "c",
  };
  const candidate = createArtifact(
    path.join(ingressRoot, candidateCommit),
    candidateOptions
  );
  if (candidateStaged) {
    createArtifact(path.join(coreRoot, "incoming", candidateCommit), candidateOptions);
  }
  const lineageIdentity = {
    currentCommit,
    previousCommit,
    rollbackReserveCommit: previousCommit,
  };
  const fingerprint = computeHermesCoreLineageFingerprint(lineageIdentity);
  const generationName = `generation-${fingerprint}`;
  const generationRoot = path.join(coreRoot, "lineage", "generations", generationName);
  fs.mkdirSync(generationRoot);
  const lineageManifest = {
    schema_version: 1,
    generation_type: "hermes-core-lineage",
    fingerprint_sha256: fingerprint,
    current_commit: currentCommit,
    previous_commit: previousCommit,
    rollback_reserve_commit: previousCommit,
  };
  assert.equal(lineageValidator(lineageManifest), true);
  writeJson(path.join(generationRoot, "lineage.json"), lineageManifest);
  for (const [role, commit] of [
    ["current", currentCommit],
    ["previous", previousCommit],
    ["rollback-reserve", previousCommit],
  ]) {
    fs.symlinkSync(`../../../releases/${commit}`, path.join(generationRoot, role));
    fs.symlinkSync(`lineage/active/${role}`, path.join(coreRoot, role));
  }
  fs.symlinkSync(
    `generations/${generationName}`,
    path.join(coreRoot, "lineage", "active")
  );

  fs.chmodSync(trustedParent, 0o700);
  fs.chmodSync(coreRoot, 0o755);
  fs.chmodSync(path.join(coreRoot, "releases"), 0o755);
  fs.chmodSync(path.join(coreRoot, "lineage"), 0o755);
  fs.chmodSync(path.join(coreRoot, "lineage", "generations"), 0o755);
  fs.chmodSync(path.join(generationRoot, "lineage.json"), 0o444);
  fs.chmodSync(generationRoot, 0o555);
  fs.chmodSync(path.join(coreRoot, "incoming"), 0o700);
  fs.chmodSync(path.join(coreRoot, "quarantine"), 0o700);
  fs.chmodSync(path.join(coreRoot, "state"), 0o700);
  fs.chmodSync(path.join(coreRoot, "state", "transactions"), 0o700);
  fs.chmodSync(path.join(coreRoot, "state", "manager.lock"), 0o600);
  fs.chmodSync(ingressTrustedParent, 0o700);
  fs.chmodSync(ingressRoot, 0o700);

  return {
    trustedParent,
    coreRoot,
    ingressTrustedParent,
    ingressRoot,
    expected: {
      tag: candidate.tag,
      commit: candidate.commit,
      sourceSha256: candidate.sourceSha256,
      identitySha256: candidate.identitySha256,
      manifestSha256: candidate.manifestSha256,
      currentCommit,
      previousCommit,
      rollbackReserveCommit: previousCommit,
    },
  };
}

function createArtifact(root, options) {
  const core = path.join(root, "core");
  const packageDirectory = path.join(core, "hermes_cli");
  fs.mkdirSync(packageDirectory, { recursive: true });
  fs.writeFileSync(
    path.join(core, "pyproject.toml"),
    "[project]\nname='hermes-agent'\n"
  );
  fs.writeFileSync(path.join(packageDirectory, "main.py"), "print('fixture')\n");
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
  const identitySource = {
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
  const identitySha256 = computeHermesCoreArtifactIdentity(identitySource);
  const sharedIdentity = {
    ...identitySource,
    artifact_identity_sha256: identitySha256,
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
    repository: identitySource.repository,
    tag: options.tag,
    commit_sha: options.commit,
    source_archive_sha256: sourceSha256,
    artifact_identity_sha256: identitySha256,
    previous_version: "1.2.0",
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
    artifact_identity_sha256: identitySha256,
    runtime_binding_sha256: identitySource.runtime_binding_sha256,
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
  chmodTree(core);
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
      build: receiptPointer(root, "build-receipt.json"),
      update: receiptPointer(root, "update-receipt.json"),
      validation: receiptPointer(root, "validation-summary.json"),
    },
  };
  writeJson(path.join(root, "artifact-manifest.json"), manifest);
  for (const name of [
    "artifact-manifest.json",
    "build-receipt.json",
    "update-receipt.json",
    "validation-summary.json",
  ]) {
    fs.chmodSync(path.join(root, name), 0o444);
  }
  writeChecksums(root);
  fs.chmodSync(root, 0o555);
  return {
    tag: options.tag,
    commit: options.commit,
    sourceSha256,
    identitySha256,
    manifestSha256: sha256File(path.join(root, "artifact-manifest.json")),
  };
}

function receiptPointer(root, name) {
  return { path: name, sha256: sha256File(path.join(root, name)) };
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

function writeChecksums(root) {
  const files = [
    "artifact-manifest.json",
    "build-receipt.json",
    "update-receipt.json",
    "validation-summary.json",
    ...inventoryCore(root)
      .filter((entry) => entry.type === "file")
      .map((entry) => entry.path),
  ].sort();
  fs.writeFileSync(
    path.join(root, "SHA256SUMS"),
    `${files
      .map(
        (relativePath) =>
          `${sha256File(path.join(root, ...relativePath.split("/")))}  ${relativePath}`
      )
      .join("\n")}\n`
  );
  fs.chmodSync(path.join(root, "SHA256SUMS"), 0o444);
}

function chmodTree(directoryPath) {
  for (const name of fs.readdirSync(directoryPath)) {
    const entryPath = path.join(directoryPath, name);
    const metadata = fs.lstatSync(entryPath);
    if (metadata.isDirectory()) {
      chmodTree(entryPath);
    } else {
      fs.chmodSync(entryPath, 0o444);
    }
  }
  fs.chmodSync(directoryPath, 0o555);
}

function snapshotTree(root) {
  const snapshot = [];
  const visit = (entryPath, relativePath) => {
    const metadata = fs.lstatSync(entryPath);
    const record = {
      path: relativePath,
      mode: metadata.mode & 0o7777,
      size: metadata.size,
      type: metadata.isSymbolicLink()
        ? "symlink"
        : metadata.isDirectory()
          ? "directory"
          : "file",
    };
    if (metadata.isSymbolicLink()) {
      record.target = fs.readlinkSync(entryPath);
    } else if (metadata.isFile()) {
      record.sha256 = sha256File(entryPath);
    }
    snapshot.push(record);
    if (metadata.isDirectory()) {
      for (const name of fs.readdirSync(entryPath).sort()) {
        visit(path.join(entryPath, name), path.posix.join(relativePath, name));
      }
    }
  };
  visit(root, ".");
  return snapshot;
}

function replaceCandidateWithSymlink(fixture) {
  const candidate = path.join(fixture.coreRoot, "incoming", candidateCommit);
  const moved = path.join(tmpRoot, `moved-candidate-${sequence}`);
  makeWritable(candidate);
  fs.renameSync(candidate, moved);
  fs.symlinkSync(moved, candidate, "dir");
}

function replaceIngressCandidateWithSymlink(fixture) {
  const candidate = path.join(fixture.ingressRoot, candidateCommit);
  const moved = path.join(tmpRoot, `moved-ingress-candidate-${sequence}`);
  makeWritable(candidate);
  fs.renameSync(candidate, moved);
  fs.symlinkSync(moved, candidate, "dir");
}

function addIngressHardlink(fixture) {
  const packageDirectory = path.join(
    fixture.ingressRoot,
    candidateCommit,
    "core",
    "hermes_cli"
  );
  fs.chmodSync(packageDirectory, 0o755);
  fs.linkSync(
    path.join(packageDirectory, "main.py"),
    path.join(packageDirectory, "main-hardlink.py")
  );
  fs.chmodSync(packageDirectory, 0o555);
}

function activeLineageManifestPath(fixture) {
  return path.join(
    fs.realpathSync.native(path.join(fixture.coreRoot, "lineage", "active")),
    "lineage.json"
  );
}

function expectedNextLineage(fixture) {
  return {
    currentCommit: fixture.expected.commit,
    previousCommit: fixture.expected.currentCommit,
    rollbackReserveCommit: fixture.expected.previousCommit,
  };
}

function originalLineage(fixture) {
  return {
    currentCommit: fixture.expected.currentCommit,
    previousCommit: fixture.expected.previousCommit,
    rollbackReserveCommit: fixture.expected.rollbackReserveCommit,
  };
}

function expectedNextActiveTarget(fixture) {
  const fingerprint = computeHermesCoreLineageFingerprint(expectedNextLineage(fixture));
  return `generations/generation-${fingerprint}`;
}

function expectedOriginalActiveTarget(fixture) {
  const fingerprint = computeHermesCoreLineageFingerprint(originalLineage(fixture));
  return `generations/generation-${fingerprint}`;
}

function expectedNextGenerationPath(fixture) {
  return path.join(fixture.coreRoot, "lineage", expectedNextActiveTarget(fixture));
}

function assertCommittedLineage(fixture) {
  assert.deepEqual(fs.readdirSync(path.join(fixture.coreRoot, "incoming")), []);
  assert.equal(
    fs.readlinkSync(path.join(fixture.coreRoot, "lineage", "active")),
    expectedNextActiveTarget(fixture)
  );
  for (const [role, commit] of [
    ["current", candidateCommit],
    ["previous", currentCommit],
    ["rollback-reserve", previousCommit],
  ]) {
    assert.equal(
      fs.realpathSync.native(path.join(fixture.coreRoot, role)),
      path.join(fixture.coreRoot, "releases", commit)
    );
  }
}

function assertOriginalLineage(fixture) {
  assert.equal(
    fs.readlinkSync(path.join(fixture.coreRoot, "lineage", "active")),
    expectedOriginalActiveTarget(fixture)
  );
  for (const [role, commit] of [
    ["current", currentCommit],
    ["previous", previousCommit],
    ["rollback-reserve", previousCommit],
  ]) {
    assert.equal(
      fs.realpathSync.native(path.join(fixture.coreRoot, role)),
      path.join(fixture.coreRoot, "releases", commit)
    );
  }
}

function rewriteReadOnly(filePath, value) {
  fs.chmodSync(filePath, 0o644);
  fs.writeFileSync(filePath, value);
  fs.chmodSync(filePath, 0o444);
}

function assertPlanError(callback, expectedCode) {
  assert.throws(callback, (error) => error?.message === expectedCode);
}

function assertStageError(callback, expectedCode) {
  assert.throws(callback, (error) => error?.message === expectedCode);
}

function assertCommitError(callback, expectedCode) {
  assert.throws(callback, (error) => error?.message === expectedCode);
}

function writeJson(filePath, value) {
  fs.writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`);
}

function sha256File(filePath) {
  return crypto.createHash("sha256").update(fs.readFileSync(filePath)).digest("hex");
}

function modeOfPath(targetPath) {
  return fs.lstatSync(targetPath).mode & 0o7777;
}

function fsyncFixturePath(targetPath) {
  const descriptor = fs.openSync(targetPath, "r");
  try {
    fs.fsyncSync(descriptor);
  } finally {
    fs.closeSync(descriptor);
  }
}

function makeWritable(directoryPath) {
  if (!fs.existsSync(directoryPath)) {
    return;
  }
  const metadata = fs.lstatSync(directoryPath);
  if (metadata.isSymbolicLink()) {
    return;
  }
  if (metadata.isDirectory()) {
    fs.chmodSync(directoryPath, 0o700);
    for (const name of fs.readdirSync(directoryPath)) {
      makeWritable(path.join(directoryPath, name));
    }
  } else {
    fs.chmodSync(directoryPath, 0o600);
  }
}
