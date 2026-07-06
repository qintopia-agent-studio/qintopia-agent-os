#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { execFileSync } from "node:child_process";

const repoRoot = process.cwd();
const bundleName = "qintopia-agent-os-deploy-bundle";
const outputRoot = path.join(repoRoot, "dist", "deploy-bundles");
const bundleDir = path.join(outputRoot, bundleName);
const payloadDir = path.join(bundleDir, "payload");
const archiveName = `${bundleName}.tar.gz`;
const archivePath = path.join(bundleDir, archiveName);
const manifestPath = path.join(bundleDir, "artifact-manifest.json");
const checksumPath = path.join(bundleDir, "SHA256SUMS");

const sourceFiles = [
  "deploy/sidecar/scripts/hermes/qintopia-context-mcp",
  "deploy/sidecar/scripts/render-systemd-units.sh",
  "mcp/qintopia-collab/bin/qintopia-collab-mcp",
  "deploy/sidecar/docs/m9f-legacy-reference-removal.md",
  "deploy/sidecar/docs/systemd-cutover-plan.md",
  "docs/operations/m9-server-cutover-runbook.md",
  "docs/operations/release-current-model.md",
  "skills/qintopia-tools/manifest.yaml",
  "skills/qintopia-tools/README.md",
  "skills/qintopia-tools/docs/source-snapshot.md",
  "skills/qintopia-weather/manifest.yaml",
  "skills/qintopia-weather/README.md",
  "skills/qintopia-weather/__init__.py",
  "skills/qintopia-weather/plugin.yaml",
  "mcp/weather-provider/manifest.yaml",
  "mcp/weather-provider/README.md",
  "skills/qiwe/manifest.yaml",
  "skills/qiwe/README.md",
  "skills/qiwe/__init__.py",
  "skills/qiwe/adapter.py",
  "skills/qiwe/nats_capture.py",
  "skills/qiwe/passive_pipeline.py",
  "skills/qiwe/plugin.yaml",
  "skills/qiwe/qiwe_events.py",
  "skills/feishu-base/manifest.yaml",
  "skills/feishu-base/README.md",
  "skills/feishu-base/__init__.py",
  "skills/feishu-base/plugin.yaml",
];
const sourceDirs = [
  "runtime/postgres/migrations",
  "skills/qintopia-tools/variants",
  "skills/qintopia-weather/tests",
  "skills/qiwe/docs",
  "skills/qiwe/scripts",
  "skills/qiwe/solitaire",
  "skills/qiwe/tests",
  "skills/feishu-base/docs",
  "skills/feishu-base/tests",
];
const sourceDirExcludes = [
  /(^|\/)__pycache__(\/|$)/,
  /\.pyc$/,
  /(^|\/)\.DS_Store$/,
  /\.bak/,
];

const run = (command, args, options = {}) =>
  (
    execFileSync(command, args, {
      cwd: repoRoot,
      encoding: "utf8",
      stdio: options.stdio ?? ["ignore", "pipe", "pipe"],
    }) ?? ""
  ).trim();

const sha256File = (filePath) => {
  const hash = crypto.createHash("sha256");
  hash.update(fs.readFileSync(filePath));
  return hash.digest("hex");
};

const gitOutput = (args, fallback = "") => {
  try {
    return run("git", args);
  } catch {
    return fallback;
  }
};

const toolOutput = (command, args, fallback = "") => {
  try {
    return run(command, args);
  } catch {
    return fallback;
  }
};

const copyFile = (relativePath) => {
  const sourcePath = path.join(repoRoot, relativePath);
  if (!fs.existsSync(sourcePath)) {
    throw new Error(`deploy bundle source file not found: ${relativePath}`);
  }

  const targetPath = path.join(payloadDir, relativePath);
  fs.mkdirSync(path.dirname(targetPath), { recursive: true });
  fs.copyFileSync(sourcePath, targetPath);

  const mode = fs.statSync(sourcePath).mode & 0o777;
  fs.chmodSync(targetPath, mode);

  return {
    path: `payload/${relativePath}`,
    source_path: relativePath,
    sha256: sha256File(targetPath),
    size_bytes: fs.statSync(targetPath).size,
    mode: mode.toString(8).padStart(4, "0"),
  };
};

