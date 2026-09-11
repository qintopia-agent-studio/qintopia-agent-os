#!/usr/bin/env node

import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { computeHermesCoreArtifactIdentity } from "./verify-hermes-core-artifact.mjs";

const repoRoot = process.cwd();
const shellPath = path.join(
  repoRoot,
  "deploy",
  "runner",
  "fetch-hermes-core-artifact.sh"
);
const extractorPath = path.join(
  repoRoot,
  "tools",
  "deploy",
  "extract-hermes-core-artifact.py"
);
const verifierPath = path.join(
  repoRoot,
  "tools",
  "deploy",
  "verify-hermes-core-artifact.mjs"
);
const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "hermes-core-ingress-fetch-"));
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
    if operation == "extract":
        archive_path, output_path, expected_json = sys.argv[3:6]
        module.extract_archive(
            archive_path,
            output_path,
            json.loads(expected_json),
            owner=owner,
            sync_path=lambda _path: None,
        )
    elif operation == "commit":
        staging, ingress, expected_commit, fault = sys.argv[3:7]

        def rename_fail(_source, _target):
            raise OSError("fixture rename failure")

        def rename_uncertain(source, target):
            os.rename(source, target)
            raise OSError("fixture post-rename failure")

        sync_calls = 0

        def sync_post_failure(_path):
            nonlocal_sync[0] += 1
            if nonlocal_sync[0] == 2:
                raise module.ArtifactError("fixture fsync failure")

        nonlocal_sync = [0]
        if fault == "rename-failed":
            rename = rename_fail
            sync = lambda _path: None
        elif fault == "rename-uncertain":
            rename = rename_uncertain
            sync = lambda _path: None
        elif fault == "post-rename-fsync":
            rename = os.rename
            sync = sync_post_failure
        else:
            rename = os.rename
            sync = lambda _path: None

        module.commit_artifact(
            staging,
            expected_commit,
            ingress_root=ingress,
            owner=owner,
            sync_path=sync,
            rename=rename,
        )
    else:
        raise RuntimeError("unknown fixture operation")
except module.ArtifactError as error:
    print(error.code)
    raise SystemExit(1)
except Exception:
    print("unexpected_fixture_failure")
    raise SystemExit(1)
