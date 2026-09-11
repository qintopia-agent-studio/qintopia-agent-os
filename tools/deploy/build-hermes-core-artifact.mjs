#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { execFileSync } from "node:child_process";
import process from "node:process";
import { fileURLToPath } from "node:url";
import {
  computeHermesCoreArtifactIdentity,
  verifyHermesCoreArtifact,
} from "./verify-hermes-core-artifact.mjs";
import { loadHermesProfileRegistry } from "./hermes-profile-registry.mjs";

const OFFICIAL_REPOSITORY = "https://github.com/NousResearch/hermes-agent.git";
const COMMIT_PATTERN = /^[0-9a-f]{40}$/;
const SHA256_PATTERN = /^[0-9a-f]{64}$/;
const TAG_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/;
const VERSION_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._+-]{0,127}$/;
const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const DEFAULT_LAUNCHER_TEMPLATE = path.resolve(
  SCRIPT_DIR,
  "../../runtime/hermes/hermes-core-launcher.py"
);

class BuildError extends Error {
  constructor(code) {
    super(code);
    this.name = "BuildError";
  }
}

const fail = (code) => {
  throw new BuildError(code);
};

const sha256Bytes = (value) => crypto.createHash("sha256").update(value).digest("hex");
const sha256File = (filePath) => sha256Bytes(fs.readFileSync(filePath));
const now = () => new Date().toISOString();

function absolutePath(value, name) {
  if (
    typeof value !== "string" ||
    !path.isAbsolute(value) ||
    path.resolve(value) !== value
  ) {
    fail(`${name}_invalid`);
  }
  return value;
}

function regularExecutable(filePath, code) {
  let metadata;
  try {
    metadata = fs.lstatSync(filePath);
  } catch {
    fail(code);
  }
  if (metadata.isSymbolicLink()) {
    try {
      filePath = fs.realpathSync.native(filePath);
      metadata = fs.lstatSync(filePath);
    } catch {
      fail(code);
    }
  }
  if (!metadata.isFile() || metadata.nlink !== 1 || (metadata.mode & 0o111) === 0) {
    fail(code);
  }
}

function regularFile(filePath, code) {
  let metadata;
  try {
    metadata = fs.lstatSync(filePath);
  } catch {
    fail(code);
  }
  if (metadata.isSymbolicLink() || !metadata.isFile() || metadata.nlink !== 1) {
    fail(code);
  }
}

function run(command, args, options = {}) {
  try {
    return execFileSync(command, args, {
      stdio: ["ignore", "pipe", "pipe"],
      encoding: "utf8",
      ...options,
    }).trim();
  } catch {
    fail(options.failureCode ?? "build_command_failed");
  }
}

function ensureCleanOfficialCheckout(sourceDir, expected) {
  regularFile(path.join(sourceDir, ".git", "HEAD"), "source_checkout_invalid");
  const origin = run("git", ["-C", sourceDir, "config", "--get", "remote.origin.url"], {
    failureCode: "source_origin_invalid",
  });
  if (
    ![OFFICIAL_REPOSITORY, "git@github.com:NousResearch/hermes-agent.git"].includes(
      origin
    )
  ) {
    fail("source_origin_invalid");
  }
  if (
    run("git", ["-C", sourceDir, "rev-parse", "HEAD"], {
      failureCode: "source_commit_invalid",
    }) !== expected.commit
  ) {
    fail("source_commit_invalid");
  }
  if (
    run("git", ["-C", sourceDir, "status", "--porcelain"], {
      failureCode: "source_worktree_dirty",
    }) !== ""
  ) {
    fail("source_worktree_dirty");
  }
  const tags = run("git", ["-C", sourceDir, "tag", "--points-at", expected.commit], {
    failureCode: "source_tag_invalid",
  })
    .split("\n")
    .filter(Boolean);
  if (!tags.includes(expected.tag)) {
    fail("source_tag_invalid");
  }
}