const collectDirectoryFiles = (relativeDir) => {
  const absoluteDir = path.join(repoRoot, relativeDir);
  if (!fs.existsSync(absoluteDir)) {
    throw new Error(`deploy bundle source directory not found: ${relativeDir}`);
  }

  const discovered = [];
  const walk = (dir) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const absolutePath = path.join(dir, entry.name);
      const relativePath = path.relative(repoRoot, absolutePath);
      if (sourceDirExcludes.some((pattern) => pattern.test(relativePath))) {
        continue;
      }
      if (entry.isDirectory()) {
        walk(absolutePath);
      } else if (entry.isFile()) {
        discovered.push(relativePath);
      }
    }
  };
  walk(absoluteDir);
  return discovered.sort();
};

const buildStartedAt = new Date().toISOString();
const commitSha = process.env.GITHUB_SHA || gitOutput(["rev-parse", "HEAD"], "unknown");
const branch =
  process.env.GITHUB_REF_NAME || gitOutput(["branch", "--show-current"], "unknown");

fs.rmSync(bundleDir, { recursive: true, force: true });
fs.mkdirSync(payloadDir, { recursive: true });

const files = [...sourceFiles, ...sourceDirs.flatMap(collectDirectoryFiles)].map(
  copyFile
);

run("tar", ["-C", bundleDir, "-czf", archivePath, "payload"]);
const archiveSha256 = sha256File(archivePath);
files.push({
  path: archiveName,
  sha256: archiveSha256,
  size_bytes: fs.statSync(archivePath).size,
  content: ["payload/"],
  compression: "gzip",
  mode: "0644",
});

const manifest = {
  schema_version: 1,
  artifact_name: bundleName,
  package_name: "qintopia-agent-os-deploy",
  target: "server-operator-files",
  repository: process.env.GITHUB_REPOSITORY || "local",
  commit_sha: commitSha,
  branch,
  run_id: process.env.GITHUB_RUN_ID || null,
  run_attempt: process.env.GITHUB_RUN_ATTEMPT || null,
  build_started_at: buildStartedAt,
  build_finished_at: new Date().toISOString(),
  runner: {
    os: process.env.RUNNER_OS || os.platform(),
    arch: process.env.RUNNER_ARCH || os.arch(),
  },
  toolchain: {
    node: process.version,
    git: toolOutput("git", ["--version"]),
  },
  files,
  validation: {
    required_workflow_jobs: ["check", "deploy-bundle-artifact"],
    paired_runtime_artifact:
      "M9-F must also name an approved sidecar runtime artifact SHA; deploy bundle does not contain the runtime binary.",
    server_verification: [
      "download only from Tencent COS or GitHub Actions artifact for the approved deploy bundle commit SHA",
      "sha256sum -c SHA256SUMS",
      "verify payload wrapper does not reference /home/ubuntu/qintopia-msg-sidecar",
      "render systemd units from payload/render-systemd-units.sh for the approved runtime artifact SHA",
      "use payload/runtime/postgres/migrations as QINTOPIA_SIDECAR_MIGRATIONS_DIR",
      "verify skills/qintopia-tools variants are present before any profile plugin repoint",
      "verify skills/qintopia-weather is present before any qintopia-tools repoint that delegates weather lookup",
      "verify mcp/weather-provider is present before enabling provider-level weather adapters",
      "verify skills/qiwe is present before any Erhua qiwe-platform plugin repoint",
      "verify skills/feishu-base is present before any Huabaosi qintopia-base-read plugin repoint",
    ],
  },
};

fs.writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
fs.writeFileSync(checksumPath, `${archiveSha256}  ${archiveName}\n`);

console.log(`Built ${bundleName}`);
console.log(`Manifest: ${path.relative(repoRoot, manifestPath)}`);
console.log(`Checksum: ${path.relative(repoRoot, checksumPath)}`);