`;

try {
  const fixture = createValidArtifact();
  const expected = {
    archive_sha256: fixture.archiveSha256,
    tag,
    commit,
    source_sha256: sourceSha256,
    identity_sha256: identitySha256,
    manifest_sha256: fixture.manifestSha256,
  };

  const shell = fs.readFileSync(shellPath, "utf8");
  assert.match(shell, /if \[\[ "\$\{EUID\}" -ne 0 \]\]/);
  assert.match(shell, /readonly COS_OBJECT_PREFIX=qintopia-agent-os\/hermes-core/);
  assert.match(
    shell,
    /object_key="\$\{COS_OBJECT_PREFIX\}\/\$\{expected_commit\}\/hermes-core-\$\{expected_commit\}\.tar\.gz"/
  );
  assert.doesNotMatch(shell, /TENCENT_COS_PREFIX/);
  for (const forbiddenArgument of [
    "--object-key",
    "--url",
    "--output-dir",
    "--archive-path",
    "--secret_id",
    "--secret_key",
    "--session_token",
  ]) {
    assert.doesNotMatch(
      shell,
      new RegExp(forbiddenArgument.replaceAll("-", "\\-")),
      `shell must not expose ${forbiddenArgument}`
    );
  }
  assert.ok(
    shell.indexOf('if [[ "${EUID}" -ne 0 ]]') <
      shell.indexOf("result_code=coscli_override_rejected")
  );
  assert.match(shell, /readonly MAX_ARCHIVE_BYTES=\$\(\(512 \* 1024 \* 1024\)\)/);
  assert.match(shell, /readonly DOWNLOAD_TIMEOUT_SECONDS=600/);
  assert.match(shell, /readonly MAX_COSCLI_LOG_BYTES=\$\(\(16 \* 1024 \* 1024\)\)/);
  assert.match(shell, /readonly MAX_FAILED_QUARANTINES=2/);
  assert.match(shell, /readonly TIMEOUT_BIN=\/usr\/bin\/timeout/);
  assert.match(shell, /readonly PRLIMIT_BIN=\/usr\/bin\/prlimit/);
  assert.match(shell, /--fsize="\$\{MAX_ARCHIVE_BYTES\}:\$\{MAX_ARCHIVE_BYTES\}"/);
  assert.match(shell, /result_code=download_size_invalid/);
  assert.match(shell, /result_code=quarantine_capacity_reached/);
  assert.match(shell, /result_code=existing_artifact_commit_uncertain/);
  assert.match(shell, /cleanup_untrusted_archive \|\| cleanup_failed=1/);
  assert.match(shell, /elif ! quarantine_attempt/);
  assert.match(shell, /result_code=sensitive_cleanup_failed/);
  assert.match(shell, /stat\.S_IMODE\(metadata\.st_mode\) & 0o022/);
  assert.doesNotMatch(shell, /coscli_path="\$\{COSCLI_PATH:-\}"/);
  assert.match(shell, /emit_ready true/);
  assert.match(shell, /mv -- "\$attempt_dir" "\$slot"/);
  assert.match(shell, /result_code=commit_uncertain/);
  assert.doesNotMatch(shell, /systemctl|poll-deploy-requests/);

  const fakeCoscli = writeFakeCoscli();
  const objectKey = `qintopia-agent-os/hermes-core/${commit}/hermes-core-${commit}.tar.gz`;
  const objectUri = `cos://qintopia-agent-os-hermes-core/${objectKey}`;
  const downloadedArchive = path.join(tmpRoot, "fake-download.tar.gz");
  const fakeConfigResult = runFakeCoscli(fakeCoscli, [
    "config",
    "add",
    "-b",
    "fixture-bucket-1234567890",
    "-r",
    "ap-shanghai",
    "-a",
    "qintopia-agent-os-hermes-core",
  ]);
  assert.equal(fakeConfigResult.status, 0, fakeConfigResult.stderr);
  const fakeDownloadResult = runFakeCoscli(
    fakeCoscli,
    ["cp", objectUri, downloadedArchive, "-c", "fixture.yaml"],
    { FIXTURE_ARCHIVE: fixture.archivePath }
  );
  assert.equal(fakeDownloadResult.status, 0, fakeDownloadResult.stderr);
  assert.equal(
    fs.readFileSync(downloadedArchive).toString("hex"),
    fs.readFileSync(fixture.archivePath).toString("hex")
  );
  assert.match(
    fs.readFileSync(path.join(tmpRoot, "fake-coscli-args.log"), "utf8"),
    new RegExp(objectUri.replaceAll("/", "\\/"))
  );

  const nonRootShell = spawnSync(
    "bash",
    [
      shellPath,
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
    ],
    {
      cwd: repoRoot,
      env: {
        ...process.env,
        COSCLI_PATH: fakeCoscli,
        TENCENT_COS_SECRET_ID: "fixture-secret-id",
        TENCENT_COS_SECRET_KEY: "fixture-secret-key",
        TENCENT_COS_BUCKET: "fixture-bucket-1234567890",
        TENCENT_COS_REGION: "ap-shanghai",
      },
      encoding: "utf8",
    }
  );
  if (process.getuid?.() !== 0) {
    assert.notEqual(nonRootShell.status, 0);
    assert.match(nonRootShell.stderr, /hermes_core_artifact_error=root_required/);
    assert.doesNotMatch(
      `${nonRootShell.stdout}\n${nonRootShell.stderr}`,
      /fixture-secret-(?:id|key)/
    );
  }

  const extracted = path.join(tmpRoot, "extracted-good");
  assert.equal(
    runPython("extract", fixture.archivePath, extracted, expected).status,
    0
  );
  assertNormalizedArtifact(extracted, expected);
  const verifierResult = runVerifier(extracted, expected);
  assert.equal(verifierResult.status, 0, verifierResult.stderr);

  assertPythonFailure(
    "extract",
    fixture.archivePath,
    path.join(tmpRoot, "digest-wrong"),
    { ...expected, archive_sha256: "0".repeat(64) },
    "archive_digest_mismatch"
  );

  const truncated = path.join(tmpRoot, "truncated.tar.gz");
  const archiveBytes = fs.readFileSync(fixture.archivePath);
  fs.writeFileSync(truncated, archiveBytes.subarray(0, archiveBytes.length - 12));
  assertPythonFailure(
    "extract",
    truncated,
    path.join(tmpRoot, "truncated-output"),
    { ...expected, archive_sha256: sha256File(truncated) },
    "archive_truncated"
  );

  const traversal = writeMaliciousArchive("traversal");
  assertPythonFailure(
    "extract",
    traversal,
    path.join(tmpRoot, "traversal-output"),
    { ...expected, archive_sha256: sha256File(traversal) },
    "archive_path_invalid"
  );

  for (const [kind, errorCode] of [
    ["symlink", "archive_link_rejected"],
    ["hardlink", "archive_link_rejected"],
    ["fifo", "archive_special_file_rejected"],
    ["duplicate", "archive_duplicate_path"],
  ]) {
    const archive = writeMaliciousArchive(kind);
    assertPythonFailure(
      "extract",
      archive,
      path.join(tmpRoot, `${kind}-output`),
      { ...expected, archive_sha256: sha256File(archive) },
      errorCode
    );
  }

  assertPythonFailure(
    "extract",
    fixture.archivePath,
    path.join(tmpRoot, "identity-wrong"),
    { ...expected, tag: "v9.9.9" },
    "artifact_identity_mismatch"
  );

  const ingress = path.join(tmpRoot, "ingress");
  fs.mkdirSync(ingress, { mode: 0o700 });
  fs.chmodSync(ingress, 0o700);
  const staging = path.join(ingress, `.staging-${commit}.aaaaaa`);
  assert.equal(runPython("extract", fixture.archivePath, staging, expected).status, 0);
  assert.equal(runPython("commit", staging, ingress, commit, "none").status, 0);
  const installed = path.join(ingress, commit);
  assert.equal(fs.existsSync(installed), true);
  assert.equal(runVerifier(installed, expected).status, 0);
  assert.match(shell, /if \[\[ -L "\$target_path" \|\| -e "\$target_path" \]\]/);
  assert.match(shell, /emit_ready true/);

  const renameFailedIngress = path.join(tmpRoot, "rename-failed-ingress");
  fs.mkdirSync(renameFailedIngress, { mode: 0o700 });
  fs.chmodSync(renameFailedIngress, 0o700);
  const renameFailedStaging = path.join(
    renameFailedIngress,
    `.staging-${commit}.bbbbbb`
  );
  assert.equal(
    runPython("extract", fixture.archivePath, renameFailedStaging, expected).status,
    0
  );
  assertPythonFailure(
    "commit",
    renameFailedStaging,
    renameFailedIngress,
    commit,
    "rename_failed",
    "rename-failed"
  );
  assert.equal(fs.existsSync(renameFailedStaging), true);
  assert.equal(fs.existsSync(path.join(renameFailedIngress, commit)), false);

  const uncertainIngress = path.join(tmpRoot, "rename-uncertain-ingress");
  fs.mkdirSync(uncertainIngress, { mode: 0o700 });
  fs.chmodSync(uncertainIngress, 0o700);
  const uncertainStaging = path.join(uncertainIngress, `.staging-${commit}.cccccc`);
  assert.equal(
    runPython("extract", fixture.archivePath, uncertainStaging, expected).status,
    0
  );
  assertPythonFailure(
    "commit",
    uncertainStaging,
    uncertainIngress,
    commit,
    "commit_uncertain",
    "rename-uncertain"
  );
  assert.equal(fs.existsSync(path.join(uncertainIngress, commit)), true);
  assert.equal(fs.existsSync(uncertainStaging), false);

  const fsyncIngress = path.join(tmpRoot, "fsync-uncertain-ingress");
  fs.mkdirSync(fsyncIngress, { mode: 0o700 });
  fs.chmodSync(fsyncIngress, 0o700);
  const fsyncStaging = path.join(fsyncIngress, `.staging-${commit}.dddddd`);
  assert.equal(
    runPython("extract", fixture.archivePath, fsyncStaging, expected).status,
    0
  );
  assertPythonFailure(
    "commit",
    fsyncStaging,
    fsyncIngress,
    commit,
    "commit_uncertain",
    "post-rename-fsync"
  );
  assert.equal(fs.existsSync(path.join(fsyncIngress, commit)), true);
  assert.equal(fs.existsSync(fsyncStaging), false);
} finally {
  makeWritable(tmpRoot);
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Hermes core ingress fetch boundary test passed.");

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
  writeTarArchive(root, archivePath);
  return {
    root,
    archivePath,
    archiveSha256: sha256File(archivePath),
    manifestSha256: sha256File(path.join(root, "artifact-manifest.json")),
  };
}

