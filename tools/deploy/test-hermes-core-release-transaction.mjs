#!/usr/bin/env node

import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import Ajv2020 from "ajv/dist/2020.js";
import { loadHermesProfileRegistry } from "./hermes-profile-registry.mjs";
import {
  executeHermesCoreReleaseTransactionForTest,
  HermesCoreTransactionCrash,
} from "./run-hermes-core-release-transaction.mjs";

const tmpRoot = fs.realpathSync.native(
  fs.mkdtempSync(path.join(os.tmpdir(), "hermes-core-transaction-"))
);
const owner = { uid: process.getuid(), gid: process.getgid() };
const profiles = loadHermesProfileRegistry();
const currentCommit = "1".repeat(40);
const previousCommit = "2".repeat(40);
const rollbackReserveCommit = "4".repeat(40);
const candidateCommit = "3".repeat(40);
const expected = {
  tag: "v3.0.0",
  commit: candidateCommit,
  sourceSha256: "a".repeat(64),
  identitySha256: "b".repeat(64),
  manifestSha256: "c".repeat(64),
  currentCommit,
  previousCommit,
  rollbackReserveCommit,
};
const journalValidator = new Ajv2020({ allErrors: true, strict: true }).compile(
  JSON.parse(
    fs.readFileSync(
      path.join(
        process.cwd(),
        "runtime",
        "hermes",
        "core-release-contracts",
        "transaction-journal.schema.json"
      ),
      "utf8"
    )
  )
);
let sequence = 0;

