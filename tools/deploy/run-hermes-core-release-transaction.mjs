#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import {
  commitHermesCoreLineageForTest,
  restoreHermesCoreLineageForTest,
} from "./commit-hermes-core-lineage.mjs";
import { loadHermesProfileRegistry } from "./hermes-profile-registry.mjs";
import { verifyHermesCoreManagerLockBoundary } from "./plan-hermes-core-release.mjs";

const COMMIT_PATTERN = /^[0-9a-f]{40}$/;
const SHA256_PATTERN = /^[0-9a-f]{64}$/;
const MAX_JOURNAL_BYTES = 256 * 1024;
const TERMINAL_PHASES = new Set(["committed", "rolled_back", "rollback_failed"]);
const PHASES = new Set([
  "prepared",
  "services_stopping",
  "services_stopped",
  "lineage_committing",
  "lineage_committed",
  "services_starting",
  "smoke_running",
  "committed",
  "rollback_stopping_candidate",
  "rollback_restoring_lineage",
  "rollback_starting_original",
  "rolled_back",
  "rollback_failed",
]);
const LOAD_STATES = new Set(["loaded"]);
const ACTIVE_STATES = new Set(["active", "inactive"]);
const UNIT_FILE_STATES = new Set(["enabled", "disabled"]);
const JOURNAL_KEYS = new Set([
  "schema_version",
  "transaction_type",
  "revision",
  "candidate_commit",
  "candidate_artifact",
  "original_lineage",
  "candidate_lineage",
  "phase",
  "services",
  "stopped_services",
  "started_candidate_services",
  "rollback",
]);
const ARTIFACT_KEYS = new Set([
  "tag",
  "commit",
  "source_sha256",
  "identity_sha256",
  "manifest_sha256",
]);

export class HermesCoreTransactionError extends Error {
  constructor(code, options = {}) {
    super(code);
    this.name = "HermesCoreTransactionError";
    this.rollbackStatus = options.rollbackStatus ?? "not_attempted";
    this.causeCode = options.causeCode ?? null;
  }
}

export class HermesCoreTransactionCrash extends Error {
  constructor(checkpoint) {
    super("hermes_core_transaction_simulated_crash");
    this.name = "HermesCoreTransactionCrash";
    this.checkpoint = checkpoint;
  }
}

const fail = (code, options) => {
  throw new HermesCoreTransactionError(code, options);
};

const exactKeys = (value, expected) => {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return false;
  }
  const keys = Object.keys(value);
  return keys.length === expected.size && keys.every((key) => expected.has(key));
};

const sameOwner = (metadata, owner) =>
  metadata.uid === owner.uid && metadata.gid === owner.gid;

const modeOf = (metadata) => metadata.mode & 0o7777;

const lineageRecord = (lineage) => ({
  current_commit: lineage.currentCommit,
  previous_commit: lineage.previousCommit,
  rollback_reserve_commit: lineage.rollbackReserveCommit,
});

const lineageFromRecord = (lineage) => ({
  currentCommit: lineage.current_commit,
  previousCommit: lineage.previous_commit,
  rollbackReserveCommit: lineage.rollback_reserve_commit,
});

const candidateLineage = (expected) => ({
  currentCommit: expected.commit,
  previousCommit: expected.currentCommit,
  rollbackReserveCommit: expected.previousCommit,
});

const artifactRecord = (expected) => ({
  tag: expected.tag,
  commit: expected.commit,
  source_sha256: expected.sourceSha256,
  identity_sha256: expected.identitySha256,
  manifest_sha256: expected.manifestSha256,
});

const originalLineage = (expected) => ({
  currentCommit: expected.currentCommit,
  previousCommit: expected.previousCommit,
  rollbackReserveCommit: expected.rollbackReserveCommit,
});

function normalizeFailure(error, fallback) {
  if (error instanceof HermesCoreTransactionError) {
    return error.message;
  }
  if (error instanceof Error && /^[a-z][a-z0-9_]+$/.test(error.message)) {
    return error.message;
  }
  return fallback;
}