function writeFakeCoscli() {
  const fakePath = path.join(tmpRoot, "fake-coscli");
  fs.writeFileSync(
    fakePath,
    `#!/usr/bin/env bash
set -euo pipefail
printf '%s\\n' "$*" >> "$FAKE_COSCLI_ARGS_LOG"
case "\${1:-}" in
  config)
    exit 0
    ;;
  cp)
    [[ "\${2:-}" == "\${EXPECTED_OBJECT_URI}" ]]
    /bin/cp "\${FIXTURE_ARCHIVE}" "\${3}"
    ;;
  *)
    exit 64
    ;;
esac
`,
    "utf8"
  );
  fs.chmodSync(fakePath, 0o755);
  return fakePath;
}

function runFakeCoscli(fakePath, args, extraEnv = {}) {
  return spawnSync(fakePath, args, {
    cwd: repoRoot,
    env: {
      ...process.env,
      FAKE_COSCLI_ARGS_LOG: path.join(tmpRoot, "fake-coscli-args.log"),
      EXPECTED_OBJECT_URI:
        "cos://qintopia-agent-os-hermes-core/qintopia-agent-os/hermes-core/0123456789abcdef0123456789abcdef01234567/hermes-core-0123456789abcdef0123456789abcdef01234567.tar.gz",
      ...extraEnv,
    },
    encoding: "utf8",
  });
}

