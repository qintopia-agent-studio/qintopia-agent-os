#!/usr/bin/env node

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { computeHermesCoreLineageFingerprint } from "./plan-hermes-core-release.mjs";
import { HermesCoreTransactionError } from "./run-hermes-core-release-transaction.mjs";
import { runHermesCoreReleaseProductionForTest } from "./run-hermes-core-release-production.mjs";

const tmpRoot = fs.realpathSync.native(
  fs.mkdtempSync(path.join(os.tmpdir(), "hermes-core-production-"))
);
const owner = { uid: process.getuid(), gid: process.getgid() };
const currentCommit = "1".repeat(40);
const previousCommit = "2".repeat(40);
const rollbackReserveCommit = "4".repeat(40);
const candidateCommit = "3".repeat(40);
const requestId = "deploy-20260910T010203Z-0123456789ab";
const expected = {
  tag: "v0.1.0",
  commit: candidateCommit,
  sourceSha256: "a".repeat(64),
  identitySha256: "b".repeat(64),
  manifestSha256: "c".repeat(64),
  currentCommit,
  previousCommit,
  rollbackReserveCommit,
};

try {
  runCase();
  runFailureCase();
} finally {
  makeWritable(tmpRoot);
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Hermes core production controller test passed.");

function runCase() {
  const fixture = createFixture();
  const result = runHermesCoreReleaseProductionForTest(
    fixture.requestPath,
    fixture.evidencePath,
    {
      coreRoot: fixture.coreRoot,
      stateRoot: fixture.stateRoot,
      trustedParent: path.dirname(fixture.coreRoot),
      owner,
      lockFd: fixture.lockFd,
      fetchArtifact(args) {
        assert.deepEqual(args, [
          "--expected-archive-sha256",
          "d".repeat(64),
          "--expected-tag",
          expected.tag,
          "--expected-commit",
          expected.commit,
          "--expected-source-sha256",
          expected.sourceSha256,
          "--expected-identity-sha256",
          expected.identitySha256,
          "--expected-manifest-sha256",
          expected.manifestSha256,
        ]);
        fixture.events.push("fetch");
      },
      stageArtifact(candidate) {
        assert.equal(candidate.tag, expected.tag);
        assert.equal(candidate.commit, expected.commit);
        assert.equal(candidate.sourceSha256, expected.sourceSha256);
        assert.equal(candidate.identitySha256, expected.identitySha256);
        assert.equal(candidate.manifestSha256, expected.manifestSha256);
        writeRelease(
          path.join(fixture.coreRoot, "releases", candidate.commit),
          candidate.commit,
          "0.1.0"
        );
        fixture.events.push("stage");
      },
      installUnits() {
        fixture.events.push("install");
      },
      serviceController: createServiceController(fixture),
      lineageController: createLineageController(fixture),
    }
  );

  assert.equal(result.status, "committed");
  assert.deepEqual(fixture.events.slice(0, 3), ["fetch", "stage", "install"]);
  assert.equal(fixture.events.filter((event) => event.startsWith("stop:")).length, 7);
  assert.equal(fixture.events.filter((event) => event.startsWith("start:")).length, 7);
  assert.equal(fixture.events.filter((event) => event.startsWith("smoke:")).length, 7);
  assert.equal(
    JSON.parse(fs.readFileSync(fixture.evidencePath, "utf8")).transaction_status,
    "committed"
  );
}

function runFailureCase() {
  const fixture = createFixture();
  const serviceController = createServiceController(fixture, "huabaosi");
  assert.throws(
    () =>
      runHermesCoreReleaseProductionForTest(fixture.requestPath, fixture.evidencePath, {
        coreRoot: fixture.coreRoot,
        stateRoot: fixture.stateRoot,
        trustedParent: path.dirname(fixture.coreRoot),
        owner,
        lockFd: fixture.lockFd,
        fetchArtifact: () => fixture.events.push("fetch"),
        stageArtifact: (candidate) => {
          writeRelease(
            path.join(fixture.coreRoot, "releases", candidate.commit),
            candidate.commit,
            "0.1.0"
          );
          fixture.events.push("stage");
        },
        installUnits: () => fixture.events.push("install"),
        serviceController,
        lineageController: createLineageController(fixture),
      }),
    (error) =>
      error instanceof HermesCoreTransactionError &&
      error.rollbackStatus === "succeeded"
  );

  const evidence = JSON.parse(fs.readFileSync(fixture.evidencePath, "utf8"));
  assert.equal(evidence.transaction_status, "rolled_back");
  assert.equal(evidence.error, "hermes_core_candidate_start_failed");
  assert.equal(fixture.events.filter((event) => event.startsWith("stop:")).length, 10);
  assert.equal(fixture.events.filter((event) => event.startsWith("start:")).length, 10);
}

function createFixture() {
  const id = `${Date.now()}-${Math.random().toString(16).slice(2)}`;
  const root = path.join(tmpRoot, `fixture-${id}`, "qintopia-hermes-core");
  const stateRoot = path.join(tmpRoot, `fixture-${id}`, "controller-state");
  const requestPath = path.join(tmpRoot, `fixture-${id}`, "request.json");
  const evidencePath = path.join(stateRoot, "results", `${requestId}.hermes-core.json`);
  const lineage = { currentCommit, previousCommit, rollbackReserveCommit };
  const fingerprint = computeHermesCoreLineageFingerprint(lineage);

  for (const [directory, mode] of [
    [root, 0o755],
    [path.join(root, "releases"), 0o755],
    [path.join(root, "incoming"), 0o700],
    [path.join(root, "quarantine"), 0o700],
    [path.join(root, "state"), 0o700],
    [path.join(root, "state", "transactions"), 0o700],
    [path.join(root, "lineage"), 0o755],
    [path.join(root, "lineage", "generations"), 0o755],
    [stateRoot, 0o700],
    [path.join(stateRoot, "results"), 0o700],
  ]) {
    fs.mkdirSync(directory, { recursive: true, mode });
    fs.chmodSync(directory, mode);
  }
  fs.writeFileSync(path.join(root, "state", "manager.lock"), "\n", { mode: 0o600 });
  fs.chmodSync(path.join(root, "state", "manager.lock"), 0o600);
  const lockFd = fs.openSync(
    path.join(root, "state", "manager.lock"),
    fs.constants.O_RDWR
  );

  for (const commit of [currentCommit, previousCommit, rollbackReserveCommit]) {
    writeRelease(
      path.join(root, "releases", commit),
      commit,
      commit === currentCommit ? "0.0.9" : "0.0.8"
    );
  }
  const generation = path.join(
    root,
    "lineage",
    "generations",
    `generation-${fingerprint}`
  );
  fs.mkdirSync(generation, { mode: 0o700 });
  fs.chmodSync(generation, 0o700);
  writeJson(
    path.join(generation, "lineage.json"),
    {
      schema_version: 1,
      generation_type: "hermes-core-lineage",
      fingerprint_sha256: fingerprint,
      current_commit: currentCommit,
      previous_commit: previousCommit,
      rollback_reserve_commit: rollbackReserveCommit,
    },
    0o444
  );
  for (const [name, commit] of [
    ["current", currentCommit],
    ["previous", previousCommit],
    ["rollback-reserve", rollbackReserveCommit],
  ]) {
    fs.symlinkSync(`../../../releases/${commit}`, path.join(generation, name));
  }
  fs.chmodSync(generation, 0o555);
  fs.symlinkSync(
    `generations/generation-${fingerprint}`,
    path.join(root, "lineage", "active")
  );
  fs.symlinkSync("lineage/active/current", path.join(root, "current"));
  fs.symlinkSync("lineage/active/previous", path.join(root, "previous"));
  fs.symlinkSync(
    "lineage/active/rollback-reserve",
    path.join(root, "rollback-reserve")
  );

  writeJson(
    requestPath,
    {
      schema_version: 1,
      request_id: requestId,
      environment: "production",
      repository: "qintopia-agent-studio/qintopia-agent-os",
      release_scope: ["hermes-core-release"],
      restart_targets: ["hermes-core"],
      rollback_on_smoke_failure: true,
      dry_run: false,
      hermes_core_release: {
        repository: "https://github.com/NousResearch/hermes-agent.git",
        tag: expected.tag,
        commit_sha: expected.commit,
        source_archive_sha256: expected.sourceSha256,
        artifact_identity_sha256: expected.identitySha256,
        artifact_manifest_sha256: expected.manifestSha256,
        previous_version: "0.0.9",
        previous_commit_sha: expected.currentCommit,
        archive_sha256: "d".repeat(64),
      },
    },
    0o600
  );
  return { coreRoot: root, stateRoot, requestPath, evidencePath, lockFd, events: [] };
}

function writeRelease(releasePath, commit, version) {
  fs.mkdirSync(releasePath, { recursive: true, mode: 0o700 });
  writeJson(
    path.join(releasePath, "artifact-manifest.json"),
    {
      schema_version: 1,
      artifact_type: "hermes-core",
      artifact_name: `hermes-core-${commit}`,
      commit_sha: commit,
      hermes_cli_version: version,
    },
    0o444
  );
  fs.chmodSync(releasePath, 0o555);
}

function writeJson(filePath, value, mode) {
  fs.writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`, { mode });
  fs.chmodSync(filePath, mode);
}

function makeWritable(target) {
  if (!fs.existsSync(target)) return;
  const metadata = fs.lstatSync(target);
  if (metadata.isSymbolicLink()) return;
  if (metadata.isDirectory()) {
    for (const name of fs.readdirSync(target)) makeWritable(path.join(target, name));
    fs.chmodSync(target, 0o700);
  } else {
    fs.chmodSync(target, 0o600);
  }
}

function createLineageController(fixture) {
  return {
    prepare() {
      fixture.events.push("prepare");
    },
    verifyCandidate() {
      fixture.events.push("verify-candidate");
    },
    verify() {
      fixture.events.push("verify-lineage");
    },
    commit() {
      fixture.events.push("commit-lineage");
    },
    restore() {
      fixture.events.push("restore-lineage");
    },
  };
}

function createServiceController(fixture, failProfile = null) {
  const states = new Map();
  const profiles = [
    "default",
    "erhua",
    "guanerye",
    "huabaosi",
    "silaoshi",
    "wenyuange",
    "xiaoman",
  ];
  for (const profile of profiles) states.set(profile, true);
  let failed = false;
  return {
    snapshot(profile) {
      return {
        loadState: "loaded",
        activeState: states.get(profile.id) ? "active" : "inactive",
        unitFileState: "enabled",
        execStartSha256: "e".repeat(64),
      };
    },
    stop(profile) {
      states.set(profile.id, false);
      fixture.events.push(`stop:${profile.id}`);
    },
    start(profile) {
      if (profile.id === failProfile && !failed) {
        failed = true;
        throw new Error("hermes_core_candidate_start_failed");
      }
      states.set(profile.id, true);
      fixture.events.push(`start:${profile.id}`);
    },
    restoreUnitFileState() {},
    daemonReload() {
      fixture.events.push("daemon-reload");
    },
    smoke(profile) {
      fixture.events.push(`smoke:${profile.id}`);
    },
  };
}
