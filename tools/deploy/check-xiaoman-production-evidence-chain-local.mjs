#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import path from "node:path";
import process from "node:process";

const repoRoot = process.cwd();

const checks = [
  ["node", ["tools/deploy/check-deploy-contracts.mjs"]],
  ["node", ["tools/deploy/check-deploy-runner.mjs"]],
  ["node", ["tools/deploy/test-sidecar-artifact-build-boundary.mjs"]],
  ["node", ["tools/deploy/test-build-sidecar-artifact.mjs"]],
  ["node", ["tools/deploy/test-build-qiwe-production-sidecar-artifact.mjs"]],
  ["node", ["tools/deploy/test-fetch-cos-artifact-permissions.mjs"]],
  ["node", ["tools/deploy/test-fetch-staging-sidecar-artifact.mjs"]],
  ["node", ["tools/deploy/check-xiaoman-preflight-readiness.mjs"]],
  ["node", ["tools/deploy/test-xiaoman-legacy-cron-observation.mjs"]],
  ["node", ["tools/deploy/test-staging-runtime-prerequisite-observation.mjs"]],
  ["node", ["tools/deploy/test-staging-runtime-values-observation.mjs"]],
  ["node", ["tools/deploy/test-staging-runtime-env-render.mjs"]],
  ["node", ["tools/deploy/test-staging-runtime-readiness-evidence.mjs"]],
  ["node", ["tools/deploy/test-huabaosi-image-staging-readiness.mjs"]],
  ["node", ["tools/deploy/test-huabaosi-image-staging-smoke.mjs"]],
  ["node", ["tools/deploy/test-qiwe-image-staging-readiness.mjs"]],
  ["node", ["tools/deploy/test-qiwe-image-staging-smoke.mjs"]],
  ["node", ["tools/deploy/test-qiwe-image-production-observation.mjs"]],
  ["node", ["tools/deploy/test-qiwe-image-production-activation.mjs"]],
  ["node", ["tools/deploy/test-qiwe-image-callback-bridge-production-observation.mjs"]],
  ["node", ["tools/deploy/test-qiwe-image-callback-bridge-production-activation.mjs"]],
  ["node", ["tools/deploy/test-huabaosi-image-production-canary.mjs"]],
  ["node", ["tools/deploy/test-huabaosi-image-production-canary-evidence.mjs"]],
  ["node", ["tools/deploy/test-xiaoman-image-send-staging-evidence.mjs"]],
  ["node", ["tools/deploy/test-xiaoman-real-activity-production-evidence.mjs"]],
  ["node", ["tools/deploy/test-xiaoman-qiwe-group-arrival-confirmation-evidence.mjs"]],
  ["node", ["tools/deploy/test-xiaoman-production-completion-manifest-builder.mjs"]],
  ["node", ["tools/deploy/test-xiaoman-production-completion-evidence.mjs"]],
  ["node", ["tools/deploy/test-finalize-xiaoman-production-completion-evidence.mjs"]],
  [
    "node",
    [
      "tools/agents/pr-doctor.mjs",
      "docs/reports/2026-07-24-xiaoman-production-evidence-pr-body.md",
    ],
  ],
  [
    "cargo",
    [
      "test",
      "--manifest-path",
      "runtime/sidecar/Cargo.toml",
      "xiaoman_real_activity_evidence",
    ],
  ],
];

for (const [index, [command, args]] of checks.entries()) {
  const display = [command, ...args].join(" ");
  process.stdout.write(
    `[${String(index + 1).padStart(2, "0")}/${String(checks.length).padStart(2, "0")}] ${display}\n`
  );
  execFileSync(command, args, {
    cwd: repoRoot,
    stdio: "inherit",
  });
}

process.stdout.write(
  `Xiaoman production evidence chain local check passed: ${path.relative(repoRoot, "tools/deploy/check-xiaoman-production-evidence-chain-local.mjs")}\n`
);