function validateExpected(expected) {
  if (
    !expected ||
    typeof expected.tag !== "string" ||
    !/^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/.test(expected.tag) ||
    !COMMIT_PATTERN.test(expected.commit) ||
    !SHA256_PATTERN.test(expected.sourceSha256) ||
    !SHA256_PATTERN.test(expected.identitySha256) ||
    !SHA256_PATTERN.test(expected.manifestSha256) ||
    !COMMIT_PATTERN.test(expected.currentCommit) ||
    !COMMIT_PATTERN.test(expected.previousCommit) ||
    !COMMIT_PATTERN.test(expected.rollbackReserveCommit) ||
    expected.commit === expected.currentCommit ||
    expected.commit === expected.previousCommit ||
    expected.commit === expected.rollbackReserveCommit ||
    expected.currentCommit === expected.previousCommit
  ) {
    fail("hermes_core_transaction_identity_invalid");
  }
}

function validateArtifactRecord(value, candidateCommit) {
  return (
    exactKeys(value, ARTIFACT_KEYS) &&
    /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/.test(value.tag) &&
    value.commit === candidateCommit &&
    COMMIT_PATTERN.test(value.commit) &&
    SHA256_PATTERN.test(value.source_sha256) &&
    SHA256_PATTERN.test(value.identity_sha256) &&
    SHA256_PATTERN.test(value.manifest_sha256)
  );
}

function isExactPrefix(value, expected) {
  return (
    value.length <= expected.length &&
    value.every((item, index) => item === expected[index])
  );
}

function validateSnapshot(snapshot) {
  if (
    !snapshot ||
    typeof snapshot !== "object" ||
    Array.isArray(snapshot) ||
    !LOAD_STATES.has(snapshot.loadState) ||
    !ACTIVE_STATES.has(snapshot.activeState) ||
    !UNIT_FILE_STATES.has(snapshot.unitFileState) ||
    !SHA256_PATTERN.test(snapshot.execStartSha256)
  ) {
    fail("hermes_core_service_snapshot_invalid");
  }
}

function validateLineageRecord(value) {
  return (
    exactKeys(
      value,
      new Set(["current_commit", "previous_commit", "rollback_reserve_commit"])
    ) &&
    COMMIT_PATTERN.test(value.current_commit) &&
    COMMIT_PATTERN.test(value.previous_commit) &&
    COMMIT_PATTERN.test(value.rollback_reserve_commit) &&
    value.current_commit !== value.previous_commit
  );
}

function validateJournal(journal, profiles) {
  const expectedServices = profiles.map((profile) => profile.systemd_user_service);
  if (
    !exactKeys(journal, JOURNAL_KEYS) ||
    journal.schema_version !== 1 ||
    journal.transaction_type !== "hermes-core-release" ||
    !Number.isSafeInteger(journal.revision) ||
    journal.revision < 1 ||
    !COMMIT_PATTERN.test(journal.candidate_commit) ||
    !validateArtifactRecord(journal.candidate_artifact, journal.candidate_commit) ||
    !validateLineageRecord(journal.original_lineage) ||
    !validateLineageRecord(journal.candidate_lineage) ||
    !PHASES.has(journal.phase) ||
    !Array.isArray(journal.services) ||
    journal.services.length !== profiles.length ||
    !Array.isArray(journal.stopped_services) ||
    !Array.isArray(journal.started_candidate_services) ||
    !exactKeys(journal.rollback, new Set(["attempted", "status", "failure_code"])) ||
    typeof journal.rollback.attempted !== "boolean" ||
    !new Set(["not_attempted", "in_progress", "succeeded", "failed"]).has(
      journal.rollback.status
    ) ||
    !(
      journal.rollback.failure_code === null ||
      (typeof journal.rollback.failure_code === "string" &&
        /^[a-z][a-z0-9_]+$/.test(journal.rollback.failure_code))
    )
  ) {
    fail("hermes_core_transaction_journal_invalid");
  }
  for (let index = 0; index < profiles.length; index += 1) {
    const service = journal.services[index];
    const profile = profiles[index];
    if (
      !exactKeys(
        service,
        new Set([
          "profile_id",
          "unit",
          "load_state",
          "active_state",
          "unit_file_state",
          "exec_start_sha256",
        ])
      ) ||
      service.profile_id !== profile.id ||
      service.unit !== profile.systemd_user_service ||
      !LOAD_STATES.has(service.load_state) ||
      !ACTIVE_STATES.has(service.active_state) ||
      !UNIT_FILE_STATES.has(service.unit_file_state) ||
      !SHA256_PATTERN.test(service.exec_start_sha256)
    ) {
      fail("hermes_core_transaction_journal_invalid");
    }
  }
  for (const list of [journal.stopped_services, journal.started_candidate_services]) {
    if (
      new Set(list).size !== list.length ||
      list.some((unit) => !expectedServices.includes(unit))
    ) {
      fail("hermes_core_transaction_journal_invalid");
    }
  }
  const activeServices = journal.services
    .filter((service) => service.active_state === "active")
    .map((service) => service.unit);
  if (
    !isExactPrefix(journal.stopped_services, activeServices) ||
    !isExactPrefix(journal.started_candidate_services, activeServices)
  ) {
    fail("hermes_core_transaction_journal_invalid");
  }
  const forwardRollback =
    journal.rollback.attempted === false &&
    journal.rollback.status === "not_attempted" &&
    journal.rollback.failure_code === null;
  const rollbackInProgress =
    journal.rollback.attempted === true &&
    journal.rollback.status === "in_progress" &&
    typeof journal.rollback.failure_code === "string";
  const rollbackSucceeded =
    journal.rollback.attempted === true &&
    journal.rollback.status === "succeeded" &&
    typeof journal.rollback.failure_code === "string";
  const rollbackFailed =
    journal.rollback.attempted === true &&
    journal.rollback.status === "failed" &&
    typeof journal.rollback.failure_code === "string";
  const stoppedAll = journal.stopped_services.length === activeServices.length;
  const startedAll =
    journal.started_candidate_services.length === activeServices.length;
  const phaseValid =
    (journal.phase === "prepared" &&
      forwardRollback &&
      journal.stopped_services.length === 0 &&
      journal.started_candidate_services.length === 0) ||
    (journal.phase === "services_stopping" &&
      forwardRollback &&
      journal.started_candidate_services.length === 0) ||
    (new Set(["services_stopped", "lineage_committing", "lineage_committed"]).has(
      journal.phase
    ) &&
      forwardRollback &&
      stoppedAll &&
      journal.started_candidate_services.length === 0) ||
    (journal.phase === "services_starting" && forwardRollback && stoppedAll) ||
    (new Set(["smoke_running", "committed"]).has(journal.phase) &&
      forwardRollback &&
      stoppedAll &&
      startedAll) ||
    (new Set([
      "rollback_stopping_candidate",
      "rollback_restoring_lineage",
      "rollback_starting_original",
    ]).has(journal.phase) &&
      rollbackInProgress) ||
    (journal.phase === "rolled_back" && rollbackSucceeded) ||
    (journal.phase === "rollback_failed" && rollbackFailed);
  if (!phaseValid) {
    fail("hermes_core_transaction_journal_invalid");
  }
  return journal;
}

