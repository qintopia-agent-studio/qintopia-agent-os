#!/usr/bin/env node

import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { computeHermesCoreArtifactIdentity } from "./verify-hermes-core-artifact.mjs";

const repoRoot = process.cwd();
const extractorPath = path.join(
  repoRoot,
  "tools",
  "deploy",
  "extract-hermes-core-artifact.py"
);
const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "hermes-core-extractor-"));
const repository = "https://github.com/NousResearch/hermes-agent.git";
const commit = "0123456789abcdef0123456789abcdef01234567";
const tag = "v1.2.3";
const sourceSha256 = "a".repeat(64);
const runtime = {
  kind: "release-local-venv",
  platform: "linux-x86_64",
  interpreter_path: "runtime/venv/bin/python",
  site_packages_path: "runtime/venv/lib/python3.12/site-packages",
  launcher_path: "runtime/hermes-core-launcher.py",
};
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
const extensionLimit = 1024 * 1024;
const extensionTotalLimit = 8 * 1024 * 1024;
const archiveLimit = 512 * 1024 * 1024;
const memberLimit = 256 * 1024 * 1024;
const totalLimit = 1024 * 1024 * 1024;

const driver = String.raw`
import importlib.util
import json
import os
import sys

module_path, operation = sys.argv[1:3]
spec = importlib.util.spec_from_file_location("hermes_core_extractor", module_path)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
owner = (os.getuid(), os.getgid())

try:
    if operation == "constants":
        print(json.dumps({
            "archive": module.MAX_ARCHIVE_BYTES,
            "member": module.MAX_MEMBER_BYTES,
            "total": module.MAX_TOTAL_BYTES,
            "uncompressed": module.MAX_UNCOMPRESSED_ARCHIVE_BYTES,
            "extension_total": module.MAX_EXTENDED_HEADER_TOTAL_BYTES,
            "extension_count": module.MAX_EXTENDED_HEADERS,
        }))
    elif operation == "extract":
        archive_path, output_path, expected_json = sys.argv[3:6]
        module.extract_archive(
            archive_path,
            output_path,
            json.loads(expected_json),
            owner=owner,
            sync_path=lambda _path: None,
        )
    elif operation == "main":
        argv = json.loads(sys.argv[3])
        module.os.geteuid = lambda: 0
        module._same_owner = lambda _metadata, _owner: True
        module._chown = lambda _path, _owner: None
        raise SystemExit(module._main(argv))
    else:
        raise RuntimeError("unknown fixture operation")
except module.ArtifactError as error:
    print(error.code)
    raise SystemExit(1)
except SystemExit:
    raise
except Exception:
    print("unexpected_fixture_failure")
    raise SystemExit(1)
`;

