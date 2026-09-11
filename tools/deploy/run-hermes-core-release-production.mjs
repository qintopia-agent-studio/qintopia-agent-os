#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import {
  commitHermesCoreLineage,
  commitHermesCoreLineageForTest,
  restoreHermesCoreLineage,
  restoreHermesCoreLineageForTest,
} from "./commit-hermes-core-lineage.mjs";
import {
  computeHermesCoreLineageFingerprint,
  planHermesCoreRelease,
  verifyHermesCoreLineage,
} from "./plan-hermes-core-release.mjs";
import { stageHermesCoreRelease } from "./stage-hermes-core-release.mjs";
import {
  executeHermesCoreReleaseTransaction,
  HermesCoreTransactionError,
} from "./run-hermes-core-release-transaction.mjs";

const CORE_ROOT = "/var/lib/qintopia-hermes-core";
const STATE_ROOT = "/var/lib/qintopia-agent-os-deploy";
const TRUSTED_PARENT = "/var/lib";
const REPOSITORY = "https://github.com/NousResearch/hermes-agent.git";
const OWNER = { uid: 0, gid: 0 };
const SHA256 = /^[0-9a-f]{64}$/;
const COMMIT = /^[0-9a-f]{40}$/;
const TAG = /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/;
const VERSION = /^[A-Za-z0-9][A-Za-z0-9._+-]{0,127}$/;
const REQUEST_ID = /^deploy-[0-9]{8}T[0-9]{6}Z-[0-9a-f]{7,40}$/;
const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const RUNNER_DIR = path.resolve(SCRIPT_DIR, "../../deploy/runner");
const RUNUSER = "/usr/sbin/runuser";
const SYSTEMCTL = "/usr/bin/systemctl";
const ENV = "/usr/bin/env";
const HERMES_USER = "ubuntu";
const HERMES_UID = "1000";
const PROFILES = Object.freeze([
  ["default", "hermes-gateway.service"],
  ["erhua", "hermes-gateway-erhua.service"],
  ["guanerye", "hermes-gateway-guanerye.service"],
  ["huabaosi", "hermes-gateway-huabaosi.service"],
  ["silaoshi", "hermes-gateway-silaoshi.service"],
  ["wenyuange", "hermes-gateway-wenyuange.service"],
  ["xiaoman", "hermes-gateway-xiaoman.service"],
]);

class ProductionReleaseError extends Error {
  constructor(code, options = {}) {
    super(code);
    this.name = "ProductionReleaseError";
    this.rollbackStatus = options.rollbackStatus ?? "not_attempted";
  }
}

const fail = (code, options) => {
  throw new ProductionReleaseError(code, options);
};

function readJsonFile(filePath, code, owner = null, expectedMode = null) {
  try {
    const metadata = fs.lstatSync(filePath);
    if (
      metadata.isSymbolicLink() ||
      !metadata.isFile() ||
      metadata.nlink !== 1 ||
      (owner && (metadata.uid !== owner.uid || metadata.gid !== owner.gid)) ||
      (expectedMode && (metadata.mode & 0o7777) !== expectedMode)
    )
      fail(code);
    return JSON.parse(fs.readFileSync(filePath, "utf8"));
  } catch (error) {
    if (error instanceof ProductionReleaseError) throw error;
    fail(code);
  }
}

function readRequest(requestPath, owner = OWNER) {
  const metadata = fs.lstatSync(requestPath);
  if (
    metadata.isSymbolicLink() ||
    !metadata.isFile() ||
    metadata.nlink !== 1 ||
    metadata.uid !== owner.uid ||
    metadata.gid !== owner.gid ||
    (metadata.mode & 0o7777) !== 0o600
  )
    fail("request_invalid");
  return readJsonFile(requestPath, "request_invalid", owner, 0o600);
}