function readJournalFile(filePath, owner, profiles) {
  let metadata;
  let value;
  try {
    metadata = fs.lstatSync(filePath);
    if (
      metadata.isSymbolicLink() ||
      !metadata.isFile() ||
      metadata.nlink !== 1 ||
      !sameOwner(metadata, owner) ||
      modeOf(metadata) !== 0o600 ||
      metadata.size <= 0 ||
      metadata.size > MAX_JOURNAL_BYTES
    ) {
      fail("hermes_core_transaction_journal_invalid");
    }
    value = JSON.parse(fs.readFileSync(filePath, "utf8"));
  } catch (error) {
    if (error instanceof HermesCoreTransactionError) {
      throw error;
    }
    fail("hermes_core_transaction_journal_invalid");
  }
  return validateJournal(value, profiles);
}

function journalIdentityMatches(left, right) {
  return (
    left.candidate_commit === right.candidate_commit &&
    JSON.stringify(left.candidate_artifact) ===
      JSON.stringify(right.candidate_artifact) &&
    JSON.stringify(left.original_lineage) === JSON.stringify(right.original_lineage) &&
    JSON.stringify(left.candidate_lineage) ===
      JSON.stringify(right.candidate_lineage) &&
    JSON.stringify(left.services) === JSON.stringify(right.services)
  );
}

function transitionAllowed(from, to) {
  if (from === to && !new Set(["committed", "rolled_back"]).has(from)) {
    return true;
  }
  const transitions = new Map([
    ["prepared", new Set(["services_stopping", "rollback_stopping_candidate"])],
    ["services_stopping", new Set(["services_stopped", "rollback_stopping_candidate"])],
    [
      "services_stopped",
      new Set(["lineage_committing", "rollback_stopping_candidate"]),
    ],
    [
      "lineage_committing",
      new Set(["lineage_committed", "rollback_stopping_candidate"]),
    ],
    [
      "lineage_committed",
      new Set(["services_starting", "rollback_stopping_candidate"]),
    ],
    ["services_starting", new Set(["smoke_running", "rollback_stopping_candidate"])],
    ["smoke_running", new Set(["committed", "rollback_stopping_candidate"])],
    [
      "rollback_stopping_candidate",
      new Set(["rollback_restoring_lineage", "rollback_failed"]),
    ],
    [
      "rollback_restoring_lineage",
      new Set(["rollback_starting_original", "rollback_failed"]),
    ],
    ["rollback_starting_original", new Set(["rolled_back", "rollback_failed"])],
    ["rollback_failed", new Set(["rollback_stopping_candidate"])],
    ["rolled_back", new Set(["rollback_stopping_candidate"])],
  ]);
  return transitions.get(from)?.has(to) ?? false;
}