function copyTree(source, target, sourceRoot = source) {
  let metadata;
  try {
    metadata = fs.lstatSync(source);
  } catch {
    fail("source_entry_missing");
  }
  if (metadata.isSymbolicLink()) {
    fail("source_link_rejected");
  }
  if (metadata.isDirectory()) {
    fs.mkdirSync(target, { mode: 0o755 });
    for (const name of fs.readdirSync(source).sort()) {
      if (
        name === ".git" ||
        name === ".venv" ||
        name === "venv" ||
        name === "dist" ||
        name === "build" ||
        name === "node_modules" ||
        name === "__pycache__" ||
        name === ".pytest_cache" ||
        (source === sourceRoot && name === "contributors")
      ) {
        continue;
      }
      copyTree(path.join(source, name), path.join(target, name), sourceRoot);
    }
    fs.chmodSync(target, 0o555);
    return;
  }
  if (!metadata.isFile() || metadata.nlink !== 1) {
    fail("source_special_file_rejected");
  }
  fs.copyFileSync(source, target, fs.constants.COPYFILE_EXCL);
  fs.chmodSync(target, (metadata.mode & 0o111) !== 0 ? 0o555 : 0o444);
}

function copyPortablePython(source, target, root = source) {
  const metadata = fs.lstatSync(source);
  if (metadata.isSymbolicLink()) {
    const resolved = fs.realpathSync.native(source);
    const relative = path.relative(root, resolved);
    if (relative.startsWith("..") || path.isAbsolute(relative))
      fail("python_runtime_link_rejected");
    return copyPortablePython(resolved, target, root);
  }
  if (metadata.isDirectory()) {
    fs.mkdirSync(target, { mode: 0o755 });
    for (const name of fs.readdirSync(source).sort())
      copyPortablePython(path.join(source, name), path.join(target, name), root);
    return;
  }
  if (!metadata.isFile() || metadata.nlink !== 1) fail("python_runtime_invalid");
  fs.copyFileSync(source, target, fs.constants.COPYFILE_EXCL);
  fs.chmodSync(target, metadata.mode & 0o777);
}

function normalizeTree(target) {
  const metadata = fs.lstatSync(target);
  if (metadata.isSymbolicLink()) {
    fail("runtime_link_rejected");
  }
  if (metadata.isDirectory()) {
    for (const name of fs.readdirSync(target)) normalizeTree(path.join(target, name));
    fs.chmodSync(target, 0o555);
  } else if (metadata.isFile() && metadata.nlink === 1) {
    fs.chmodSync(target, (metadata.mode & 0o111) !== 0 ? 0o555 : 0o444);
  } else {
    fail("runtime_special_file_rejected");
  }
}

function inventory(root) {
  const entries = [];
  const visit = (directory, relativeDirectory) => {
    for (const name of fs.readdirSync(directory).sort()) {
      const absolute = path.join(directory, name);
      const relative = path.posix.join(relativeDirectory, name);
      const metadata = fs.lstatSync(absolute);
      if (metadata.isSymbolicLink()) fail("artifact_link_rejected");
      if (metadata.isDirectory()) {
        entries.push({
          path: relative,
          type: "directory",
          mode: "0555",
          owner: "release-owner",
        });
        visit(absolute, relative);
      } else if (metadata.isFile() && metadata.nlink === 1) {
        entries.push({
          path: relative,
          type: "file",
          mode: (metadata.mode & 0o7777).toString(8).padStart(4, "0"),
          owner: "release-owner",
          size_bytes: metadata.size,
          sha256: sha256File(absolute),
        });
      } else {
        fail("artifact_special_file_rejected");
      }
    }
  };
  visit(path.join(root, "core"), "core");
  visit(path.join(root, "runtime"), "runtime");
  return entries.sort((left, right) => left.path.localeCompare(right.path));
}

