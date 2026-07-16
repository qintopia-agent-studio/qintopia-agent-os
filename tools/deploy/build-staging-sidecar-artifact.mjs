#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { execFileSync } from "node:child_process";
import {
  resolveApprovedTarget,
  resolveContainedArtifactDir,
} from "./sidecar-artifact-build-boundary.mjs";

const repoRoot = process.cwd();
const packageName = "qintopia-message-sidecar";
const binaryName = "qintopia-message-sidecar";
const targetTriple = resolveApprovedTarget();
const outputRoot = path.join(repoRoot, "dist", "sidecar-artifacts");
const artifactName = `${binaryName}-staging-${targetTriple}`;
const cargoFeatures = ["huabaosi-staging-adapter", "qiwe-staging-adapter"];
const artifactDir = resolveContainedArtifactDir(outputRoot, artifactName);
const binaryPath = path.join(
  repoRoot,
  "runtime",
  "sidecar",
  "target",
  "release",
  binaryName
);
const stagedBinaryPath = path.join(artifactDir, binaryName);
const bundleName = `${binaryName}.tar.gz`;
const bundlePath = path.join(artifactDir, bundleName);
const manifestPath = path.join(artifactDir, "artifact-manifest.json");
const checksumPath = path.join(artifactDir, "SHA256SUMS");

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

const ensureFile = (filePath, label) => {
  if (!fs.existsSync(filePath)) {
    throw new Error(`${label} not found at ${filePath}`);
  }
};

const worktreeStatus = gitOutput(["status", "--porcelain"], "unknown");
if (worktreeStatus) {
  throw new Error(
    "refusing to build a staging artifact from a dirty or unreadable git worktree"
  );
}

const buildStartedAt = new Date().toISOString();

run(
  "cargo",
  [
    "build",
    "--release",
    "--locked",
    "--manifest-path",
    "runtime/sidecar/Cargo.toml",
    "--no-default-features",
    "--features",
    cargoFeatures.join(","),
  ],
  { stdio: "inherit" }
);
ensureFile(binaryPath, "release binary");

fs.rmSync(artifactDir, { recursive: true, force: true });
fs.mkdirSync(artifactDir, { recursive: true });
fs.copyFileSync(binaryPath, stagedBinaryPath);
fs.chmodSync(stagedBinaryPath, 0o755);
run("tar", ["-C", artifactDir, "-czf", bundlePath, binaryName]);

const binarySha256 = sha256File(stagedBinaryPath);
const bundleSha256 = sha256File(bundlePath);
const manifest = {
  schema_version: 1,
  artifact_name: artifactName,
  package_name: packageName,
  binary_name: binaryName,
  target: targetTriple,
  repository: process.env.GITHUB_REPOSITORY || "local",
  commit_sha: process.env.GITHUB_SHA || gitOutput(["rev-parse", "HEAD"], "unknown"),
  branch:
    process.env.GITHUB_REF_NAME || gitOutput(["branch", "--show-current"], "unknown"),
  run_id: process.env.GITHUB_RUN_ID || null,
  run_attempt: process.env.GITHUB_RUN_ATTEMPT || null,
  build_started_at: buildStartedAt,
  build_finished_at: new Date().toISOString(),
  runner: {
    os: process.env.RUNNER_OS || os.platform(),
    arch: process.env.RUNNER_ARCH || os.arch(),
  },
  toolchain: {
    rustc: toolOutput("rustc", ["--version"]),
    cargo: toolOutput("cargo", ["--version"]),
  },
  files: [
    {
      path: binaryName,
      sha256: binarySha256,
      size_bytes: fs.statSync(stagedBinaryPath).size,
      mode: "0755",
    },
    {
      path: bundleName,
      sha256: bundleSha256,
      size_bytes: fs.statSync(bundlePath).size,
      content: [binaryName],
      compression: "gzip",
      mode: "0644",
    },
  ],
  validation: {
    cargo_features: cargoFeatures,
    staging_only: true,
    production_eligible: false,
    required_workflow_jobs: ["check", "staging-sidecar-artifact"],
    server_verification: [
      "download only from a successful CI workflow run for the approved staging commit SHA",
      "sha256sum -c SHA256SUMS",
      "./qintopia-message-sidecar check",
      "install only under /home/ubuntu/qintopia-agent-os-staging-releases/<approved 40-hex sha>",
    ],
  },
};

fs.writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
fs.writeFileSync(checksumPath, `${binarySha256}  ${binaryName}\n`);

console.log(`Built ${artifactName}`);
console.log(`Manifest: ${path.relative(repoRoot, manifestPath)}`);
console.log(`Checksum: ${path.relative(repoRoot, checksumPath)}`);