function fsyncPath(targetPath) {
  const descriptor = fs.openSync(targetPath, "r");
  try {
    fs.fsyncSync(descriptor);
  } finally {
    fs.closeSync(descriptor);
  }
}

function discardPendingJournal(pendingPath, transactionsRoot, owner, syncPath) {
  if (!fs.existsSync(pendingPath)) {
    return;
  }
  let metadata;
  try {
    metadata = fs.lstatSync(pendingPath);
  } catch {
    fail("hermes_core_transaction_journal_invalid");
  }
  if (
    metadata.isSymbolicLink() ||
    !metadata.isFile() ||
    metadata.nlink !== 1 ||
    !sameOwner(metadata, owner) ||
    modeOf(metadata) !== 0o600
  ) {
    fail("hermes_core_transaction_journal_invalid");
  }
  try {
    fs.unlinkSync(pendingPath);
    syncPath(transactionsRoot);
  } catch {
    fail("hermes_core_transaction_journal_commit_uncertain");
  }
}

function loadJournal({ transactionsRoot, candidateCommit, owner, profiles, syncPath }) {
  const journalPath = path.join(transactionsRoot, `${candidateCommit}.json`);
  const pendingPath = path.join(transactionsRoot, `.${candidateCommit}.pending`);
  const finalExists = fs.existsSync(journalPath);
  const pendingExists = fs.existsSync(pendingPath);
  if (!finalExists && !pendingExists) {
    return { journal: null, journalPath, pendingPath };
  }
  const current = finalExists ? readJournalFile(journalPath, owner, profiles) : null;
  let pending = null;
  if (pendingExists) {
    try {
      pending = readJournalFile(pendingPath, owner, profiles);
    } catch (error) {
      if (!current) {
        throw error;
      }
      discardPendingJournal(pendingPath, transactionsRoot, owner, syncPath);
      try {
        syncPath(journalPath);
        syncPath(transactionsRoot);
      } catch {
        fail("hermes_core_transaction_journal_commit_uncertain");
      }
      return { journal: current, journalPath, pendingPath };
    }
  }
  if (pending) {
    if (
      current &&
      (!journalIdentityMatches(current, pending) ||
        !(
          pending.revision === current.revision + 1 ||
          (pending.revision === current.revision &&
            JSON.stringify(pending) === JSON.stringify(current))
        ))
    ) {
      fail("hermes_core_transaction_journal_invalid");
    }
    if (!current || pending.revision > current.revision) {
      fs.renameSync(pendingPath, journalPath);
    } else {
      fs.unlinkSync(pendingPath);
    }
    try {
      syncPath(transactionsRoot);
    } catch {
      fail("hermes_core_transaction_journal_commit_uncertain");
    }
    return { journal: pending, journalPath, pendingPath };
  }
  try {
    syncPath(journalPath);
    syncPath(transactionsRoot);
  } catch {
    fail("hermes_core_transaction_journal_commit_uncertain");
  }
  return { journal: current, journalPath, pendingPath };
}

function writeJournal({
  journal,
  journalPath,
  pendingPath,
  transactionsRoot,
  owner,
  syncPath,
}) {
  if (fs.existsSync(pendingPath)) {
    fail("hermes_core_transaction_journal_invalid");
  }
  const flags =
    fs.constants.O_WRONLY |
    fs.constants.O_CREAT |
    fs.constants.O_EXCL |
    (fs.constants.O_NOFOLLOW ?? 0);
  let descriptor;
  let writeFailed = false;
  try {
    descriptor = fs.openSync(pendingPath, flags, 0o600);
    fs.writeFileSync(descriptor, `${JSON.stringify(journal, null, 2)}\n`, "utf8");
    fs.fchownSync(descriptor, owner.uid, owner.gid);
    fs.fchmodSync(descriptor, 0o600);
    fs.fsyncSync(descriptor);
  } catch {
    writeFailed = true;
  } finally {
    if (descriptor !== undefined) {
      fs.closeSync(descriptor);
    }
  }
  if (writeFailed) {
    discardPendingJournal(pendingPath, transactionsRoot, owner, syncPath);
    fail("hermes_core_transaction_journal_write_failed");
  }
  let renamed = false;
  try {
    fs.renameSync(pendingPath, journalPath);
    renamed = true;
    syncPath(transactionsRoot);
  } catch {
    if (renamed) {
      fail("hermes_core_transaction_journal_commit_uncertain");
    }
    fail("hermes_core_transaction_journal_write_failed");
  }
}