function validateRequest(request) {
  const core = request?.hermes_core_release;
  const coreKeys = [
    "archive_sha256",
    "artifact_identity_sha256",
    "artifact_manifest_sha256",
    "commit_sha",
    "previous_commit_sha",
    "previous_version",
    "repository",
    "source_archive_sha256",
    "tag",
  ];
  if (
    !REQUEST_ID.test(request?.request_id) ||
    request?.environment !== "production" ||
    request?.repository !== "qintopia-agent-studio/qintopia-agent-os" ||
    JSON.stringify(request?.release_scope) !==
      JSON.stringify(["hermes-core-release"]) ||
    JSON.stringify(request?.restart_targets) !== JSON.stringify(["hermes-core"]) ||
    request?.rollback_on_smoke_failure !== true ||
    request?.dry_run !== false ||
    !core ||
    JSON.stringify(Object.keys(core).sort()) !== JSON.stringify(coreKeys.sort())
  )
    fail("request_invalid");
  if (
    core.repository !== REPOSITORY ||
    !TAG.test(core.tag) ||
    !COMMIT.test(core.commit_sha) ||
    !COMMIT.test(core.previous_commit_sha) ||
    !VERSION.test(core.previous_version) ||
    core.commit_sha === core.previous_commit_sha ||
    ![
      "source_archive_sha256",
      "artifact_identity_sha256",
      "artifact_manifest_sha256",
      "archive_sha256",
    ].every((key) => SHA256.test(core[key]))
  )
    fail("request_invalid");
  return core;
}

function artifactExpected(core) {
  return {
    tag: core.tag,
    commit: core.commit_sha,
    sourceSha256: core.source_archive_sha256,
    identitySha256: core.artifact_identity_sha256,
    manifestSha256: core.artifact_manifest_sha256,
  };
}

function readOwnedDirectory(directoryPath, code, owner, expectedMode) {
  let metadata;
  try {
    metadata = fs.lstatSync(directoryPath);
  } catch {
    fail(code);
  }
  if (
    metadata.isSymbolicLink() ||
    !metadata.isDirectory() ||
    metadata.uid !== owner.uid ||
    metadata.gid !== owner.gid ||
    (metadata.mode & 0o7777) !== expectedMode
  )
    fail(code);
}

function readActiveLineage(coreRoot, owner) {
  const lineageRoot = path.join(coreRoot, "lineage");
  const active = path.join(lineageRoot, "active");
  let target;
  try {
    const metadata = fs.lstatSync(active);
    if (
      !metadata.isSymbolicLink() ||
      metadata.uid !== owner.uid ||
      metadata.gid !== owner.gid
    )
      fail("lineage_invalid");
    target = fs.readlinkSync(active);
  } catch (error) {
    if (error instanceof ProductionReleaseError) throw error;
    fail("lineage_invalid");
  }
  if (!/^generations\/generation-[0-9a-f]{64}$/.test(target)) fail("lineage_invalid");
  const generationPath = path.join(lineageRoot, target);
  readOwnedDirectory(generationPath, "lineage_invalid", owner, 0o555);
  const lineage = readJsonFile(
    path.join(generationPath, "lineage.json"),
    "lineage_invalid",
    owner,
    0o444
  );
  if (
    computeHermesCoreLineageFingerprint({
      currentCommit: lineage.current_commit,
      previousCommit: lineage.previous_commit,
      rollbackReserveCommit: lineage.rollback_reserve_commit,
    }) !== target.slice("generations/generation-".length) ||
    !COMMIT.test(lineage.current_commit) ||
    !COMMIT.test(lineage.previous_commit) ||
    !COMMIT.test(lineage.rollback_reserve_commit) ||
    lineage.current_commit === lineage.previous_commit
  )
    fail("lineage_invalid");
  return {
    currentCommit: lineage.current_commit,
    previousCommit: lineage.previous_commit,
    rollbackReserveCommit: lineage.rollback_reserve_commit,
  };
}

