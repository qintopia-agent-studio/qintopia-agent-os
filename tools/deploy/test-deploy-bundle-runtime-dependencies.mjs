#!/usr/bin/env node

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const bundleRoot = path.join(
  repoRoot,
  "dist",
  "deploy-bundles",
  "qintopia-agent-os-deploy-bundle"
);
const payloadRoot = path.join(bundleRoot, "payload");
const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "deploy-bundle-runtime-"));

try {
  const build = spawnSync(
    process.execPath,
    [path.join(repoRoot, "tools", "deploy", "build-deploy-bundle.mjs")],
    { cwd: repoRoot, encoding: "utf8" }
  );
  assert.equal(build.status, 0, build.stderr);

  const yamlRoot = path.join(payloadRoot, "node_modules", "yaml");
  const yamlPackage = JSON.parse(
    fs.readFileSync(path.join(yamlRoot, "package.json"), "utf8")
  );
  assert.equal(yamlPackage.name, "yaml");
  assert.equal(yamlPackage.version, "2.9.0");
  assert.deepEqual(yamlPackage.dependencies ?? {}, {});
  assertRegularTree(yamlRoot);

  const manifest = JSON.parse(
    fs.readFileSync(path.join(bundleRoot, "artifact-manifest.json"), "utf8")
  );
  assert.deepEqual(manifest.validation.runtime_node_dependencies, [
    { name: "yaml", version: "2.9.0", dependencies: [] },
  ]);
  assert.ok(
    manifest.files.some(
      (entry) =>
        entry.path === "payload/node_modules/yaml/package.json" &&
        entry.source_path === "dependency:yaml@2.9.0/package.json"
    )
  );
  for (const relativePath of [
    "runtime/hermes/core-release-contracts/transaction-journal.schema.json",
    "tools/deploy/run-hermes-core-release-transaction.mjs",
    "tools/deploy/run-hermes-core-release-production.mjs",
    "deploy/runner/run-hermes-core-release.sh",
    "deploy/runner/install-hermes-core-systemd-units.sh",
    "deploy/runner/qintopia-hermes-core-launcher",
  ]) {
    assert.ok(
      manifest.files.some((entry) => entry.path === `payload/${relativePath}`),
      `${relativePath} must be present in the deploy bundle`
    );
  }

  const isolatedEnvironment = {
    PATH: process.env.PATH ?? "",
    HOME: tmpRoot,
    NODE_PATH: "",
    LANG: "C",
  };
  const registry = runPayloadTool(
    "tools/deploy/hermes-profile-registry.mjs",
    ["--services"],
    isolatedEnvironment
  );
  assert.equal(registry.status, 0, registry.stderr);
  assert.equal(registry.stdout.trim().split("\n").length, 7);

  const readiness = runPayloadTool(
    "tools/deploy/check-hermes-wecom-readiness.mjs",
    ["--staging-home", path.join(tmpRoot, "missing-staging-home")],
    isolatedEnvironment
  );
  assert.notEqual(readiness.status, 0);
  const readinessOutput = `${readiness.stdout}\n${readiness.stderr}`;
  assert.match(readinessOutput, /hermes_wecom_readiness=blocked/);
  assert.doesNotMatch(readinessOutput, /ERR_MODULE_NOT_FOUND/);

  const parity = runPayloadTool(
    "tools/deploy/check-hermes-wecom-parity.mjs",
    [],
    isolatedEnvironment
  );
  assert.equal(parity.status, 2);
  const parityOutput = `${parity.stdout}\n${parity.stderr}`;
  assert.match(parityOutput, /hermes_wecom_parity=blocked/);
  assert.doesNotMatch(parityOutput, /ERR_MODULE_NOT_FOUND/);

  for (const [relativePath, marker] of [
    [
      "tools/deploy/plan-hermes-core-release.mjs",
      "hermes_core_release_error=invalid_invocation",
    ],
    [
      "tools/deploy/stage-hermes-core-release.mjs",
      "hermes_core_candidate_staging_error=invalid_invocation",
    ],
    [
      "tools/deploy/bootstrap-hermes-core-root.mjs",
      "hermes_core_bootstrap_error=invalid_invocation",
    ],
  ]) {
    const result = runPayloadTool(relativePath, [], isolatedEnvironment);
    assert.equal(result.status, 2, result.stderr);
    const output = `${result.stdout}\n${result.stderr}`;
    assert.match(output, new RegExp(marker));
    assert.doesNotMatch(output, /ERR_MODULE_NOT_FOUND/);
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Deploy bundle runtime dependency test passed.");

function runPayloadTool(relativePath, args, env) {
  return spawnSync(
    process.execPath,
    [path.join(payloadRoot, ...relativePath.split("/")), ...args],
    { cwd: tmpRoot, encoding: "utf8", env }
  );
}

function assertRegularTree(root) {
  const visit = (directoryPath) => {
    for (const name of fs.readdirSync(directoryPath)) {
      const entryPath = path.join(directoryPath, name);
      const metadata = fs.lstatSync(entryPath);
      assert.equal(metadata.isSymbolicLink(), false, `${name} must not be a symlink`);
      if (metadata.isDirectory()) {
        visit(entryPath);
      } else {
        assert.equal(metadata.isFile(), true, `${name} must be a regular file`);
      }
    }
  };
  visit(root);
}