function updateJournal(context, changes, checkpoint) {
  const loaded = loadJournal({
    transactionsRoot: context.transactionsRoot,
    candidateCommit: context.journal.candidate_commit,
    owner: context.owner,
    profiles: context.profiles,
    syncPath: context.syncPath,
  });
  if (!loaded.journal || !journalIdentityMatches(context.journal, loaded.journal)) {
    fail("hermes_core_transaction_journal_invalid");
  }
  const nextJournal = {
    ...loaded.journal,
    ...changes,
    revision: loaded.journal.revision + 1,
  };
  if (!transitionAllowed(loaded.journal.phase, nextJournal.phase)) {
    fail("hermes_core_transaction_journal_transition_invalid");
  }
  validateJournal(nextJournal, context.profiles);
  writeJournal({
    journal: nextJournal,
    journalPath: context.journalPath,
    pendingPath: context.pendingPath,
    transactionsRoot: context.transactionsRoot,
    owner: context.owner,
    syncPath: context.syncPath,
  });
  context.journal = nextJournal;
  context.afterCheckpoint?.(checkpoint, structuredClone(context.journal));
}

function assertJournalIdentity(journal, expected) {
  if (
    journal.candidate_commit !== expected.commit ||
    JSON.stringify(journal.candidate_artifact) !==
      JSON.stringify(artifactRecord(expected)) ||
    JSON.stringify(journal.original_lineage) !==
      JSON.stringify(lineageRecord(originalLineage(expected))) ||
    JSON.stringify(journal.candidate_lineage) !==
      JSON.stringify(lineageRecord(candidateLineage(expected)))
  ) {
    fail("hermes_core_transaction_journal_identity_mismatch");
  }
}

function assertServiceState(serviceController, profile, expected, activeState) {
  const observed = serviceController.snapshot(profile);
  validateSnapshot(observed);
  if (
    observed.loadState !== expected.load_state ||
    observed.activeState !== activeState ||
    observed.unitFileState !== expected.unit_file_state ||
    observed.execStartSha256 !== expected.exec_start_sha256
  ) {
    fail("hermes_core_service_state_mismatch");
  }
}

function detectLiveLineage(context) {
  try {
    context.lineageController.verify(
      lineageFromRecord(context.journal.candidate_lineage)
    );
    return "candidate";
  } catch {
    try {
      context.lineageController.verify(
        lineageFromRecord(context.journal.original_lineage)
      );
      return "original";
    } catch {
      return "unknown";
    }
  }
}

