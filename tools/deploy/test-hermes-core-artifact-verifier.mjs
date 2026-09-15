#!/usr/bin/env node

import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import Ajv2020 from "ajv/dist/2020.js";
import { computeHermesCoreArtifactIdentity } from "./verify-hermes-core-artifact.mjs";

const repoRoot = process.cwd();
const verifierPath = path.join(
  repoRoot,
  "tools",
  "deploy",
  "verify-hermes-core-artifact.mjs"
);
const contractDirectory = path.join(
  repoRoot,
  "runtime",
  "hermes",
  "core-release-contracts"
);
const ajv = new Ajv2020({ allErrors: true, strict: true });
const contractValidators = new Map(
  ["artifact-manifest", "build-receipt", "update-receipt", "validation-summary"].map(
    (name) => [
      name,
      ajv.compile(
        JSON.parse(
          fs.readFileSync(path.join(contractDirectory, `${name}.schema.json`), "utf8")
        )
      ),
    ]
  )
);
const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "hermes-core-artifact-"));
const repository = "https://github.com/NousResearch/hermes-agent.git";
const commit = "0123456789abcdef0123456789abcdef01234567";
const otherCommit = "89abcdef0123456789abcdef0123456789abcdef";
const tag = "v1.2.3";
const sourceSha256 = "a".repeat(64);
let fixtureSequence = 0;