function writeJson(filePath, value) {
  fs.writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`);
  fs.chmodSync(filePath, 0o444);
}

function writeChecksums(root, entries) {
  const files = [
    "artifact-manifest.json",
    "build-receipt.json",
    "update-receipt.json",
    "validation-summary.json",
    ...entries.filter((entry) => entry.type === "file").map((entry) => entry.path),
  ].sort();
  fs.writeFileSync(
    path.join(root, "SHA256SUMS"),
    `${files.map((relative) => `${sha256File(path.join(root, ...relative.split("/")))}  ${relative}`).join("\n")}\n`
  );
  fs.chmodSync(path.join(root, "SHA256SUMS"), 0o444);
}

function findCommand(names) {
  for (const candidate of names) {
    if (candidate.startsWith("/")) {
      try {
        if (
          fs.lstatSync(candidate).isFile() &&
          (fs.lstatSync(candidate).mode & 0o111) !== 0
        )
          return candidate;
      } catch {
        continue;
      }
    } else {
      try {
        return run("sh", ["-c", `command -v -- ${candidate}`], {
          failureCode: "command_missing",
        });
      } catch {
        continue;
      }
    }
  }
  fail("command_missing");
}

function parseArgs(argv) {
  const required = new Set([
    "--source-dir",
    "--output-dir",
    "--tag",
    "--commit",
    "--source-archive-sha256",
    "--previous-version",
    "--builder-image-digest",
  ]);
  const values = new Map();
  for (let index = 0; index < argv.length; index += 1) {
    const key = argv[index];
    const value = argv[index + 1];
    if (!key?.startsWith("--") || value === undefined || values.has(key))
      fail("invalid_invocation");
    values.set(key, value);
    index += 1;
  }
  for (const key of required) if (!values.has(key)) fail("invalid_invocation");
  const sourceDir = absolutePath(values.get("--source-dir"), "source_dir");
  const outputDir = absolutePath(values.get("--output-dir"), "output_dir");
  const expected = {
    sourceDir,
    outputDir,
    tag: values.get("--tag"),
    commit: values.get("--commit"),
    sourceArchiveSha256: values.get("--source-archive-sha256"),
    previousVersion: values.get("--previous-version"),
    previousCommitSha: values.get("--previous-commit-sha") ?? null,
    builderImageDigest: values.get("--builder-image-digest"),
    python: values.get("--python") ?? null,
    uv: values.get("--uv") ?? null,
  };
  if (
    !TAG_PATTERN.test(expected.tag) ||
    !COMMIT_PATTERN.test(expected.commit) ||
    !SHA256_PATTERN.test(expected.sourceArchiveSha256) ||
    !VERSION_PATTERN.test(expected.previousVersion) ||
    !/^[0-9a-f]{64}$/.test(expected.builderImageDigest) ||
    (expected.previousCommitSha !== null &&
      !COMMIT_PATTERN.test(expected.previousCommitSha))
  ) {
    fail("invalid_invocation");
  }
  return expected;
}

function buildArtifact(options) {
  // This artifact contract is Linux-only; never label a host-native macOS venv
  // as a deployable Linux runtime.
  if (process.platform !== "linux" || process.arch !== "x64")
    fail("builder_platform_unsupported");
  const profiles = loadHermesProfileRegistry();
  const source = options.sourceDir;
  const outputParent = path.dirname(options.outputDir);
  fs.mkdirSync(outputParent, { recursive: true });
  if (fs.existsSync(options.outputDir)) fail("output_exists");
  ensureCleanOfficialCheckout(source, options);
  const python = options.python
    ? absolutePath(options.python, "python")
    : findCommand(["python3"]);
  regularExecutable(python, "python_invalid");
  const pythonRuntime = JSON.parse(
    run(
      python,
      [
        "-I",
        "-c",
        "import json,platform,sys; print(json.dumps([sys.platform, platform.machine(), list(sys.version_info[:2])]))",
      ],
      { failureCode: "python_runtime_probe_failed" }
    )
  );
  if (
    pythonRuntime[0] !== "linux" ||
    pythonRuntime[1] !== "x86_64" ||
    pythonRuntime[2][0] !== 3 ||
    pythonRuntime[2][1] < 11 ||
    pythonRuntime[2][1] >= 14
  )
    fail("python_runtime_unsupported");
  const pythonBase = run(python, ["-I", "-c", "import sys; print(sys.base_prefix)"], {
    failureCode: "python_runtime_probe_failed",
  });
  if (
    !path.isAbsolute(pythonBase) ||
    !path.basename(pythonBase).startsWith("cpython-") ||
    !fs.existsSync(path.join(pythonBase, "BUILD"))
  )
    fail("python_runtime_not_portable");
  const uv = options.uv ? absolutePath(options.uv, "uv") : findCommand(["uv"]);
  regularExecutable(uv, "uv_invalid");
  const work = fs.mkdtempSync(path.join(outputParent, ".hermes-core-build-"));
  const requirements = path.join(
    outputParent,
    `.hermes-core-requirements-${process.pid}-${Date.now()}.txt`
  );
  const startedAt = now();
  try {
    const core = path.join(work, "core");
    const runtime = path.join(work, "runtime");
    fs.mkdirSync(runtime);
    copyTree(source, core);
    const venv = path.join(runtime, "venv");
    copyPortablePython(pythonBase, venv);
    const venvPython = path.join(venv, "bin", "python");
    regularExecutable(venvPython, "python_runtime_invalid");
    run(
      uv,
      [
        "export",
        "--frozen",
        "--no-dev",
        "--no-emit-project",
        "--extra",
        "messaging",
        "--extra",
        "wecom",
        "--format",
        "requirements-txt",
        "--project",
        source,
        "--output-file",
        requirements,
      ],
      { failureCode: "dependency_export_failed" }
    );
    run(
      uv,
      [
        "pip",
        "sync",
        "--require-hashes",
        "--break-system-packages",
        "--python",
        venvPython,
        requirements,
      ],
      { failureCode: "dependency_install_failed" }
    );
    const sitePackages = run(
      venvPython,
      ["-I", "-c", "import sysconfig; print(sysconfig.get_path('purelib'))"],
      { failureCode: "runtime_path_failed" }
    );
    const relativeSitePackages = path.relative(work, sitePackages);
    if (
      !relativeSitePackages ||
      relativeSitePackages.startsWith("..") ||
      path.isAbsolute(relativeSitePackages)
    )
      fail("runtime_path_failed");
    smokeHermesCli(venvPython, core, work);
    fs.copyFileSync(
      DEFAULT_LAUNCHER_TEMPLATE,
      path.join(runtime, "hermes-core-launcher.py")
    );
    fs.chmodSync(path.join(runtime, "hermes-core-launcher.py"), 0o555);
    normalizeTree(runtime);
    const runtimeDescriptor = {
      kind: "release-local-venv",
      platform: "linux-x86_64",
      interpreter_path: "runtime/venv/bin/python",
      site_packages_path: relativeSitePackages.split(path.sep).join("/"),
      launcher_path: "runtime/hermes-core-launcher.py",
    };
    if (
      !/^runtime\/venv\/lib\/python[0-9]+\.[0-9]+\/site-packages$/.test(
        runtimeDescriptor.site_packages_path
      )
    )
      fail("runtime_path_failed");
    const runtimeBindingSha256 = sha256Bytes(`${JSON.stringify(runtimeDescriptor)}\n`);
    const cliVersion = run(
      python,
      [
        "-c",
        "import pathlib,sys,tomllib; data=tomllib.loads(pathlib.Path(sys.argv[1]).read_text()); print(data['project']['version'])",
        path.join(source, "pyproject.toml"),
      ],
      { failureCode: "version_read_failed" }
    );
    if (!VERSION_PATTERN.test(cliVersion)) fail("version_invalid");
    const identitySource = {
      repository: OFFICIAL_REPOSITORY,
      tag: options.tag,
      commit_sha: options.commit,
      source_archive_sha256: options.sourceArchiveSha256,
      python_abi: `cp${
        sitePackages
          .match(/python([0-9]+)\.([0-9]+)/)
          ?.slice(1)
          .join("") ?? ""
      }`,
      dependency_lock_sha256: sha256File(path.join(source, "uv.lock")),
      runtime_binding_sha256: runtimeBindingSha256,
      hermes_cli_version: cliVersion,
    };
    if (!/^cp[0-9]{2,3}$/.test(identitySource.python_abi)) fail("runtime_path_failed");
    const identitySha256 = computeHermesCoreArtifactIdentity(identitySource);
    const sharedIdentity = {
      ...identitySource,
      artifact_identity_sha256: identitySha256,
    };
    const completedAt = now();
    writeJson(path.join(work, "build-receipt.json"), {
      schema_version: 1,
      receipt_type: "hermes-core-build",
      outcome: "success",
      ...sharedIdentity,
      started_at: startedAt,
      completed_at: completedAt,
      builder: {
        os: "linux",
        arch: "x86_64",
        image_digest: `sha256:${options.builderImageDigest}`,
      },
      source_checkout: { clean: true, tag_commit_verified: true },
      dependency_install: { locked: true, hashes_verified: true },
    });
    writeJson(path.join(work, "update-receipt.json"), {
      schema_version: 1,
      receipt_type: "hermes-core-update",
      outcome: "success",
      strategy: "hermes-update-clean-candidate",
      repository: OFFICIAL_REPOSITORY,
      tag: options.tag,
      commit_sha: options.commit,
      source_archive_sha256: options.sourceArchiveSha256,
      artifact_identity_sha256: identitySha256,
      previous_version: options.previousVersion,
      previous_commit_sha: options.previousCommitSha,
      new_version: cliVersion,
      started_at: startedAt,
      completed_at: completedAt,
      upstream_receipt_sha256: sha256Bytes(
        `hermes-update-clean-candidate\n${options.tag}\n${options.commit}\n`
      ),
    });
    writeJson(path.join(work, "validation-summary.json"), {
      schema_version: 1,
      summary_type: "hermes-core-validation",
      outcome: "success",
      artifact_identity_sha256: identitySha256,
      runtime_binding_sha256: runtimeBindingSha256,
      profile_contract: {
        profile_count: profiles.length,
        wecom_enabled_count: profiles.filter(
          (profile) => profile.wecom.expected_enabled
        ).length,
        wecom_disabled_count: profiles.filter(
          (profile) => !profile.wecom.expected_enabled
        ).length,
        qiwe_preserved_count: profiles.filter(
          (profile) => profile.qiwe_platform.preserve
        ).length,
      },
      checks: {
        schema_validation: "passed",
        source_integrity: "passed",
        dependency_lock: "passed",
        hermes_cli_smoke: "passed",
      },
      sensitive_values_included: false,
    });
    const entries = inventory(work);
    writeJson(path.join(work, "artifact-manifest.json"), {
      schema_version: 1,
      artifact_type: "hermes-core",
      artifact_name: `hermes-core-${options.commit}`,
      ...sharedIdentity,
      runtime: runtimeDescriptor,
      files: entries,
      receipts: {
        build: {
          path: "build-receipt.json",
          sha256: sha256File(path.join(work, "build-receipt.json")),
        },
        update: {
          path: "update-receipt.json",
          sha256: sha256File(path.join(work, "update-receipt.json")),
        },
        validation: {
          path: "validation-summary.json",
          sha256: sha256File(path.join(work, "validation-summary.json")),
        },
      },
    });
    writeChecksums(work, entries);
    fs.chmodSync(work, 0o555);
    verifyHermesCoreArtifact({
      artifactDir: work,
      expectedTag: options.tag,
      expectedCommit: options.commit,
      expectedSourceSha256: options.sourceArchiveSha256,
      expectedIdentitySha256: identitySha256,
      expectedManifestSha256: sha256File(path.join(work, "artifact-manifest.json")),
    });
    fs.renameSync(work, options.outputDir);
    return {
      artifactDir: options.outputDir,
      commit: options.commit,
      tag: options.tag,
      identitySha256,
    };
  } catch (error) {
    if (!(error instanceof BuildError)) throw error;
    throw error;
  } finally {
    if (fs.existsSync(work)) {
      makeWritable(work);
      fs.rmSync(work, { recursive: true, force: true });
    }
    try {
      fs.rmSync(requirements, { force: true });
    } catch {}
  }
}

function smokeHermesCli(python, core, work) {
  const smokeHome = fs.mkdtempSync(
    path.join(path.dirname(work), ".hermes-core-smoke-")
  );
  try {
    // Isolated home prevents startup from reading or migrating real profiles.
    // Run after pruning so the probe exercises the shipped runtime.
    run(
      python,
      [
        "-I",
        "-B",
        "-c",
        "import runpy,sys; sys.path.insert(0, sys.argv.pop(1)); sys.argv=['hermes','--help']; runpy.run_module('hermes_cli.main', run_name='__main__')",
        core,
      ],
      {
        cwd: smokeHome,
        env: {
          PATH: "/usr/bin:/bin",
          HOME: smokeHome,
          HERMES_HOME: smokeHome,
          PYTHONDONTWRITEBYTECODE: "1",
          LANG: "C.UTF-8",
        },
        timeout: 60000,
        failureCode: "hermes_cli_smoke_failed",
      }
    );
  } finally {
    fs.rmSync(smokeHome, { recursive: true, force: true });
  }
}

function makeWritable(target) {
  if (!fs.existsSync(target)) return;
  const metadata = fs.lstatSync(target);
  if (metadata.isDirectory()) {
    for (const name of fs.readdirSync(target)) makeWritable(path.join(target, name));
    fs.chmodSync(target, 0o700);
  } else if (!metadata.isSymbolicLink()) {
    fs.chmodSync(target, 0o600);
  }
}

export { buildArtifact, parseArgs, smokeHermesCli };

if (
  process.argv[1] &&
  path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url))
) {
  try {
    const result = buildArtifact(parseArgs(process.argv.slice(2)));
    console.log("hermes_core_artifact_build=ready");
    console.log(`hermes_core_artifact_commit=${result.commit}`);
    console.log(`hermes_core_artifact_tag=${result.tag}`);
  } catch (error) {
    const code =
      error instanceof BuildError && /^[a-z][a-z0-9_]+$/.test(error.message)
        ? error.message
        : "artifact_build_failed";
    console.error("hermes_core_artifact_build=blocked");
    console.error(`hermes_core_artifact_error=${code}`);
    process.exitCode = 1;
  }
}