function rollbackTransaction(context, causeCode) {
  const { profiles, serviceController, lineageController } = context;
  const record = (changes, checkpoint) => {
    try {
      updateJournal(context, changes, checkpoint);
    } catch (error) {
      if (error instanceof HermesCoreTransactionCrash) {
        throw error;
      }
      fail("hermes_core_transaction_rollback_journal_uncertain", {
        rollbackStatus: "failed",
        causeCode,
      });
    }
  };
  const markFailed = (failureCode) => {
    try {
      updateJournal(
        context,
        {
          phase: "rollback_failed",
          rollback: {
            attempted: true,
            status: "failed",
            failure_code: failureCode,
          },
        },
        "rollback_failed"
      );
    } catch (error) {
      if (error instanceof HermesCoreTransactionCrash) {
        throw error;
      }
    }
    fail("hermes_core_transaction_rollback_failed", {
      rollbackStatus: "failed",
      causeCode,
    });
  };

  const liveLineage = detectLiveLineage(context);
  const persistedRollbackPhase = new Set([
    "rollback_stopping_candidate",
    "rollback_restoring_lineage",
    "rollback_starting_original",
  ]).has(context.journal.phase)
    ? context.journal.phase
    : null;
  if (persistedRollbackPhase === null) {
    record(
      {
        phase: "rollback_stopping_candidate",
        rollback: { attempted: true, status: "in_progress", failure_code: causeCode },
      },
      "rollback_stopping_candidate"
    );
  } else if (
    context.journal.rollback.failure_code !== causeCode ||
    context.journal.rollback.status !== "in_progress"
  ) {
    record(
      {
        rollback: { attempted: true, status: "in_progress", failure_code: causeCode },
      },
      persistedRollbackPhase
    );
  }
  if (liveLineage === "unknown") {
    markFailed("hermes_core_lineage_state_unknown");
  }

  if (
    liveLineage === "candidate" &&
    persistedRollbackPhase !== "rollback_starting_original"
  ) {
    const stopErrors = [];
    for (const profile of [...profiles].reverse()) {
      try {
        const observed = serviceController.snapshot(profile);
        validateSnapshot(observed);
        if (observed.activeState === "active") {
          serviceController.stop(profile);
        }
        const stopped = serviceController.snapshot(profile);
        validateSnapshot(stopped);
        if (stopped.activeState !== "inactive") {
          throw new Error("hermes_core_candidate_service_stop_failed");
        }
      } catch (error) {
        stopErrors.push(
          normalizeFailure(error, "hermes_core_candidate_service_stop_failed")
        );
      }
    }
    if (stopErrors.length > 0) {
      markFailed(stopErrors[0]);
    }
  }

  if (persistedRollbackPhase !== "rollback_starting_original") {
    record({ phase: "rollback_restoring_lineage" }, "rollback_restoring_lineage");
  }
  if (liveLineage === "candidate") {
    try {
      lineageController.restore(
        lineageFromRecord(context.journal.candidate_lineage),
        lineageFromRecord(context.journal.original_lineage)
      );
    } catch (error) {
      markFailed(normalizeFailure(error, "hermes_core_lineage_restore_failed"));
    }
  }
  try {
    lineageController.verify(lineageFromRecord(context.journal.original_lineage));
  } catch (error) {
    markFailed(normalizeFailure(error, "hermes_core_original_lineage_invalid"));
  }
  try {
    serviceController.daemonReload();
  } catch (error) {
    markFailed(normalizeFailure(error, "hermes_core_daemon_reload_failed"));
  }

  if (persistedRollbackPhase !== "rollback_starting_original") {
    record({ phase: "rollback_starting_original" }, "rollback_starting_original");
  }
  const restoreErrors = [];
  for (let index = 0; index < profiles.length; index += 1) {
    const profile = profiles[index];
    const expectedService = context.journal.services[index];
    try {
      let observed = serviceController.snapshot(profile);
      validateSnapshot(observed);
      if (
        expectedService.active_state === "inactive" &&
        observed.activeState === "active"
      ) {
        serviceController.stop(profile);
      }
      serviceController.restoreUnitFileState(profile, expectedService.unit_file_state);
      observed = serviceController.snapshot(profile);
      validateSnapshot(observed);
      if (
        expectedService.active_state === "active" &&
        observed.activeState !== "active"
      ) {
        serviceController.start(profile);
      }
      assertServiceState(
        serviceController,
        profile,
        expectedService,
        expectedService.active_state
      );
      if (expectedService.active_state === "active") {
        serviceController.smoke(
          profile,
          context.journal.original_lineage.current_commit
        );
      }
    } catch (error) {
      restoreErrors.push(
        normalizeFailure(error, "hermes_core_original_service_restore_failed")
      );
    }
  }
  if (restoreErrors.length > 0) {
    markFailed(restoreErrors[0]);
  }

  try {
    updateJournal(
      context,
      {
        phase: "rolled_back",
        rollback: { attempted: true, status: "succeeded", failure_code: causeCode },
      },
      "rolled_back"
    );
  } catch (error) {
    if (error instanceof HermesCoreTransactionCrash) {
      throw error;
    }
    fail("hermes_core_transaction_rollback_journal_uncertain", {
      rollbackStatus: "succeeded",
      causeCode,
    });
  }
  fail("hermes_core_transaction_rolled_back", {
    rollbackStatus: "succeeded",
    causeCode,
  });
}