function readReleaseManifest(coreRoot, commit, owner) {
  const releasePath = path.join(coreRoot, "releases", commit);
  readOwnedDirectory(releasePath, "release_manifest_invalid", owner, 0o555);
  return readJsonFile(
    path.join(releasePath, "artifact-manifest.json"),
    "release_manifest_invalid",
    owner,
    0o444
  );
}

function systemctlUser(args) {
  try {
    return execFileSync(
      RUNUSER,
      [
        "-u",
        HERMES_USER,
        "--",
        ENV,
        `XDG_RUNTIME_DIR=/run/user/${HERMES_UID}`,
        SYSTEMCTL,
        "--user",
        ...args,
      ],
      { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }
    ).trim();
  } catch {
    fail("hermes_systemd_command_failed");
  }
}

function createServiceController() {
  const serviceFor = (profile) => {
    const entry = PROFILES.find(([id]) => id === profile.id);
    if (!entry) fail("profile_registry_invalid");
    return entry[1];
  };
  const show = (unit, property) =>
    systemctlUser(["show", unit, `--property=${property}`, "--value"]);
  return {
    snapshot(profile) {
      const unit = serviceFor(profile);
      const execStart = show(unit, "ExecStart");
      return {
        loadState: show(unit, "LoadState"),
        activeState: show(unit, "ActiveState"),
        unitFileState: show(unit, "UnitFileState"),
        execStartSha256: crypto
          .createHash("sha256")
          .update(execStart, "utf8")
          .digest("hex"),
      };
    },
    stop(profile) {
      systemctlUser(["stop", serviceFor(profile)]);
    },
    start(profile) {
      systemctlUser(["start", serviceFor(profile)]);
    },
    restoreUnitFileState(profile, state) {
      if (state === "enabled") systemctlUser(["enable", serviceFor(profile)]);
      else if (state === "disabled") systemctlUser(["disable", serviceFor(profile)]);
      else fail("hermes_unit_file_state_invalid");
    },
    daemonReload() {
      systemctlUser(["daemon-reload"]);
    },
    smoke(profile, commit) {
      const unit = serviceFor(profile);
      const active = show(unit, "ActiveState");
      const pid = Number(show(unit, "MainPID"));
      if (active !== "active" || !Number.isSafeInteger(pid) || pid <= 0)
        fail("hermes_profile_smoke_failed");
      let environment;
      try {
        environment = fs.readFileSync(`/proc/${pid}/environ`, "utf8").split("\0");
      } catch {
        fail("hermes_profile_smoke_failed");
      }
      if (!environment.includes(`QINTOPIA_HERMES_CORE_COMMIT=${commit}`))
        fail("hermes_runtime_identity_mismatch");
    },
  };
}

function createLineageController(expected, options = {}) {
  const coreRoot = options.coreRoot ?? CORE_ROOT;
  const trustedParent = options.trustedParent ?? TRUSTED_PARENT;
  const owner = options.owner ?? OWNER;
  const lockFd = options.lockFd ?? 9;
  const useTestPrimitives = options.useTestPrimitives === true;
  const lineageOnly = (value) => ({
    currentCommit: value.currentCommit,
    previousCommit: value.previousCommit,
    rollbackReserveCommit: value.rollbackReserveCommit,
  });
  const verify = (value) =>
    verifyHermesCoreLineage({
      expectedLineage: lineageOnly(value),
      coreRoot,
      trustedParent,
      owner,
      minimumFreeBytes: 0,
      lockHeld: true,
      lockFd,
      verifyInheritedLock: true,
    });
  const plan = () =>
    planHermesCoreRelease({
      expected,
      coreRoot,
      trustedParent,
      owner,
      minimumFreeBytes: 0,
      lockHeld: true,
      lockFd,
      verifyInheritedLock: true,
    });
  return {
    prepare: plan,
    verifyCandidate: plan,
    verify,
    commit: useTestPrimitives
      ? () =>
          commitHermesCoreLineageForTest({
            expected,
            coreRoot,
            trustedParent,
            owner,
            lockHeld: true,
            lockFd,
            verifyInheritedLock: true,
          })
      : () => commitHermesCoreLineage({ expected }),
    restore: (fromLineage, toLineage) =>
      useTestPrimitives
        ? restoreHermesCoreLineageForTest({
            fromLineage: lineageOnly(fromLineage),
            toLineage: lineageOnly(toLineage),
            coreRoot,
            trustedParent,
            owner,
            lockHeld: true,
            lockFd,
            verifyInheritedLock: true,
          })
        : restoreHermesCoreLineage({
            fromLineage: lineageOnly(fromLineage),
            toLineage: lineageOnly(toLineage),
          }),
  };
}

