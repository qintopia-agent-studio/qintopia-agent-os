#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();

const commands = [
  ["cargo", ["check", "--manifest-path", "runtime/sidecar/Cargo.toml"]],
  [
    "cargo",
    [
      "test",
      "--manifest-path",
      "runtime/sidecar/Cargo.toml",
      "identity_alias",
      "--",
      "--nocapture",
    ],
    { env: { RUST_MIN_STACK: "33554432" } },
  ],
  [
    "cargo",
    [
      "test",
      "--manifest-path",
      "runtime/sidecar/Cargo.toml",
      "identity_bootstrap",
      "--",
      "--nocapture",
    ],
    { env: { RUST_MIN_STACK: "33554432" } },
  ],
  [
    "cargo",
    [
      "test",
      "--manifest-path",
      "runtime/sidecar/Cargo.toml",
      "context_tools",
      "--",
      "--nocapture",
    ],
    { env: { RUST_MIN_STACK: "33554432" } },
  ],
  [
    "cargo",
    [
      "test",
      "--manifest-path",
      "runtime/sidecar/Cargo.toml",
      "member_profile",
      "--",
      "--nocapture",
    ],
    { env: { RUST_MIN_STACK: "33554432" } },
  ],
  ["node", ["tools/deploy/test-erhua-room-member-sync.mjs"]],
  ["node", ["tools/deploy/test-erhua-member-recognition-production-config.mjs"]],
  [
    "node",
    ["tools/deploy/test-erhua-member-recognition-production-config-observation.mjs"],
  ],
  ["node", ["tools/deploy/test-erhua-member-recognition-coverage.mjs"]],
  ["node", ["tools/deploy/test-erhua-member-recognition-coverage-summary.mjs"]],
  ["node", ["tools/deploy/test-finalize-erhua-member-recognition-coverage.mjs"]],
  ["node", ["tools/deploy/test-erhua-member-safe-alias-payload.mjs"]],
  ["node", ["tools/deploy/test-erhua-member-safe-alias-payload-template.mjs"]],
  ["node", ["tools/deploy/test-erhua-member-safe-identity-payload.mjs"]],
  ["node", ["tools/deploy/test-erhua-member-safe-identity-payload-template.mjs"]],
  ["node", ["tools/deploy/test-erhua-member-recognition-canary.mjs"]],
  ["node", ["tools/deploy/test-erhua-member-recognition-canary-builder.mjs"]],
  ["node", ["tools/deploy/test-erhua-member-recognition-canary-mcp-input.mjs"]],
  ["node", ["tools/deploy/test-erhua-member-recognition-completion.mjs"]],
  ["node", ["tools/deploy/test-erhua-member-recognition-completion-summary.mjs"]],
  ["node", ["tools/deploy/test-finalize-erhua-member-recognition-completion.mjs"]],
  ["node", ["tools/deploy/check-deploy-contracts.mjs"]],
  ["node", ["tools/deploy/build-deploy-bundle.mjs"]],
];

const requiredBundlePaths = [
  "payload/tools/deploy/build-erhua-member-recognition-canary-evidence.mjs",
  "payload/tools/deploy/build-erhua-member-recognition-canary-mcp-input.mjs",
  "payload/tools/deploy/build-erhua-member-safe-alias-payload-template.mjs",
  "payload/tools/deploy/build-erhua-member-safe-identity-payload-template.mjs",
  "payload/tools/deploy/check-erhua-member-recognition-canary.mjs",
  "payload/tools/deploy/check-erhua-member-recognition-completion.mjs",
  "payload/tools/deploy/check-erhua-member-recognition-completion-summary.mjs",
  "payload/tools/deploy/check-erhua-member-recognition-coverage.mjs",
  "payload/tools/deploy/check-erhua-member-recognition-coverage-summary.mjs",
  "payload/tools/deploy/finalize-erhua-member-recognition-coverage.mjs",
  "payload/tools/deploy/finalize-erhua-member-recognition-completion.mjs",
  "payload/tools/deploy/check-erhua-member-safe-alias-payload.mjs",
  "payload/tools/deploy/check-erhua-member-safe-identity-payload.mjs",
  "payload/tools/deploy/check-erhua-room-member-sync.mjs",
  "payload/deploy/sidecar/scripts/apply-erhua-member-recognition-production-config.sh",
  "payload/deploy/sidecar/scripts/erhua-member-recognition-production-config-observation-smoke.sh",
  "payload/docs/operations/erhua-member-recognition-production-runbook.md",
];

for (const [command, args, options = {}] of commands) {
  const result = spawnSync(command, args, {
    cwd: repoRoot,
    encoding: "utf8",
    env: { ...process.env, ...(options.env ?? {}) },
    stdio: "inherit",
  });
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

const manifestPath = path.join(
  repoRoot,
  "dist/deploy-bundles/qintopia-agent-os-deploy-bundle/artifact-manifest.json"
);
const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
const bundlePaths = new Set(manifest.files.map((file) => file.path));
const missing = requiredBundlePaths.filter((requiredPath) => {
  return !bundlePaths.has(requiredPath);
});

if (missing.length > 0) {
  console.error("Erhua member recognition deploy bundle is missing required files:");
  for (const item of missing) {
    console.error(`- ${item}`);
  }
  process.exit(1);
}

console.log(
  `Erhua member recognition local check passed: ${requiredBundlePaths.length} release-current files present.`
);