function executeHermesCoreReleaseTransactionInternal({
  expected,
  coreRoot,
  trustedParent,
  owner,
  lockHeld,
  lockFd,
  verifyInheritedLock = true,
  serviceController,
  lineageController,
  syncPath = fsyncPath,
  afterCheckpoint,
}) {
  validateExpected(expected);
  if (
    !serviceController ||
    !lineageController ||
    ["snapshot", "stop", "start", "restoreUnitFileState", "daemonReload", "smoke"].some(
      (method) => typeof serviceController[method] !== "function"
    ) ||
    ["prepare", "commit", "restore", "verify", "verifyCandidate"].some(
      (method) => typeof lineageController[method] !== "function"
    )
  ) {
    fail("hermes_core_transaction_dependency_invalid");
  }
  let resolvedRoot;
  let profiles;
  try {
    resolvedRoot = verifyHermesCoreManagerLockBoundary({
      coreRoot,
      trustedParent,
      owner,
      lockHeld,
      lockFd,
      verifyInheritedLock,
    });
    profiles = loadHermesProfileRegistry();
  } catch (error) {
    fail(normalizeFailure(error, "hermes_core_transaction_preflight_failed"));
  }
  const transactionsRoot = path.join(resolvedRoot, "state", "transactions");
  const transactionMetadata = fs.lstatSync(transactionsRoot);
  if (
    transactionMetadata.isSymbolicLink() ||
    !transactionMetadata.isDirectory() ||
    !sameOwner(transactionMetadata, owner) ||
    modeOf(transactionMetadata) !== 0o700
  ) {
    fail("hermes_core_transaction_directory_invalid");
  }

  const loaded = loadJournal({
    transactionsRoot,
    candidateCommit: expected.commit,
    owner,
    profiles,
    syncPath,
  });
  const context = {
    ...loaded,
    transactionsRoot,
    owner,
    profiles,
    serviceController,
    lineageController,
    syncPath,
    afterCheckpoint,
  };
  if (context.journal) {
    assertJournalIdentity(context.journal, expected);
    if (context.journal.phase === "committed") {
      try {
        lineageController.verify(lineageFromRecord(context.journal.candidate_lineage));
        lineageController.verifyCandidate(expected);
        for (let index = 0; index < profiles.length; index += 1) {
          assertServiceState(
            serviceController,
            profiles[index],
            context.journal.services[index],
            context.journal.services[index].active_state
          );
          if (context.journal.services[index].active_state === "active") {
            serviceController.smoke(profiles[index], expected.commit);
          }
        }
      } catch {
        fail("hermes_core_transaction_terminal_state_invalid");
      }
      return {
        status: "committed",
        alreadyCommitted: true,
        recovered: true,
        candidateCommit: expected.commit,
        serviceChanges: 0,
        pointerChanges: 0,
      };
    }
    if (context.journal.phase === "rolled_back") {
      let rollbackStateValid = true;
      try {
        lineageController.verify(lineageFromRecord(context.journal.original_lineage));
        for (let index = 0; index < profiles.length; index += 1) {
          assertServiceState(
            serviceController,
            profiles[index],
            context.journal.services[index],
            context.journal.services[index].active_state
          );
          if (context.journal.services[index].active_state === "active") {
            serviceController.smoke(
              profiles[index],
              context.journal.original_lineage.current_commit
            );
          }
        }
      } catch {
        rollbackStateValid = false;
      }
      if (rollbackStateValid) {
        fail("hermes_core_transaction_previously_rolled_back", {
          rollbackStatus: "succeeded",
          causeCode: context.journal.rollback.failure_code,
        });
      }
      rollbackTransaction(context, "hermes_core_transaction_recovery_required");
    }
    rollbackTransaction(context, "hermes_core_transaction_recovery_required");
  }

  try {
    lineageController.verify(originalLineage(expected));
    lineageController.verifyCandidate(expected);
    lineageController.prepare(expected);
  } catch (error) {
    fail(normalizeFailure(error, "hermes_core_transaction_preflight_failed"));
  }
  const services = profiles.map((profile) => {
    const snapshot = serviceController.snapshot(profile);
    validateSnapshot(snapshot);
    return {
      profile_id: profile.id,
      unit: profile.systemd_user_service,
      load_state: snapshot.loadState,
      active_state: snapshot.activeState,
      unit_file_state: snapshot.unitFileState,
      exec_start_sha256: snapshot.execStartSha256,
    };
  });
  context.journal = {
    schema_version: 1,
    transaction_type: "hermes-core-release",
    revision: 1,
    candidate_commit: expected.commit,
    candidate_artifact: artifactRecord(expected),
    original_lineage: lineageRecord(originalLineage(expected)),
    candidate_lineage: lineageRecord(candidateLineage(expected)),
    phase: "prepared",
    services,
    stopped_services: [],
    started_candidate_services: [],
    rollback: { attempted: false, status: "not_attempted", failure_code: null },
  };
  validateJournal(context.journal, profiles);
  writeJournal({
    journal: context.journal,
    journalPath: context.journalPath,
    pendingPath: context.pendingPath,
    transactionsRoot,
    owner,
    syncPath,
  });
  afterCheckpoint?.("prepared", structuredClone(context.journal));

  try {
    updateJournal(context, { phase: "services_stopping" }, "services_stopping");
    for (let index = 0; index < profiles.length; index += 1) {
      const profile = profiles[index];
      const expectedService = services[index];
      if (expectedService.active_state === "active") {
        serviceController.stop(profile);
        assertServiceState(serviceController, profile, expectedService, "inactive");
        updateJournal(
          context,
          {
            stopped_services: [
              ...context.journal.stopped_services,
              profile.systemd_user_service,
            ],
          },
          `service_stopped:${profile.id}`
        );
      } else {
        assertServiceState(serviceController, profile, expectedService, "inactive");
      }
    }
    updateJournal(context, { phase: "services_stopped" }, "services_stopped");
    updateJournal(context, { phase: "lineage_committing" }, "lineage_committing");
    lineageController.commit(expected);
    updateJournal(context, { phase: "lineage_committed" }, "lineage_committed");
    serviceController.daemonReload();
    updateJournal(context, { phase: "services_starting" }, "services_starting");
    for (let index = 0; index < profiles.length; index += 1) {
      const profile = profiles[index];
      if (services[index].active_state === "active") {
        serviceController.start(profile);
        assertServiceState(serviceController, profile, services[index], "active");
        updateJournal(
          context,
          {
            started_candidate_services: [
              ...context.journal.started_candidate_services,
              profile.systemd_user_service,
            ],
          },
          `candidate_service_started:${profile.id}`
        );
      } else {
        assertServiceState(serviceController, profile, services[index], "inactive");
      }
    }
    updateJournal(context, { phase: "smoke_running" }, "smoke_running");
    for (let index = 0; index < profiles.length; index += 1) {
      if (services[index].active_state === "active") {
        serviceController.smoke(profiles[index], expected.commit);
      }
    }
    lineageController.verify(candidateLineage(expected));
    for (let index = 0; index < profiles.length; index += 1) {
      assertServiceState(
        serviceController,
        profiles[index],
        services[index],
        services[index].active_state
      );
    }
    updateJournal(context, { phase: "committed" }, "committed");
    return {
      status: "committed",
      alreadyCommitted: false,
      recovered: false,
      candidateCommit: expected.commit,
      serviceChanges:
        services.filter((service) => service.active_state === "active").length * 2,
      pointerChanges: 1,
    };
  } catch (error) {
    if (error instanceof HermesCoreTransactionCrash) {
      throw error;
    }
    rollbackTransaction(
      context,
      normalizeFailure(error, "hermes_core_transaction_step_failed")
    );
  }
}