try {
  let fixture = createFixture();
  let result = runVerifier(fixture);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /hermes_core_artifact_verification=ready/);
  assert.match(result.stdout, /hermes_core_artifact_file_count=5/);
  assertSanitized(result, fixture.root);

  fixture = createFixture();
  result = runVerifier(fixture, { expectedManifestSha256: "f".repeat(64) });
  assertBlocked(result, /manifest_digest_mismatch/, fixture.root);

  fixture = createFixture();
  rewriteReadOnly(
    path.join(fixture.root, "core", "hermes_cli", "main.py"),
    "tampered\n"
  );
  result = runVerifier(fixture);
  assertBlocked(result, /artifact_inventory_mismatch/, fixture.root);

  fixture = createFixture();
  fs.unlinkSync(path.join(fixture.root, "build-receipt.json"));
  result = runVerifier(fixture);
  assertBlocked(result, /artifact_layout_invalid/, fixture.root);

  fixture = createFixture();
  mutateJson(fixture, "artifact-manifest.json", (manifest) => {
    manifest.unexpected = true;
  });
  assertContractRejected(fixture.root, "artifact-manifest");
  fixture.expectedManifestSha256 = sha256File(
    path.join(fixture.root, "artifact-manifest.json")
  );
  regenerateChecksums(fixture.root);
  result = runVerifier(fixture);
  assertBlocked(result, /manifest_schema_invalid/, fixture.root);

  fixture = createFixture();
  mutateJson(fixture, "build-receipt.json", (receipt) => {
    receipt.builder.image_digest = `sha256:${"e".repeat(64)}`;
  });
  result = runVerifier(fixture);
  assertBlocked(result, /receipt_digest_mismatch/, fixture.root);

  fixture = createFixture();
  mutateJson(fixture, "build-receipt.json", (receipt) => {
    receipt.outcome = "failed";
  });
  assertContractRejected(fixture.root, "build-receipt");
  refreshManifestReceipt(fixture, "build", "build-receipt.json");
  result = runVerifier(fixture);
  assertBlocked(result, /build_receipt_schema_invalid/, fixture.root);

  fixture = createFixture();
  mutateJson(fixture, "build-receipt.json", (receipt) => {
    receipt.commit_sha = otherCommit;
  });
  refreshManifestReceipt(fixture, "build", "build-receipt.json");
  result = runVerifier(fixture);
  assertBlocked(result, /build_receipt_identity_mismatch/, fixture.root);

  fixture = createFixture();
  mutateJson(fixture, "update-receipt.json", (receipt) => {
    receipt.outcome = "failed";
  });
  assertContractRejected(fixture.root, "update-receipt");
  refreshManifestReceipt(fixture, "update", "update-receipt.json");
  result = runVerifier(fixture);
  assertBlocked(result, /update_receipt_schema_invalid/, fixture.root);

  fixture = createFixture();
  mutateJson(fixture, "validation-summary.json", (summary) => {
    summary.sensitive_values_included = true;
  });
  assertContractRejected(fixture.root, "validation-summary");
  refreshManifestReceipt(fixture, "validation", "validation-summary.json");
  result = runVerifier(fixture);
  assertBlocked(result, /validation_summary_schema_invalid/, fixture.root);

  fixture = createFixture();
  replaceWithSymlink(
    path.join(fixture.root, "core", "hermes_cli", "main.py"),
    path.join(tmpRoot, "outside-sensitive")
  );
  result = runVerifier(fixture);
  assertBlocked(result, /artifact_symlink_rejected/, fixture.root);

  fixture = createFixture();
  replaceWithHardlink(
    path.join(fixture.root, "core", "hermes_cli", "main.py"),
    path.join(fixture.root, "core", "pyproject.toml")
  );
  result = runVerifier(fixture);
  assertBlocked(result, /artifact_hardlink_rejected/, fixture.root);

  fixture = createFixture();
  addCoreFile(fixture.root, "core/unlisted.py", "unlisted\n");
  result = runVerifier(fixture);
  assertBlocked(result, /artifact_inventory_mismatch/, fixture.root);

  fixture = createFixture();
  fs.chmodSync(path.join(fixture.root, "core", "pyproject.toml"), 0o644);
  result = runVerifier(fixture);
  assertBlocked(result, /artifact_mode_invalid/, fixture.root);

  fixture = createFixture();
  tamperChecksumDigest(fixture.root);
  result = runVerifier(fixture);
  assertBlocked(result, /checksum_mismatch/, fixture.root);

  fixture = createFixture();
  duplicateChecksumEntry(fixture.root);
  result = runVerifier(fixture);
  assertBlocked(result, /checksum_manifest_invalid/, fixture.root);

  fixture = createFixture();
  removeChecksumEntry(fixture.root);
  result = runVerifier(fixture);
  assertBlocked(result, /checksum_inventory_mismatch/, fixture.root);

  fixture = createFixture();
  addSpecialFile(fixture.root, "core/rejected-pipe");
  result = runVerifier(fixture);
  assertBlocked(result, /artifact_special_file_rejected/, fixture.root);

  fixture = createFixture();
  const rootLink = path.join(tmpRoot, `root-link-${fixtureSequence}`);
  fs.symlinkSync(fixture.root, rootLink, "dir");
  result = runVerifier(fixture, { artifactDir: rootLink });
  assertBlocked(result, /artifact_root_invalid/, fixture.root);

  fixture = createFixture();
  result = spawnSync(process.execPath, [verifierPath, "--artifact-dir", fixture.root], {
    cwd: repoRoot,
    encoding: "utf8",
  });
  assert.equal(result.status, 2);
  assert.match(result.stderr, /hermes_core_artifact_error=invalid_invocation/);
  assertSanitized(result, fixture.root);
} finally {
  makeWritable(tmpRoot);
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Hermes core artifact verifier test passed.");

function createFixture() {
  fixtureSequence += 1;
  const root = path.join(tmpRoot, `fixture-${fixtureSequence}`);
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

  const identitySource = {
    repository,
    tag,
    commit_sha: commit,
    source_archive_sha256: sourceSha256,
    python_abi: "cp312",
    dependency_lock_sha256: "b".repeat(64),
    runtime_binding_sha256: crypto
      .createHash("sha256")
      .update(`${JSON.stringify(runtime)}\n`)
      .digest("hex"),
    hermes_cli_version: "1.2.3",
  };
  const identitySha256 = computeHermesCoreArtifactIdentity(identitySource);
  const sharedIdentity = {
    ...identitySource,
    artifact_identity_sha256: identitySha256,
    runtime_binding_sha256: identitySource.runtime_binding_sha256,
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
      image_digest: `sha256:${"c".repeat(64)}`,
    },
    source_checkout: { clean: true, tag_commit_verified: true },
    dependency_install: { locked: true, hashes_verified: true },
  });
  writeJson(path.join(root, "update-receipt.json"), {
    schema_version: 1,
    receipt_type: "hermes-core-update",
    outcome: "success",
    strategy: "hermes-update-clean-candidate",
    repository,
    tag,
    commit_sha: commit,
    source_archive_sha256: sourceSha256,
    artifact_identity_sha256: identitySha256,
    previous_version: "1.2.2",
    previous_commit_sha: otherCommit,
    new_version: "1.2.3",
    started_at: "2026-09-10T00:02:00Z",
    completed_at: "2026-09-10T00:08:00.000Z",
    upstream_receipt_sha256: "d".repeat(64),
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
    artifact_name: `hermes-core-${commit}`,
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
  for (const name of [
    "artifact-manifest.json",
    "build-receipt.json",
    "update-receipt.json",
    "validation-summary.json",
  ]) {
    fs.chmodSync(path.join(root, name), 0o444);
  }
  regenerateChecksums(root);
  assertFixtureMatchesContracts(root);

  return {
    root,
    expectedTag: tag,
    expectedCommit: commit,
    expectedSourceSha256: sourceSha256,
    expectedIdentitySha256: identitySha256,
    expectedManifestSha256: sha256File(path.join(root, "artifact-manifest.json")),
  };
}

function runVerifier(fixture, overrides = {}) {
  const expected = { ...fixture, ...overrides };
  return spawnSync(
    process.execPath,
    [
      verifierPath,
      "--artifact-dir",
      expected.artifactDir ?? fixture.root,
      "--expected-tag",
      expected.expectedTag,
      "--expected-commit",
      expected.expectedCommit,
      "--expected-source-sha256",
      expected.expectedSourceSha256,
      "--expected-identity-sha256",
      expected.expectedIdentitySha256,
      "--expected-manifest-sha256",
      expected.expectedManifestSha256,
    ],
    { cwd: repoRoot, encoding: "utf8" }
  );
}

function assertBlocked(result, pattern, fixtureRoot) {
  assert.notEqual(result.status, 0, result.stdout);
  assert.match(result.stderr, /hermes_core_artifact_verification=blocked/);
  assert.match(result.stderr, pattern);
  assertSanitized(result, fixtureRoot);
}

function assertSanitized(result, fixtureRoot) {
  const output = `${result.stdout}\n${result.stderr}`;
  assert.doesNotMatch(output, /super-secret-value/);
  assert.equal(output.includes(fixtureRoot), false, "output exposed fixture path");
}

function writeJson(filePath, value) {
  fs.writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`);
}

function rewriteReadOnly(filePath, value) {
  fs.chmodSync(filePath, 0o644);
  fs.writeFileSync(filePath, value);
  fs.chmodSync(filePath, 0o444);
}

function mutateJson(fixture, name, mutate) {
  const filePath = path.join(fixture.root, name);
  const value = JSON.parse(fs.readFileSync(filePath, "utf8"));
  mutate(value);
  fs.chmodSync(filePath, 0o644);
  writeJson(filePath, value);
  fs.chmodSync(filePath, 0o444);
}

function refreshManifestReceipt(fixture, key, name) {
  const manifestPath = path.join(fixture.root, "artifact-manifest.json");
  const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
  manifest.receipts[key].sha256 = sha256File(path.join(fixture.root, name));
  fs.chmodSync(manifestPath, 0o644);
  writeJson(manifestPath, manifest);
  fs.chmodSync(manifestPath, 0o444);
  fixture.expectedManifestSha256 = sha256File(manifestPath);
  regenerateChecksums(fixture.root);
}

function replaceWithSymlink(filePath, targetPath) {
  fs.writeFileSync(targetPath, "super-secret-value\n");
  const parent = path.dirname(filePath);
  fs.chmodSync(parent, 0o755);
  fs.unlinkSync(filePath);
  fs.symlinkSync(targetPath, filePath);
  fs.chmodSync(parent, 0o555);
}

function replaceWithHardlink(filePath, targetPath) {
  const parent = path.dirname(filePath);
  fs.chmodSync(parent, 0o755);
  fs.unlinkSync(filePath);
  fs.linkSync(targetPath, filePath);
  fs.chmodSync(parent, 0o555);
}

function addCoreFile(root, relativePath, value) {
  const filePath = path.join(root, ...relativePath.split("/"));
  const parent = path.dirname(filePath);
  fs.chmodSync(parent, 0o755);
  fs.writeFileSync(filePath, value);
  fs.chmodSync(filePath, 0o444);
  fs.chmodSync(parent, 0o555);
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

function regenerateChecksums(root) {
  const checksumPath = path.join(root, "SHA256SUMS");
  const files = [
    "artifact-manifest.json",
    "build-receipt.json",
    "update-receipt.json",
    "validation-summary.json",
    ...inventoryCore(root)
      .filter((entry) => entry.type === "file")
      .map((entry) => entry.path),
  ].sort();
  if (fs.existsSync(checksumPath)) {
    fs.chmodSync(checksumPath, 0o644);
  }
  fs.writeFileSync(
    checksumPath,
    `${files
      .map(
        (relativePath) =>
          `${sha256File(path.join(root, ...relativePath.split("/")))}  ${relativePath}`
      )
      .join("\n")}\n`
  );
  fs.chmodSync(checksumPath, 0o444);
}

function assertFixtureMatchesContracts(root) {
  for (const name of [
    "artifact-manifest",
    "build-receipt",
    "update-receipt",
    "validation-summary",
  ]) {
    const value = JSON.parse(fs.readFileSync(path.join(root, `${name}.json`), "utf8"));
    assert.equal(
      contractValidators.get(name)(value),
      true,
      `${name} fixture must match its repository schema`
    );
  }
}

function assertContractRejected(root, name) {
  const value = JSON.parse(fs.readFileSync(path.join(root, `${name}.json`), "utf8"));
  assert.equal(
    contractValidators.get(name)(value),
    false,
    `${name} negative fixture must be rejected by its repository schema`
  );
}

function tamperChecksumDigest(root) {
  const checksumPath = path.join(root, "SHA256SUMS");
  const lines = fs.readFileSync(checksumPath, "utf8").trimEnd().split("\n");
  lines[0] = `${"0".repeat(64)}${lines[0].slice(64)}`;
  rewriteReadOnly(checksumPath, `${lines.join("\n")}\n`);
}

function duplicateChecksumEntry(root) {
  const checksumPath = path.join(root, "SHA256SUMS");
  const lines = fs.readFileSync(checksumPath, "utf8").trimEnd().split("\n");
  rewriteReadOnly(checksumPath, `${[...lines, lines[0]].join("\n")}\n`);
}

function removeChecksumEntry(root) {
  const checksumPath = path.join(root, "SHA256SUMS");
  const lines = fs.readFileSync(checksumPath, "utf8").trimEnd().split("\n");
  rewriteReadOnly(checksumPath, `${lines.slice(1).join("\n")}\n`);
}

function addSpecialFile(root, relativePath) {
  const filePath = path.join(root, ...relativePath.split("/"));
  const parent = path.dirname(filePath);
  fs.chmodSync(parent, 0o755);
  const result = spawnSync("mkfifo", [filePath], { encoding: "utf8" });
  assert.equal(result.status, 0, result.stderr);
  fs.chmodSync(parent, 0o555);
}

function sha256File(filePath) {
  return crypto.createHash("sha256").update(fs.readFileSync(filePath)).digest("hex");
}

function makeWritable(directoryPath) {
  if (!fs.existsSync(directoryPath)) {
    return;
  }
  for (const name of fs.readdirSync(directoryPath)) {
    const entryPath = path.join(directoryPath, name);
    const metadata = fs.lstatSync(entryPath);
    if (metadata.isDirectory()) {
      makeWritable(entryPath);
    } else if (!metadata.isSymbolicLink()) {
      fs.chmodSync(entryPath, 0o600);
    }
  }
  fs.chmodSync(directoryPath, 0o700);
}