function writeEvidence(outputPath, evidence, stateRoot = STATE_ROOT, owner = OWNER) {
  const expectedRoot = path.join(stateRoot, "results");
  fs.mkdirSync(expectedRoot, { recursive: true, mode: 0o700 });
  readOwnedDirectory(expectedRoot, "evidence_directory_invalid", owner, 0o700);
  if (outputPath !== path.join(expectedRoot, `${evidence.request_id}.hermes-core.json`))
    fail("evidence_path_invalid");
  const temporary = `${outputPath}.pending.${process.pid}`;
  const descriptor = fs.openSync(
    temporary,
    fs.constants.O_WRONLY | fs.constants.O_CREAT | fs.constants.O_EXCL,
    0o600
  );
  try {
    fs.writeFileSync(descriptor, `${JSON.stringify(evidence, null, 2)}\n`, "utf8");
    fs.fsyncSync(descriptor);
  } finally {
    fs.closeSync(descriptor);
  }
  fs.renameSync(temporary, outputPath);
  const directory = fs.openSync(
    expectedRoot,
    fs.constants.O_RDONLY | fs.constants.O_DIRECTORY
  );
  try {
    fs.fsyncSync(directory);
  } finally {
    fs.closeSync(directory);
  }
}

function run(requestPath, evidenceOutput, options = {}) {
  const coreRoot = options.coreRoot ?? CORE_ROOT;
  const stateRoot = options.stateRoot ?? STATE_ROOT;
  const trustedParent = options.trustedParent ?? TRUSTED_PARENT;
  const owner = options.owner ?? OWNER;
  const lockFd = options.lockFd ?? 9;
  const request = readRequest(requestPath, owner);
  const core = validateRequest(request);
  const original = readActiveLineage(coreRoot, owner);
  const expected = {
    ...artifactExpected(core),
    currentCommit: original.currentCommit,
    previousCommit: original.previousCommit,
    rollbackReserveCommit: original.rollbackReserveCommit,
  };
  if (original.currentCommit !== core.previous_commit_sha)
    fail("previous_commit_mismatch");
  const currentManifest = readReleaseManifest(coreRoot, original.currentCommit, owner);
  if (
    currentManifest.commit_sha !== original.currentCommit ||
    currentManifest.hermes_cli_version !== core.previous_version
  )
    fail("previous_version_mismatch");

  const fetchArgs = [
    "--expected-archive-sha256",
    core.archive_sha256,
    "--expected-tag",
    core.tag,
    "--expected-commit",
    core.commit_sha,
    "--expected-source-sha256",
    core.source_archive_sha256,
    "--expected-identity-sha256",
    core.artifact_identity_sha256,
    "--expected-manifest-sha256",
    core.artifact_manifest_sha256,
  ];
  try {
    if (options.fetchArtifact) options.fetchArtifact(fetchArgs);
    else
      execFileSync(
        "/bin/bash",
        [path.join(RUNNER_DIR, "fetch-hermes-core-artifact.sh"), ...fetchArgs],
        { stdio: "ignore" }
      );
  } catch {
    fail("hermes_core_artifact_fetch_failed");
  }
  try {
    if (options.stageArtifact) options.stageArtifact(expected);
    else
      stageHermesCoreRelease({
        expected,
        coreRoot,
        trustedParent,
        owner,
        lockHeld: true,
        lockFd,
        verifyInheritedLock: true,
      });
  } catch {
    fail("hermes_core_candidate_staging_failed");
  }
  try {
    if (options.installUnits) options.installUnits();
    else
      execFileSync(
        "/bin/bash",
        [path.join(RUNNER_DIR, "install-hermes-core-systemd-units.sh")],
        { stdio: "ignore" }
      );
  } catch {
    fail("hermes_core_systemd_install_failed");
  }

  try {
    const transaction = executeHermesCoreReleaseTransaction({
      expected,
      coreRoot,
      trustedParent,
      owner,
      lockHeld: true,
      lockFd,
      verifyInheritedLock: true,
      serviceController: options.serviceController ?? createServiceController(),
      lineageController:
        options.lineageController ??
        createLineageController(expected, {
          coreRoot,
          trustedParent,
          owner,
          useTestPrimitives: options.useTestPrimitives === true,
        }),
    });
    const candidateManifest = readReleaseManifest(coreRoot, core.commit_sha, owner);
    writeEvidence(
      evidenceOutput,
      {
        schema_version: 1,
        request_id: request.request_id,
        tag: core.tag,
        commit_sha: core.commit_sha,
        source_archive_sha256: core.source_archive_sha256,
        artifact_identity_sha256: core.artifact_identity_sha256,
        artifact_manifest_sha256: core.artifact_manifest_sha256,
        previous_commit_sha: core.previous_commit_sha,
        previous_version: core.previous_version,
        new_version: candidateManifest.hermes_cli_version,
        transaction_status: transaction.alreadyCommitted
          ? "already_committed"
          : "committed",
        pointer_changes: transaction.pointerChanges,
        service_changes: transaction.serviceChanges,
      },
      stateRoot,
      owner
    );
    return transaction;
  } catch (error) {
    const rolledBack =
      error instanceof HermesCoreTransactionError &&
      error.rollbackStatus === "succeeded";
    const errorCode = error?.causeCode ?? error?.message;
    writeEvidence(
      evidenceOutput,
      {
        schema_version: 1,
        request_id: request.request_id,
        tag: core.tag,
        commit_sha: core.commit_sha,
        source_archive_sha256: core.source_archive_sha256,
        artifact_identity_sha256: core.artifact_identity_sha256,
        artifact_manifest_sha256: core.artifact_manifest_sha256,
        previous_commit_sha: core.previous_commit_sha,
        previous_version: core.previous_version,
        new_version: null,
        transaction_status: rolledBack ? "rolled_back" : "failed",
        pointer_changes: 0,
        service_changes: 0,
        error: /^[a-z][a-z0-9_]+$/.test(errorCode || "")
          ? errorCode
          : "hermes_core_transaction_failed",
      },
      stateRoot,
      owner
    );
    throw error;
  }
}

export function runHermesCoreReleaseProductionForTest(
  requestPath,
  evidenceOutput,
  options
) {
  return run(requestPath, evidenceOutput, { ...options, useTestPrimitives: true });
}

function main(argv) {
  if (
    argv.length !== 4 ||
    argv[0] !== "--request-file" ||
    argv[2] !== "--evidence-output"
  ) {
    console.error("hermes_core_production=blocked");
    console.error("hermes_core_production_error=invalid_invocation");
    return 2;
  }
  try {
    const result = run(argv[1], argv[3]);
    console.log("hermes_core_production=ready");
    console.log(
      `hermes_core_transaction_status=${result.alreadyCommitted ? "already_committed" : "committed"}`
    );
    return 0;
  } catch (error) {
    const code =
      error instanceof Error && /^[a-z][a-z0-9_]+$/.test(error.message)
        ? error.message
        : "hermes_core_production_failed";
    console.error("hermes_core_production=blocked");
    console.error(`hermes_core_production_error=${code}`);
    return 1;
  }
}

if (
  process.argv[1] &&
  path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url))
) {
  process.exitCode = main(process.argv.slice(2));
}
