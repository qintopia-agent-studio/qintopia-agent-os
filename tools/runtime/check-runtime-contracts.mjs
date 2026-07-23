#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import YAML from "yaml";

import {
  extractPatchEntries,
  patchEntriesMatchAllowedPaths,
} from "./hermes-patch-paths.mjs";

const repoRoot = process.cwd();
const errors = [];

const packages = {
  "runtime/hermes": ["SOUL.md", ".env", "sessions", "logs", "cache"],
  "runtime/systemd": ["systemd", "current", "secrets"],
  "runtime/nginx": ["ingress", "TLS", "secrets"],
};

const exists = (relativePath) => fs.existsSync(path.join(repoRoot, relativePath));
const readText = (relativePath) =>
  fs.readFileSync(path.join(repoRoot, relativePath), "utf8");
const addError = (message) => errors.push(message);

const packageJson = JSON.parse(readText("package.json"));
if (
  !packageJson.scripts?.["runtime:contracts:check"]?.includes(
    "test_validate_hermes_python.py"
  )
) {
  addError(
    "package.json: runtime:contracts:check must run the Hermes Python validator tests"
  );
}

for (const [packagePath, requiredFragments] of Object.entries(packages)) {
  const readmePath = `${packagePath}/README.md`;
  const manifestPath = `${packagePath}/manifest.yaml`;
  if (!exists(readmePath)) {
    addError(`${packagePath}: missing README.md`);
    continue;
  }
  if (!exists(manifestPath)) {
    addError(`${packagePath}: missing manifest.yaml`);
    continue;
  }

  const readme = readText(readmePath);
  for (const fragment of requiredFragments) {
    if (!readme.includes(fragment)) {
      addError(`${readmePath}: must mention ${fragment}`);
    }
  }

  const manifest = YAML.parse(readText(manifestPath));
  if (manifest.id !== packagePath) {
    addError(`${manifestPath}: id must be ${packagePath}`);
  }
  if (manifest.type !== "runtime") {
    addError(`${manifestPath}: type must be runtime`);
  }
}

const hermesPatchPackage =
  "docs/operations/review-pool/hermes/2026-07-15-huabaosi-wecom-server-patch";
const hermesPatchManifestPath = `${hermesPatchPackage}/manifest.yaml`;
const hermesPatchReadmePath = `${hermesPatchPackage}/README.md`;

if (!exists(hermesPatchManifestPath) || !exists(hermesPatchReadmePath)) {
  addError(`${hermesPatchPackage}: missing review-pool package contract`);
} else {
  const manifest = YAML.parse(readText(hermesPatchManifestPath));
  const patchPath = manifest.patch?.path;
  const expectedPaths = [
    "gateway/platforms/wecom.py",
    "tests/gateway/test_text_batching.py",
    "tests/gateway/test_wecom.py",
  ];

  if (manifest.id !== hermesPatchPackage) {
    addError(`${hermesPatchManifestPath}: id must be ${hermesPatchPackage}`);
  }
  if (
    manifest.status !== "review-pool" ||
    manifest.source?.disposition !== "review-pool"
  ) {
    addError(`${hermesPatchManifestPath}: server patch must remain review-pool`);
  }
  if (manifest.production_boundary?.deployable !== false) {
    addError(`${hermesPatchManifestPath}: server patch must be non-deployable`);
  }
  if (!patchPath || !exists(patchPath)) {
    addError(`${hermesPatchManifestPath}: patch path is missing`);
  } else {
    const patch = fs.readFileSync(path.join(repoRoot, patchPath));
    const actualSha256 = crypto.createHash("sha256").update(patch).digest("hex");
    if (actualSha256 !== manifest.patch.sha256) {
      addError(`${patchPath}: SHA-256 does not match manifest`);
    }

    const patchEntries = extractPatchEntries(patch.toString("utf8"));
    if (!patchEntriesMatchAllowedPaths(patchEntries, expectedPaths)) {
      addError(`${patchPath}: contains paths outside the reviewed WeCom scope`);
    }
    if (manifest.patch.allowed_paths?.join("\n") !== expectedPaths.join("\n")) {
      addError(`${hermesPatchManifestPath}: allowed paths do not match patch scope`);
    }
  }

  const readme = readText(hermesPatchReadmePath);
  for (const fragment of ["review evidence", "not included in the deploy bundle"]) {
    if (!readme.includes(fragment)) {
      addError(`${hermesPatchReadmePath}: must mention ${fragment}`);
    }
  }
}

if (errors.length > 0) {
  console.error("Runtime contract check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log("Runtime contract check passed.");