try {
  testSuccessfulTransactionAndReplay();
  testMixedActiveAndInactiveStatesArePreserved();
  testStartFailureRollsBackInReverseOrder();
  testRollbackRestoresUnitFileState();
  testCommitUncertaintyRollsBack();
  testCrashRecoveryAfterLineageCommit();
  testCrashRecoveryDuringCandidateStartup();
  testPreCommitCrashDoesNotStopOriginalServices();
  testRollbackCheckpointCrashesRecover();
  testRollbackFailureIsNotReportedAsSuccess();
  testRollbackStopFailureBlocksLineageRestoreAndOriginalStart();
  testRollbackReloadFailureBlocksOriginalStart();
  testJournalCommitUncertaintyIsDurablyRecovered();
  testPartialPendingJournalRecovery();
  testMissingLockDoesNotWriteJournal();
  testJournalTamperingFailsClosed();
  testLegalTerminalPhaseTamperCannotBypassRecovery();
  testArtifactIdentityDriftFailsClosed();
  testOriginalLineageIsVerifiedBeforeJournalAndServiceChanges();
  testUnrestorableSystemdStateFailsBeforeJournal();
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Hermes core release transaction test passed.");

function testSuccessfulTransactionAndReplay() {
  const fixture = createFixture();
  const result = runTransaction(fixture);
  assert.deepEqual(result, {
    status: "committed",
    alreadyCommitted: false,
    recovered: false,
    candidateCommit,
    serviceChanges: 14,
    pointerChanges: 1,
  });
  assert.equal(fixture.lineage.currentCommit, candidateCommit);
  assertAllServices(fixture, candidateCommit, true);
  const journal = readJournal(fixture);
  assert.equal(
    journalValidator(journal),
    true,
    JSON.stringify(journalValidator.errors)
  );
  assert.equal(journal.phase, "committed");
  assert.equal(journal.rollback.status, "not_attempted");
  assert.deepEqual(
    fixture.log.filter((entry) => entry.startsWith("stop:")),
    profiles.map((profile) => `stop:${profile.id}`)
  );
  assert.deepEqual(
    fixture.log.filter((entry) => entry.startsWith("start:")),
    profiles.map((profile) => `start:${profile.id}:${candidateCommit}`)
  );
  assert.ok(
    fixture.log.indexOf("lineage:commit") >
      fixture.log.indexOf(`stop:${profiles.at(-1).id}`)
  );
  assert.ok(
    fixture.log.indexOf(`start:${profiles[0].id}:${candidateCommit}`) >
      fixture.log.indexOf("lineage:commit")
  );

  const replay = runTransaction(fixture);
  assert.equal(replay.alreadyCommitted, true);
  assert.equal(replay.serviceChanges, 0);
  assert.equal(replay.pointerChanges, 0);
  assertAllServices(fixture, candidateCommit, true);
}

function testMixedActiveAndInactiveStatesArePreserved() {
  const inactiveProfiles = new Set(["silaoshi", "wenyuange"]);
  const fixture = createFixture({ inactiveProfiles });
  const result = runTransaction(fixture);
  assert.equal(result.serviceChanges, (profiles.length - inactiveProfiles.size) * 2);
  for (const profile of profiles) {
    const service = fixture.services.get(profile.id);
    assert.equal(
      service.activeState,
      inactiveProfiles.has(profile.id) ? "inactive" : "active"
    );
    assert.equal(
      service.runningCommit,
      inactiveProfiles.has(profile.id) ? currentCommit : candidateCommit
    );
  }
  assert.deepEqual(
    fixture.log.filter((entry) => entry.startsWith("stop:")),
    profiles
      .filter((profile) => !inactiveProfiles.has(profile.id))
      .map((profile) => `stop:${profile.id}`)
  );
  assert.deepEqual(
    fixture.log.filter((entry) => entry.startsWith("start:")),
    profiles
      .filter((profile) => !inactiveProfiles.has(profile.id))
      .map((profile) => `start:${profile.id}:${candidateCommit}`)
  );
  assert.deepEqual(
    fixture.log.filter((entry) => entry.startsWith("smoke:")),
    profiles
      .filter((profile) => !inactiveProfiles.has(profile.id))
      .map((profile) => `smoke:${profile.id}:${candidateCommit}`)
  );
  const journal = readJournal(fixture);
  assert.deepEqual(
    journal.services.map((service) => service.active_state),
    profiles.map((profile) =>
      inactiveProfiles.has(profile.id) ? "inactive" : "active"
    )
  );
}

function testStartFailureRollsBackInReverseOrder() {
  const fixture = createFixture({ failStartOnceAfterSideEffect: "huabaosi" });
  assertTransactionError(
    () => runTransaction(fixture),
    "hermes_core_transaction_rolled_back",
    "succeeded",
    "hermes_core_candidate_start_failed"
  );
  assert.equal(fixture.lineage.currentCommit, currentCommit);
  assertAllServices(fixture, currentCommit, true);
  const journal = readJournal(fixture);
  assert.equal(journal.phase, "rolled_back");
  assert.equal(journal.rollback.status, "succeeded");
  const rollbackMarker = fixture.log.indexOf("lineage:restore");
  const rollbackStops = fixture.log
    .slice(0, rollbackMarker)
    .filter((entry) => entry.startsWith("rollback-stop:"));
  assert.deepEqual(
    rollbackStops,
    ["huabaosi", "guanerye", "erhua", "default"].map(
      (profile) => `rollback-stop:${profile}`
    )
  );
  assert.deepEqual(
    fixture.log.slice(rollbackMarker + 1).filter((entry) => entry.startsWith("start:")),
    profiles.map((profile) => `start:${profile.id}:${currentCommit}`)
  );
}

function testRollbackRestoresUnitFileState() {
  const fixture = createFixture({
    failStartOnceAfterSideEffect: "huabaosi",
    mutateUnitFileStateOnFailedStart: true,
  });
  assertTransactionError(
    () => runTransaction(fixture),
    "hermes_core_transaction_rolled_back",
    "succeeded",
    "hermes_core_candidate_start_failed"
  );
  for (const profile of profiles) {
    assert.equal(
      fixture.services.get(profile.id).unitFileState,
      profile.id === "wenyuange" ? "disabled" : "enabled"
    );
  }
  assert.ok(fixture.log.includes("restore-unit:huabaosi:enabled"));
}

function testCommitUncertaintyRollsBack() {
  const fixture = createFixture({ failCommitAfterSwitch: true });
  assertTransactionError(
    () => runTransaction(fixture),
    "hermes_core_transaction_rolled_back",
    "succeeded",
    "lineage_commit_uncertain"
  );
  assert.equal(fixture.lineage.currentCommit, currentCommit);
  assertAllServices(fixture, currentCommit, true);
}

function testCrashRecoveryAfterLineageCommit() {
  const fixture = createFixture();
  assert.throws(
    () =>
      runTransaction(fixture, {
        crashAt: "lineage_committed",
      }),
    (error) =>
      error instanceof HermesCoreTransactionCrash &&
      error.checkpoint === "lineage_committed"
  );
  assert.equal(fixture.lineage.currentCommit, candidateCommit);
  assertAllServices(fixture, currentCommit, false);
  assertTransactionError(
    () => runTransaction(fixture),
    "hermes_core_transaction_rolled_back",
    "succeeded",
    "hermes_core_transaction_recovery_required"
  );
  assert.equal(fixture.lineage.currentCommit, currentCommit);
  assertAllServices(fixture, currentCommit, true);
}

function testCrashRecoveryDuringCandidateStartup() {
  const fixture = createFixture();
  assert.throws(
    () =>
      runTransaction(fixture, {
        crashAt: "candidate_service_started:guanerye",
      }),
    (error) => error instanceof HermesCoreTransactionCrash
  );
  assert.equal(fixture.lineage.currentCommit, candidateCommit);
  assert.equal(fixture.services.get("default").activeState, "active");
  assert.equal(fixture.services.get("erhua").activeState, "active");
  assert.equal(fixture.services.get("guanerye").activeState, "active");
  assert.equal(fixture.services.get("huabaosi").activeState, "inactive");
  assertTransactionError(
    () => runTransaction(fixture),
    "hermes_core_transaction_rolled_back",
    "succeeded",
    "hermes_core_transaction_recovery_required"
  );
  assert.equal(fixture.lineage.currentCommit, currentCommit);
  assertAllServices(fixture, currentCommit, true);
}

function testPreCommitCrashDoesNotStopOriginalServices() {
  for (const checkpoint of [
    "services_stopping",
    "service_stopped:guanerye",
    "services_stopped",
    "lineage_committing",
  ]) {
    const fixture = createFixture();
    assert.throws(
      () => runTransaction(fixture, { crashAt: checkpoint }),
      (error) =>
        error instanceof HermesCoreTransactionCrash && error.checkpoint === checkpoint
    );
    assert.equal(fixture.lineage.currentCommit, currentCommit);
    assertTransactionError(
      () => runTransaction(fixture),
      "hermes_core_transaction_rolled_back",
      "succeeded",
      "hermes_core_transaction_recovery_required"
    );
    assert.equal(
      fixture.log.some((entry) => entry.startsWith("rollback-stop:")),
      false,
      checkpoint
    );
    assert.equal(fixture.log.includes("lineage:restore"), false, checkpoint);
    assertAllServices(fixture, currentCommit, true);
  }
}

function testRollbackCheckpointCrashesRecover() {
  for (const checkpoint of [
    "rollback_stopping_candidate",
    "rollback_restoring_lineage",
    "rollback_starting_original",
  ]) {
    const fixture = createFixture({ failStartOnceAfterSideEffect: "huabaosi" });
    assert.throws(
      () => runTransaction(fixture, { crashAt: checkpoint }),
      (error) =>
        error instanceof HermesCoreTransactionCrash && error.checkpoint === checkpoint
    );
    assertTransactionError(
      () => runTransaction(fixture),
      "hermes_core_transaction_rolled_back",
      "succeeded",
      "hermes_core_transaction_recovery_required"
    );
    assert.equal(fixture.lineage.currentCommit, currentCommit);
    assertAllServices(fixture, currentCommit, true);
  }
}

function testRollbackFailureIsNotReportedAsSuccess() {
  const fixture = createFixture({
    failStartOnceAfterSideEffect: "huabaosi",
    failRestore: true,
  });
  assertTransactionError(
    () => runTransaction(fixture),
    "hermes_core_transaction_rollback_failed",
    "failed",
    "hermes_core_candidate_start_failed"
  );
  const journal = readJournal(fixture);
  assert.equal(journal.phase, "rollback_failed");
  assert.equal(journal.rollback.status, "failed");
  assert.notEqual(fixture.lineage.currentCommit, currentCommit);
}

function testRollbackStopFailureBlocksLineageRestoreAndOriginalStart() {
  const fixture = createFixture({
    failStartOnceAfterSideEffect: "huabaosi",
    failRollbackStopProfile: "huabaosi",
  });
  assertTransactionError(
    () => runTransaction(fixture),
    "hermes_core_transaction_rollback_failed",
    "failed",
    "hermes_core_candidate_start_failed"
  );
  assert.equal(fixture.lineage.currentCommit, candidateCommit);
  assert.equal(fixture.log.includes("lineage:restore"), false);
  const failureIndex = fixture.log.indexOf("rollback-stop-failed:huabaosi");
  assert.ok(failureIndex >= 0);
  assert.equal(
    fixture.log.slice(failureIndex + 1).some((entry) => entry.startsWith("start:")),
    false
  );
}

function testRollbackReloadFailureBlocksOriginalStart() {
  const fixture = createFixture({
    failStartOnceAfterSideEffect: "huabaosi",
    failRollbackDaemonReload: true,
  });
  assertTransactionError(
    () => runTransaction(fixture),
    "hermes_core_transaction_rollback_failed",
    "failed",
    "hermes_core_candidate_start_failed"
  );
  assert.equal(fixture.lineage.currentCommit, currentCommit);
  const restoreIndex = fixture.log.indexOf("lineage:restore");
  assert.ok(restoreIndex >= 0);
  assert.equal(
    fixture.log.slice(restoreIndex + 1).some((entry) => entry.startsWith("start:")),
    false
  );
}

function testJournalCommitUncertaintyIsDurablyRecovered() {
  const fixture = createFixture();
  let failedAfterRename = false;
  assertTransactionError(
    () =>
      runTransaction(fixture, {
        syncPath: (targetPath) => {
          if (
            !failedAfterRename &&
            targetPath === fixture.transactionsRoot &&
            fs.existsSync(
              path.join(fixture.transactionsRoot, `${candidateCommit}.json`)
            )
          ) {
            failedAfterRename = true;
            throw new Error("fixture journal directory fsync failure");
          }
          fsyncFixturePath(targetPath);
        },
      }),
    "hermes_core_transaction_journal_commit_uncertain",
    "not_attempted",
    null
  );
  const recoverySyncPaths = [];
  assertTransactionError(
    () =>
      runTransaction(fixture, {
        syncPath: (targetPath) => {
          recoverySyncPaths.push(targetPath);
          fsyncFixturePath(targetPath);
        },
      }),
    "hermes_core_transaction_rolled_back",
    "succeeded",
    "hermes_core_transaction_recovery_required"
  );
  assert.ok(
    recoverySyncPaths.includes(
      path.join(fixture.transactionsRoot, `${candidateCommit}.json`)
    )
  );
  assert.ok(recoverySyncPaths.includes(fixture.transactionsRoot));
}

function testPartialPendingJournalRecovery() {
  const fixture = createFixture();
  runTransaction(fixture);
  const pendingPath = path.join(
    fixture.transactionsRoot,
    `.${candidateCommit}.pending`
  );
  fs.writeFileSync(pendingPath, '{"partial":', { mode: 0o600 });
  fs.chmodSync(pendingPath, 0o600);
  const replay = runTransaction(fixture);
  assert.equal(replay.alreadyCommitted, true);
  assert.equal(fs.existsSync(pendingPath), false);

  const unknownFixture = createFixture();
  const unknownPendingPath = path.join(
    unknownFixture.transactionsRoot,
    `.${candidateCommit}.pending`
  );
  fs.writeFileSync(unknownPendingPath, '{"partial":', { mode: 0o600 });
  fs.chmodSync(unknownPendingPath, 0o600);
  assertTransactionError(
    () => runTransaction(unknownFixture),
    "hermes_core_transaction_journal_invalid",
    "not_attempted",
    null
  );
  assert.equal(fs.existsSync(unknownPendingPath), true);
}

function testMissingLockDoesNotWriteJournal() {
  const fixture = createFixture();
  assertTransactionError(
    () => runTransaction(fixture, { lockHeld: false }),
    "manager_lock_not_held",
    "not_attempted",
    null
  );
  assert.deepEqual(fs.readdirSync(fixture.transactionsRoot), []);
  assertAllServices(fixture, currentCommit, true);
}

function testJournalTamperingFailsClosed() {
  const fixture = createFixture({ failStartOnceAfterSideEffect: "huabaosi" });
  assertTransactionError(
    () => runTransaction(fixture),
    "hermes_core_transaction_rolled_back",
    "succeeded",
    "hermes_core_candidate_start_failed"
  );
  const journalPath = path.join(fixture.transactionsRoot, `${candidateCommit}.json`);
  const journal = readJournal(fixture);
  journal.services[0].unit = "unexpected.service";
  fs.writeFileSync(journalPath, `${JSON.stringify(journal)}\n`);
  assertTransactionError(
    () => runTransaction(fixture),
    "hermes_core_transaction_journal_invalid",
    "not_attempted",
    null
  );
  assert.equal(fixture.lineage.currentCommit, currentCommit);
  assertAllServices(fixture, currentCommit, true);
}

function testLegalTerminalPhaseTamperCannotBypassRecovery() {
  const fixture = createFixture();
  runTransaction(fixture);
  const journalPath = path.join(fixture.transactionsRoot, `${candidateCommit}.json`);
  const journal = readJournal(fixture);
  journal.phase = "rolled_back";
  journal.rollback = {
    attempted: true,
    status: "succeeded",
    failure_code: "tampered_phase",
  };
  fs.writeFileSync(journalPath, `${JSON.stringify(journal)}\n`);
  assertTransactionError(
    () => runTransaction(fixture),
    "hermes_core_transaction_rolled_back",
    "succeeded",
    "hermes_core_transaction_recovery_required"
  );
  assert.equal(fixture.lineage.currentCommit, currentCommit);
  assertAllServices(fixture, currentCommit, true);
}

function testArtifactIdentityDriftFailsClosed() {
  const fixture = createFixture();
  runTransaction(fixture);
  assertTransactionError(
    () =>
      runTransaction(fixture, {
        expected: { ...expected, manifestSha256: "d".repeat(64) },
      }),
    "hermes_core_transaction_journal_identity_mismatch",
    "not_attempted",
    null
  );
  assert.equal(fixture.lineage.currentCommit, candidateCommit);
}

function testOriginalLineageIsVerifiedBeforeJournalAndServiceChanges() {
  const fixture = createFixture({ failOriginalVerification: true });
  assertTransactionError(
    () => runTransaction(fixture),
    "original_lineage_invalid",
    "not_attempted",
    null
  );
  assert.deepEqual(fs.readdirSync(fixture.transactionsRoot), []);
  assert.equal(
    fixture.log.some(
      (entry) => entry.startsWith("stop:") || entry.startsWith("start:")
    ),
    false
  );
}

function testUnrestorableSystemdStateFailsBeforeJournal() {
  const fixture = createFixture({
    unitFileStateOverrides: new Map([["default", "static"]]),
  });
  assertTransactionError(
    () => runTransaction(fixture),
    "hermes_core_service_snapshot_invalid",
    "not_attempted",
    null
  );
  assert.deepEqual(fs.readdirSync(fixture.transactionsRoot), []);
  assert.equal(fixture.lineage.currentCommit, currentCommit);
}

function runTransaction(fixture, overrides = {}) {
  const lockHeld = overrides.lockHeld ?? true;
  const lockFd = lockHeld
    ? fs.openSync(path.join(fixture.coreRoot, "state", "manager.lock"), "r+")
    : null;
  let crashed = false;
  try {
    return executeHermesCoreReleaseTransactionForTest({
      expected: overrides.expected ?? expected,
      coreRoot: fixture.coreRoot,
      trustedParent: fixture.trustedParent,
      owner,
      lockHeld,
      lockFd,
      verifyInheritedLock: lockHeld,
      serviceController: fixture.serviceController,
      lineageController: fixture.lineageController,
      syncPath: overrides.syncPath,
      afterCheckpoint: (checkpoint) => {
        if (!crashed && checkpoint === overrides.crashAt) {
          crashed = true;
          throw new HermesCoreTransactionCrash(checkpoint);
        }
      },
    });
  } finally {
    if (lockFd !== null) {
      fs.closeSync(lockFd);
    }
  }
}

function createFixture(options = {}) {
  sequence += 1;
  const trustedParent = path.join(tmpRoot, `trusted-${sequence}`);
  const coreRoot = path.join(trustedParent, "qintopia-hermes-core");
  const transactionsRoot = path.join(coreRoot, "state", "transactions");
  fs.mkdirSync(path.join(coreRoot, "releases"), { recursive: true });
  fs.mkdirSync(path.join(coreRoot, "incoming"));
  fs.mkdirSync(path.join(coreRoot, "quarantine"));
  fs.mkdirSync(path.join(coreRoot, "lineage", "generations"), {
    recursive: true,
  });
  fs.mkdirSync(transactionsRoot, { recursive: true });
  fs.writeFileSync(path.join(coreRoot, "state", "manager.lock"), "");
  for (const [name, mode] of [
    [trustedParent, 0o700],
    [coreRoot, 0o755],
    [path.join(coreRoot, "releases"), 0o755],
    [path.join(coreRoot, "lineage"), 0o755],
    [path.join(coreRoot, "lineage", "generations"), 0o755],
    [path.join(coreRoot, "incoming"), 0o700],
    [path.join(coreRoot, "quarantine"), 0o700],
    [path.join(coreRoot, "state"), 0o700],
    [transactionsRoot, 0o700],
    [path.join(coreRoot, "state", "manager.lock"), 0o600],
  ]) {
    fs.chmodSync(name, mode);
  }
  for (const role of ["current", "previous", "rollback-reserve"]) {
    fs.symlinkSync("lineage/active", path.join(coreRoot, role));
  }

  const lineage = {
    currentCommit,
    previousCommit,
    rollbackReserveCommit,
  };
  const log = [];
  const services = new Map(
    profiles.map((profile) => [
      profile.id,
      {
        loadState: "loaded",
        activeState: options.inactiveProfiles?.has(profile.id) ? "inactive" : "active",
        unitFileState:
          options.unitFileStateOverrides?.get(profile.id) ??
          (profile.id === "wenyuange" ? "disabled" : "enabled"),
        execStartSha256: crypto
          .createHash("sha256")
          .update(`exec:${profile.systemd_user_service}`)
          .digest("hex"),
        runningCommit: currentCommit,
      },
    ])
  );
  let failStartOnceAfterSideEffect = options.failStartOnceAfterSideEffect ?? null;
  let failCommitAfterSwitch = options.failCommitAfterSwitch ?? false;
  let failRollbackStopProfile = options.failRollbackStopProfile ?? null;
  let failRollbackDaemonReload = options.failRollbackDaemonReload ?? false;

  const serviceController = {
    snapshot(profile) {
      const service = services.get(profile.id);
      return {
        loadState: service.loadState,
        activeState: service.activeState,
        unitFileState: service.unitFileState,
        execStartSha256: service.execStartSha256,
      };
    },
    stop(profile) {
      const service = services.get(profile.id);
      if (
        service.activeState === "active" &&
        lineage.currentCommit === candidateCommit
      ) {
        if (failRollbackStopProfile === profile.id) {
          failRollbackStopProfile = null;
          log.push(`rollback-stop-failed:${profile.id}`);
          throw new Error("hermes_core_candidate_service_stop_failed");
        }
        log.push(`rollback-stop:${profile.id}`);
      } else {
        log.push(`stop:${profile.id}`);
      }
      service.activeState = "inactive";
    },
    start(profile) {
      const service = services.get(profile.id);
      service.activeState = "active";
      service.runningCommit = lineage.currentCommit;
      log.push(`start:${profile.id}:${lineage.currentCommit}`);
      if (failStartOnceAfterSideEffect === profile.id) {
        failStartOnceAfterSideEffect = null;
        if (options.mutateUnitFileStateOnFailedStart) {
          service.unitFileState =
            service.unitFileState === "enabled" ? "disabled" : "enabled";
        }
        throw new Error("hermes_core_candidate_start_failed");
      }
    },
    restoreUnitFileState(profile, expectedState) {
      const service = services.get(profile.id);
      service.unitFileState = expectedState;
      log.push(`restore-unit:${profile.id}:${expectedState}`);
    },
    daemonReload() {
      log.push(`daemon-reload:${lineage.currentCommit}`);
      if (failRollbackDaemonReload && lineage.currentCommit === currentCommit) {
        failRollbackDaemonReload = false;
        throw new Error("hermes_core_daemon_reload_failed");
      }
    },
    smoke(profile, commit) {
      const service = services.get(profile.id);
      log.push(`smoke:${profile.id}:${commit}`);
      if (service.activeState !== "active" || service.runningCommit !== commit) {
        throw new Error("hermes_core_profile_smoke_failed");
      }
    },
  };
  const lineageController = {
    prepare(value) {
      assert.equal(value.commit, candidateCommit);
      assert.equal(lineage.currentCommit, currentCommit);
      log.push("lineage:prepare");
    },
    commit() {
      lineage.currentCommit = candidateCommit;
      lineage.previousCommit = currentCommit;
      lineage.rollbackReserveCommit = previousCommit;
      log.push("lineage:commit");
      if (failCommitAfterSwitch) {
        failCommitAfterSwitch = false;
        throw new Error("lineage_commit_uncertain");
      }
    },
    restore(fromLineage, toLineage) {
      assert.equal(fromLineage.currentCommit, candidateCommit);
      assert.equal(toLineage.currentCommit, currentCommit);
      log.push("lineage:restore");
      if (options.failRestore) {
        throw new Error("lineage_rollback_commit_uncertain");
      }
      lineage.currentCommit = currentCommit;
      lineage.previousCommit = previousCommit;
      lineage.rollbackReserveCommit = rollbackReserveCommit;
    },
    verify(value) {
      if (options.failOriginalVerification && value.currentCommit === currentCommit) {
        throw new Error("original_lineage_invalid");
      }
      if (
        lineage.currentCommit !== value.currentCommit ||
        lineage.previousCommit !== value.previousCommit ||
        lineage.rollbackReserveCommit !== value.rollbackReserveCommit
      ) {
        throw new Error("lineage_pointer_invalid");
      }
      log.push(`lineage:verify:${value.currentCommit}`);
    },
    verifyCandidate(value) {
      assert.deepEqual(
        {
          tag: value.tag,
          commit: value.commit,
          sourceSha256: value.sourceSha256,
          identitySha256: value.identitySha256,
          manifestSha256: value.manifestSha256,
        },
        {
          tag: expected.tag,
          commit: expected.commit,
          sourceSha256: expected.sourceSha256,
          identitySha256: expected.identitySha256,
          manifestSha256: expected.manifestSha256,
        }
      );
      if (options.failCandidateVerification) {
        throw new Error("candidate_artifact_invalid");
      }
      log.push(`candidate:verify:${value.commit}`);
    },
  };
  return {
    trustedParent,
    coreRoot,
    transactionsRoot,
    lineage,
    services,
    serviceController,
    lineageController,
    log,
  };
}

function readJournal(fixture) {
  return JSON.parse(
    fs.readFileSync(
      path.join(fixture.transactionsRoot, `${candidateCommit}.json`),
      "utf8"
    )
  );
}

function assertAllServices(fixture, commit, active) {
  for (const service of fixture.services.values()) {
    assert.equal(service.activeState, active ? "active" : "inactive");
    assert.equal(service.runningCommit, commit);
  }
}

function fsyncFixturePath(targetPath) {
  const descriptor = fs.openSync(targetPath, "r");
  try {
    fs.fsyncSync(descriptor);
  } finally {
    fs.closeSync(descriptor);
  }
}

function assertTransactionError(callback, code, rollbackStatus, causeCode) {
  assert.throws(callback, (error) => {
    assert.equal(error?.message, code);
    assert.equal(error?.rollbackStatus, rollbackStatus);
    assert.equal(error?.causeCode, causeCode);
    return true;
  });
}