export function executeHermesCoreReleaseTransactionForTest(options) {
  const lineageOptions = {
    coreRoot: options.coreRoot,
    trustedParent: options.trustedParent,
    owner: options.owner,
    lockHeld: options.lockHeld,
    lockFd: options.lockFd,
    verifyInheritedLock: options.verifyInheritedLock,
    syncPath: options.syncPath,
  };
  const lineageController =
    options.lineageController ??
    Object.freeze({
      prepare: options.prepare,
      commit: (expected) =>
        commitHermesCoreLineageForTest({ ...lineageOptions, expected }),
      restore: (fromLineage, toLineage) =>
        restoreHermesCoreLineageForTest({
          ...lineageOptions,
          fromLineage,
          toLineage,
        }),
      verify: options.verify,
      verifyCandidate: options.verifyCandidate,
    });
  return executeHermesCoreReleaseTransactionInternal({
    ...options,
    lineageController,
  });
}

// The production entry point requires real service and lineage controllers.  The
// test-only entry point above is the only place that can synthesize test lineage
// primitives from fixture options.
export function executeHermesCoreReleaseTransaction(options) {
  if (
    !options ||
    options.useTestPrimitives === true ||
    typeof options.serviceController?.snapshot !== "function" ||
    typeof options.lineageController?.commit !== "function"
  ) {
    fail("hermes_core_transaction_dependency_invalid");
  }
  return executeHermesCoreReleaseTransactionInternal(options);
}

export function isHermesCoreTransactionTerminalPhase(phase) {
  return TERMINAL_PHASES.has(phase);
}