function runPython(operation, first, second, expectedOrCommit, fault = "none") {
  const args = [extractorPath, operation, first, second];
  if (operation === "extract") {
    args.push(JSON.stringify(expectedOrCommit));
  } else {
    args.push(expectedOrCommit, fault);
  }
  return spawnSync("python3", ["-c", driver, ...args], {
    cwd: repoRoot,
    encoding: "utf8",
  });
}

function assertPythonFailure(
  operation,
  first,
  second,
  expectedOrCommit,
  expectedCode,
  fault = "none"
) {
  const result = runPython(operation, first, second, expectedOrCommit, fault);
  assert.notEqual(result.status, 0, result.stdout);
  assert.equal(result.stdout.trim(), expectedCode, result.stderr);
  assert.doesNotMatch(`${result.stdout}\n${result.stderr}`, /fixture-secret/);
}

function runVerifier(artifactDir, expected) {
  return spawnSync(
    process.execPath,
    [
      verifierPath,
      "--artifact-dir",
      artifactDir,
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
    ],
    { cwd: repoRoot, encoding: "utf8" }
  );
}

function writeMaliciousArchive(kind) {
  const archivePath = path.join(tmpRoot, `${kind}.tar.gz`);
  const code = String.raw`
import io
import sys
import tarfile

archive_path, kind = sys.argv[1:3]
with tarfile.open(archive_path, "w:gz") as archive:
    def add_directory(name):
        info = tarfile.TarInfo(name)
        info.type = tarfile.DIRTYPE
        info.mode = 0o755
        archive.addfile(info)

    def add_file(name, value=b"x"):
        info = tarfile.TarInfo(name)
        info.size = len(value)
        archive.addfile(info, io.BytesIO(value))

    for name in ("artifact-manifest.json", "build-receipt.json", "update-receipt.json", "validation-summary.json", "SHA256SUMS"):
        add_file(name, b"{}\n")
    add_directory("core")
    if kind == "traversal":
        add_file("../escape", b"escape")
    elif kind == "symlink":
        info = tarfile.TarInfo("core/link")
        info.type = tarfile.SYMTYPE
        info.linkname = "/etc/passwd"
        archive.addfile(info)
    elif kind == "hardlink":
        info = tarfile.TarInfo("core/link")
        info.type = tarfile.LNKTYPE
        info.linkname = "core/source"
        archive.addfile(info)
    elif kind == "fifo":
        info = tarfile.TarInfo("core/fifo")
        info.type = tarfile.FIFOTYPE
        archive.addfile(info)
    elif kind == "duplicate":
        add_directory("core")
`;
  const result = spawnSync("python3", ["-c", code, archivePath, kind], {
    cwd: repoRoot,
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr);
  return archivePath;
}

function writeTarArchive(root, archivePath) {
  const relativeEntries = listEntries(root).sort((left, right) => {
    const leftDepth = left.split("/").length;
    const rightDepth = right.split("/").length;
    return leftDepth - rightDepth || left.localeCompare(right);
  });
  const code = String.raw`
import sys
import tarfile

archive_path, root, *entries = sys.argv[1:]
with tarfile.open(archive_path, "w:gz") as archive:
    for relative in entries:
        archive.add(f"{root}/{relative}", arcname=relative, recursive=False)
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

function assertNormalizedArtifact(root, expected) {
  assert.equal(fs.lstatSync(root).mode & 0o7777, 0o555);
  for (const name of [
    "artifact-manifest.json",
    "build-receipt.json",
    "update-receipt.json",
    "validation-summary.json",
    "SHA256SUMS",
  ]) {
    const metadata = fs.lstatSync(path.join(root, name));
    assert.equal(metadata.uid, process.getuid?.() ?? metadata.uid);
    assert.equal(metadata.nlink, 1);
    assert.equal(metadata.mode & 0o7777, 0o444);
  }
  assert.equal(fs.lstatSync(path.join(root, "core")).mode & 0o7777, 0o555);
  assert.equal(
    fs.lstatSync(path.join(root, "core", "hermes_cli")).mode & 0o7777,
    0o555
  );
  assert.equal(
    fs.lstatSync(path.join(root, "core", "hermes_cli", "main.py")).mode & 0o7777,
    0o444
  );
  assert.equal(
    fs.lstatSync(path.join(root, "core", "pyproject.toml")).mode & 0o7777,
    0o444
  );
  assert.equal(expected.commit, commit);
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