try {
  const constants = runPythonConstants();
  assert.deepEqual(constants, {
    archive: archiveLimit,
    member: memberLimit,
    total: totalLimit,
    uncompressed: totalLimit + 200000 * 1024 + 20 * 1024 * 1024,
    extension_total: extensionTotalLimit,
    extension_count: 1024,
  });

  const fixture = createValidArtifact();
  const expected = {
    archive_sha256: fixture.archiveSha256,
    tag,
    commit,
    source_sha256: sourceSha256,
    identity_sha256: identitySha256,
    manifest_sha256: fixture.manifestSha256,
  };

  const cliOutput = path.join(tmpRoot, "cli-output");
  const cliResult = runCliSuccess(fixture.archivePath, cliOutput, expected);
  assert.equal(cliResult.status, 0, cliResult.stderr);
  assert.match(cliResult.stdout, /hermes_core_artifact_extraction=ready/);
  assert.match(cliResult.stdout, new RegExp(`hermes_core_artifact_commit=${commit}`));
  assertExtractedArtifact(cliOutput);

  const missingStagingResult = runMain([
    "--extract",
    "--archive",
    fixture.archivePath,
    "--expected-archive-sha256",
    expected.archive_sha256,
    "--expected-tag",
    expected.tag,
    "--expected-commit",
    expected.commit,
    "--expected-source-sha256",
    expected.source_sha256,
    "--expected-identity-sha256",
    expected.identity_sha256,
    "--expected-manifest-sha256",
    expected.manifest_sha256,
  ]);
  assert.equal(missingStagingResult.status, 2);
  assert.match(
    missingStagingResult.stderr,
    /hermes_core_artifact_error=invalid_invocation/
  );

  const directOutput = path.join(tmpRoot, "direct-output");
  const directResult = runPythonExtract(fixture.archivePath, directOutput, expected);
  assert.equal(directResult.status, 0, directResult.stderr);
  assertExtractedArtifact(directOutput);

  for (const [kind, errorCode] of [
    ["pax", "archive_extension_size_invalid"],
    ["global-pax", "archive_extension_size_invalid"],
    ["gnu-longname", "archive_extension_size_invalid"],
    ["gnu-longlink", "archive_extension_size_invalid"],
    ["extension-total", "archive_extension_total_invalid"],
    ["member-too-large", "archive_size_invalid"],
    ["sparse", "archive_sparse_rejected"],
    ["bad-checksum", "archive_header_invalid"],
    ["truncated-header", "archive_truncated"],
    ["truncated-body", "archive_truncated"],
    ["malformed-pax", "archive_truncated"],
  ]) {
    const archivePath = writeMaliciousArchive(kind);
    const output = path.join(tmpRoot, `${kind}-output`);
    const result = runPythonExtract(archivePath, output, {
      ...expected,
      archive_sha256: sha256File(archivePath),
    });
    assert.notEqual(result.status, 0, `${kind} unexpectedly succeeded`);
    assert.equal(result.stdout.trim(), errorCode, result.stderr);
    if (kind !== "malformed-pax") {
      assert.equal(
        fs.existsSync(output),
        false,
        `${kind} created output before prescan`
      );
    }
  }
} finally {
  makeWritable(tmpRoot);
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Hermes core extractor boundary test passed.");

function runPythonConstants() {
  const result = spawnSync("python3", ["-c", driver, extractorPath, "constants"], {
    cwd: repoRoot,
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr);
  return JSON.parse(result.stdout);
}

function runPythonExtract(archivePath, outputPath, expected) {
  return spawnSync(
    "python3",
    [
      "-c",
      driver,
      extractorPath,
      "extract",
      archivePath,
      outputPath,
      JSON.stringify(expected),
    ],
    { cwd: repoRoot, encoding: "utf8" }
  );
}

function runMain(argv) {
  return spawnSync(
    "python3",
    ["-c", driver, extractorPath, "main", JSON.stringify(argv)],
    { cwd: repoRoot, encoding: "utf8" }
  );
}

function runCliSuccess(archivePath, outputPath, expected) {
  const argv = [
    "--extract",
    "--archive",
    archivePath,
    "--staging",
    outputPath,
    "--expected-archive-sha256",
    expected.archive_sha256,
    "--expected-tag",
    expected.tag,
    "--expected-commit",
    expected.commit,
    "--expected-source-sha256",
    expected.source_sha256,
    "--expected-identity-sha256",
    expected.identity_sha256,
    "--expected-manifest-sha256",
    expected.manifest_sha256,
  ];
  if (process.getuid?.() === 0) {
    return spawnSync("python3", [extractorPath, ...argv], {
      cwd: repoRoot,
      encoding: "utf8",
    });
  }
  return runMain(argv);
}

function createValidArtifact() {
  const root = path.join(tmpRoot, "valid-artifact");
  const core = path.join(root, "core");
  const hermesCli = path.join(core, "hermes_cli");
  fs.mkdirSync(hermesCli, { recursive: true });
  fs.writeFileSync(
    path.join(core, "pyproject.toml"),
    "[project]\nname='hermes-agent'\n"
  );
  fs.writeFileSync(path.join(hermesCli, "main.py"), "print('fixture')\n");
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
    previous_commit_sha: "89abcdef0123456789abcdef0123456789abcdef",
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

  const archivePath = path.join(tmpRoot, "valid-artifact.tar.gz");
  writePaxTarArchive(root, archivePath);
  return {
    root,
    archivePath,
    archiveSha256: sha256File(archivePath),
    manifestSha256: sha256File(path.join(root, "artifact-manifest.json")),
  };
}

function writeMaliciousArchive(kind) {
  const archivePath = path.join(tmpRoot, `${kind}.tar.gz`);
  const code = String.raw`
import gzip
import sys

archive_path, kind, extension_limit = sys.argv[1:4]
extension_limit = int(extension_limit)
BLOCK = 512
ZERO = bytes(BLOCK)

def number_field(value, width):
    encoded = oct(value)[2:].encode("ascii")
    if len(encoded) + 1 > width:
        raise ValueError("fixture number overflow")
    return b"0" * (width - len(encoded) - 1) + encoded + bytes([0])

def make_header(name, typeflag, size):
    block = bytearray(BLOCK)
    encoded_name = name.encode("ascii")
    block[:len(encoded_name)] = encoded_name
    block[100:108] = number_field(0o644, 8)
    block[108:116] = number_field(0, 8)
    block[116:124] = number_field(0, 8)
    block[124:136] = number_field(size, 12)
    block[136:148] = number_field(0, 12)
    block[148:156] = b"        "
    block[156:157] = typeflag
    block[257:265] = b"ustar " + bytes(2)
    checksum = sum(block)
    block[148:156] = f"{checksum:06o}".encode("ascii") + bytes([0]) + b" "
    return bytes(block)

def padded(data):
    return data + bytes((-len(data)) % BLOCK)

with gzip.open(archive_path, "wb") as target:
    if kind in {"pax", "global-pax", "gnu-longname", "gnu-longlink"}:
        typeflag = {
            "pax": b"x",
            "global-pax": b"g",
            "gnu-longname": b"L",
            "gnu-longlink": b"K",
        }[kind]
        body = b"x" * (extension_limit + 1)
        target.write(make_header("extension", typeflag, len(body)))
        target.write(padded(body))
        target.write(ZERO)
        target.write(ZERO)
    elif kind == "extension-total":
        body = b"x" * extension_limit
        for _ in range(9):
            target.write(make_header("extension", b"g", len(body)))
            target.write(padded(body))
        target.write(ZERO)
        target.write(ZERO)
    elif kind == "sparse":
        target.write(make_header("core/file", b"S", 0))
        target.write(ZERO)
        target.write(ZERO)
    elif kind == "member-too-large":
        target.write(make_header("core/file", b"0", 256 * 1024 * 1024 + 1))
        target.write(ZERO)
        target.write(ZERO)
    elif kind == "bad-checksum":
        block = bytearray(make_header("core/file", b"0", 0))
        block[0] ^= 1
        target.write(block)
        target.write(ZERO)
        target.write(ZERO)
    elif kind == "truncated-header":
        target.write(make_header("core/file", b"0", 0)[:100])
    elif kind == "truncated-body":
        target.write(make_header("core/file", b"0", 16))
        target.write(b"short")
    elif kind == "malformed-pax":
        body = b"bad!!"
        target.write(make_header("extension", b"x", len(body)))
        target.write(padded(body))
        target.write(make_header("core/file", b"0", 0))
        target.write(ZERO)
        target.write(ZERO)
    else:
        raise ValueError("unknown fixture kind")
`;
  const result = spawnSync(
    "python3",
    ["-c", code, archivePath, kind, String(extensionLimit)],
    { cwd: repoRoot, encoding: "utf8" }
  );
  assert.equal(result.status, 0, result.stderr);
  return archivePath;
}

function writePaxTarArchive(root, archivePath) {
  const relativeEntries = listEntries(root).sort((left, right) => {
    const leftDepth = left.split("/").length;
    const rightDepth = right.split("/").length;
    return leftDepth - rightDepth || left.localeCompare(right);
  });
  const code = String.raw`
import os
import sys
import tarfile

archive_path, root, *entries = sys.argv[1:]
with tarfile.open(archive_path, "w:gz", format=tarfile.PAX_FORMAT) as archive:
    for relative in entries:
        source_path = os.path.join(root, relative)
        info = archive.gettarinfo(source_path, arcname=relative)
        if relative == "artifact-manifest.json":
            info.pax_headers = {"comment": "valid-pax-fixture"}
        if info.isreg():
            with open(source_path, "rb") as source:
                archive.addfile(info, source)
        else:
            archive.addfile(info)
`;
  const result = spawnSync(
    "python3",
    ["-c", code, archivePath, root, ...relativeEntries],
    { cwd: repoRoot, encoding: "utf8" }
  );
  assert.equal(result.status, 0, result.stderr);
}

function listEntries(root) {
  const entries = [];
  const visit = (directory, relativeDirectory) => {
    for (const name of fs.readdirSync(directory).sort()) {
      const absolute = path.join(directory, name);
      const relative = relativeDirectory ? `${relativeDirectory}/${name}` : name;
      entries.push(relative);
      if (fs.lstatSync(absolute).isDirectory()) {
        visit(absolute, relative);
      }
    }
  };
  visit(root, "");
  return entries;
}

function inventoryCore(root) {
  const entries = [];
  const visit = (directoryPath, relativeDirectory) => {
    for (const name of fs.readdirSync(directoryPath).sort()) {
      const absolutePath = path.join(directoryPath, name);
      const relativePath = `${relativeDirectory}/${name}`;
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

function assertExtractedArtifact(root) {
  assert.equal(fs.lstatSync(root).mode & 0o7777, 0o555);
  assert.equal(
    fs.readFileSync(path.join(root, "core", "hermes_cli", "main.py"), "utf8"),
    "print('fixture')\n"
  );
  for (const name of [
    "artifact-manifest.json",
    "build-receipt.json",
    "update-receipt.json",
    "validation-summary.json",
    "SHA256SUMS",
  ]) {
    const metadata = fs.lstatSync(path.join(root, name));
    assert.equal(metadata.nlink, 1);
    assert.equal(metadata.mode & 0o7777, 0o444);
  }
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

function writeJson(filePath, value) {
  fs.writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`);
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
